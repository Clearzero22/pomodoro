//! 认证：密码哈希（Argon2）+ JWT 访问令牌 + 刷新令牌。
//!
//! 单用户阶段的用法：第一次启动时用 `POST /api/auth/register` 或
//! `POST /api/auth/login`（邮箱不存在则自动注册并用该密码登录）。
//!
//! 令牌结构：
//! ```text
//! access_token  15 分钟，claims: { sub, email, exp, typ: "access" }
//! refresh_token 7 天，  claims: { sub, email, exp, typ: "refresh" }
//! ```

use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString},
};
use rand_core::OsRng;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::{AppState, ApiError, ApiResult};

/// JWT 载荷。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claims {
    /// 用户 id。
    pub sub: String,
    /// 邮箱。
    pub email: String,
    /// 过期时间（unix 秒）。
    pub exp: i64,
    /// 令牌类型：`access` 或 `refresh`。
    pub typ: String,
}

/// 认证通过的调用方。
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user_id: String,
    pub email: String,
}

/// 登录请求。
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// 登录响应。
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub refresh_token: String,
    pub user_id: String,
    pub email: String,
}

// ---------------------------------------------------------------------------
// 密码
// ---------------------------------------------------------------------------

/// 生成 Argon2 哈希。
pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::internal(format!("hash failed: {e}")))
}

/// 校验密码。
pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

// ---------------------------------------------------------------------------
// 令牌
// ---------------------------------------------------------------------------

/// 签发令牌。
fn issue(secret: &str, user_id: &str, email: &str, typ: &str, ttl_secs: i64) -> Result<String, ApiError> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        exp: now + ttl_secs,
        typ: typ.to_string(),
    };
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::internal(format!("encode failed: {e}")))
}

/// 解析并校验令牌。
pub fn decode(secret: &str, token: &str, expect_type: &str) -> Result<Claims, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| ApiError::unauthorized())?;

    if data.claims.typ != expect_type {
        return Err(ApiError::unauthorized());
    }
    Ok(data.claims)
}

// ---------------------------------------------------------------------------
// 路由
// ---------------------------------------------------------------------------

/// 登录。邮箱不存在时自动注册（单用户阶段的便利做法）。
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<TokenResponse>> {
    let email = req.email.trim().to_lowercase();
    if email.is_empty() || req.password.is_empty() {
        return Err(ApiError::bad_request("email and password required"));
    }

    // 查用户
    let existing: Option<(String, String)> = sqlx::query_as(
        "select id, password_hash from users where email = ?",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    let user_id = match existing {
        Some((id, hash)) => {
            if !verify_password(&req.password, &hash) {
                return Err(ApiError::unauthorized());
            }
            id
        }
        None => {
            // 邮箱不存在 → 当作注册。用随机 id。
            let id = uuid::Uuid::new_v4().to_string();
            let hash = hash_password(&req.password)?;
            sqlx::query("insert into users (id, email, password_hash, created_at) values (?, ?, ?, ?)")
                .bind(&id)
                .bind(&email)
                .bind(&hash)
                .bind(chrono::Utc::now().timestamp())
                .execute(&state.db)
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?;
            id
        }
    };

    let token = issue(
        &state.jwt_secret,
        &user_id,
        &email,
        "access",
        AppState::ACCESS_TTL_SECS,
    )?;
    let refresh_token = issue(
        &state.jwt_secret,
        &user_id,
        &email,
        "refresh",
        AppState::REFRESH_TTL_SECS,
    )?;

    Ok(Json(TokenResponse {
        token,
        refresh_token,
        user_id,
        email,
    }))
}

/// 刷新请求。
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// 用刷新令牌换一个新的访问令牌。
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<TokenResponse>> {
    let claims = decode(&state.jwt_secret, &req.refresh_token, "refresh")?;
    let token = issue(
        &state.jwt_secret,
        &claims.sub,
        &claims.email,
        "access",
        AppState::ACCESS_TTL_SECS,
    )?;
    Ok(Json(TokenResponse {
        token,
        refresh_token: req.refresh_token,
        user_id: claims.sub,
        email: claims.email,
    }))
}

/// 鉴权中间件：从 `Authorization: Bearer <token>` 取用户，
/// 校验通过后把 `AuthUser` 放进请求扩展，handler 用 `Extension<AuthUser>` 取。
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: axum::extract::Request,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(&request) else {
        return ApiError::unauthorized().into_response();
    };
    let claims = match decode(&state.jwt_secret, token, "access") {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };

    request.extensions_mut().insert(AuthUser {
        user_id: claims.sub,
        email: claims.email,
    });

    next.run(request).await
}

/// 从请求头取 Bearer 令牌。
fn bearer_token(request: &axum::extract::Request) -> Option<&str> {
    let header = request.headers().get(axum::http::header::AUTHORIZATION)?;
    let value = header.to_str().ok()?;
    value.strip_prefix("Bearer ").map(str::trim).filter(|s| !s.is_empty())
}

/// 让 `StatusCode` 引用不产生未使用告警。
#[allow(dead_code)]
fn _unused(_: StatusCode) {}
