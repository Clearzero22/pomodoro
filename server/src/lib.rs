//! 番茄钟云端后端（库入口）。
//!
//! `main.rs` 是可执行入口，这里把核心逻辑暴露成库，方便集成测试
//! 在进程内跑完整的 HTTP 栈，不需要真的起一个端口。

pub mod auth;
pub mod models;

use axum::{
    Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post, put},
};
use serde::Deserialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use auth::AuthUser;
use models::{CreateRecordRequest, DailyStat, PomodoroRecord, UpdateRecordRequest};

/// 应用状态。
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub jwt_secret: String,
}

impl AppState {
    /// 访问令牌有效期：15 分钟。
    pub const ACCESS_TTL_SECS: i64 = 15 * 60;
    /// 刷新令牌有效期：7 天。
    pub const REFRESH_TTL_SECS: i64 = 7 * 24 * 3600;
}

/// API 错误。
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    pub fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: "unauthorized".into(),
        }
    }
    pub fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: "not found".into(),
        }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            axum::Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// 打开数据库连接池并建表。
pub async fn init_db(url: &str) -> anyhow::Result<SqlitePool> {
    use std::str::FromStr;

    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        // WAL 让读写不互相阻塞：前端 GET 不被 POST 卡住。
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::raw_sql(
        r#"
        create table if not exists records (
            id            text primary key,
            user_id       text not null,
            title         text,
            phase         text not null,
            duration_secs integer not null,
            completed_at  integer not null,
            created_at    integer not null
        );

        create index if not exists idx_records_user_completed
            on records(user_id, completed_at);

        create table if not exists users (
            id            text primary key,
            email         text not null unique,
            password_hash text not null,
            created_at    integer not null
        );
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

fn row_to_record(row: &SqliteRow) -> PomodoroRecord {
    PomodoroRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        title: row.get("title"),
        phase: row
            .get::<String, _>("phase")
            .parse()
            .unwrap_or(models::Phase::Focus),
        duration_secs: row.get::<i64, _>("duration_secs") as u32,
        completed_at: row.get("completed_at"),
        created_at: row.get("created_at"),
    }
}

async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    from: Option<i64>,
    to: Option<i64>,
}

async fn list_records(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<ListQuery>,
) -> ApiResult<axum::Json<Vec<PomodoroRecord>>> {
    let rows = sqlx::query(
        "select id, user_id, title, phase, duration_secs, completed_at, created_at
         from records
         where user_id = ?
           and (? is null or completed_at >= ?)
           and (? is null or completed_at <= ?)
         order by completed_at desc",
    )
    .bind(&user.user_id)
    .bind(q.from)
    .bind(q.from)
    .bind(q.to)
    .bind(q.to)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(axum::Json(rows.iter().map(row_to_record).collect()))
}

async fn create_record(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    axum::Json(req): axum::Json<CreateRecordRequest>,
) -> ApiResult<(StatusCode, axum::Json<PomodoroRecord>)> {
    if req.duration_secs == 0 || req.duration_secs > 24 * 3600 {
        return Err(ApiError::bad_request("duration_secs out of range"));
    }
    if req.id.trim().is_empty() {
        return Err(ApiError::bad_request("id required"));
    }

    let now = chrono::Utc::now().timestamp();
    // 主键和完成时间由客户端给出，保证两端一致；id 重复时当幂等处理。
    let completed_at = if req.completed_at > 0 {
        req.completed_at
    } else {
        now
    };
    let id = req.id.clone();

    sqlx::query(
        "insert into records (id, user_id, title, phase, duration_secs, completed_at, created_at)
         values (?, ?, ?, ?, ?, ?, ?)
         on conflict(id) do update set
            title = excluded.title,
            phase = excluded.phase,
            duration_secs = excluded.duration_secs,
            completed_at = excluded.completed_at",
    )
    .bind(&id)
    .bind(&user.user_id)
    .bind(&req.title)
    .bind(req.phase.as_str())
    .bind(req.duration_secs as i64)
    .bind(completed_at)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    let record = PomodoroRecord {
        id,
        user_id: user.user_id.clone(),
        title: req.title,
        phase: req.phase,
        duration_secs: req.duration_secs,
        completed_at,
        created_at: now,
    };
    Ok((StatusCode::CREATED, axum::Json(record)))
}

async fn update_record(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<String>,
    axum::Json(req): axum::Json<UpdateRecordRequest>,
) -> ApiResult<axum::Json<PomodoroRecord>> {
    let result = sqlx::query("update records set title = ? where id = ? and user_id = ?")
        .bind(&req.title)
        .bind(&id)
        .bind(&user.user_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }

    let row = sqlx::query(
        "select id, user_id, title, phase, duration_secs, completed_at, created_at
         from records where id = ?",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(axum::Json(row_to_record(&row)))
}

async fn delete_record(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query("delete from records where id = ? and user_id = ?")
        .bind(&id)
        .bind(&user.user_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct StatsQuery {
    #[serde(default = "default_days")]
    days: u32,
}

fn default_days() -> u32 {
    7
}

async fn daily_stats(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<StatsQuery>,
) -> ApiResult<axum::Json<Vec<DailyStat>>> {
    let days = q.days.clamp(1, 365);
    let since = chrono::Utc::now().timestamp() - (days as i64) * 86400;

    let rows = sqlx::query(
        "select completed_at, duration_secs from records
         where user_id = ? and completed_at >= ?",
    )
    .bind(&user.user_id)
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    // 按天聚合。用 UTC 日期分组，和客户端 `date_str()` 保持一致。
    let mut buckets: std::collections::BTreeMap<String, DailyStat> =
        std::collections::BTreeMap::new();
    for row in rows {
        let ts: i64 = row.get("completed_at");
        let secs: i64 = row.get("duration_secs");
        let date = chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "1970-01-01".to_string());
        let entry = buckets.entry(date.clone()).or_insert_with(|| DailyStat {
            date,
            completed: 0,
            focus_minutes: 0,
        });
        entry.completed += 1;
        entry.focus_minutes += (secs / 60) as u32;
    }

    Ok(axum::Json(buckets.into_values().collect()))
}

/// 构造路由。
pub fn router(state: AppState) -> Router {
    // 开发期放开 CORS；生产应收紧到具体域名。
    let cors = CorsLayer::permissive();

    let protected = Router::new()
        .route("/api/records", get(list_records).post(create_record))
        .route("/api/records/:id", put(update_record).delete(delete_record))
        .route("/api/stats/daily", get(daily_stats))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/refresh", post(auth::refresh))
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
