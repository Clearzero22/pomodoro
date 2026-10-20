//! 浏览器端的联网 `Api` 实现：用 `fetch` 访问后端。
//!
//! 不用 `reqwest`：它在 WASM 上体积大且依赖 `wasm-bindgen` 的额外特性，
//! `gloo-net` 更轻。URL 拼装、状态码判定、错误映射与桌面端完全一致，
//! 因此两端同步行为可以逐条对照。
//!
//! `gloo-net 0.6` 的调用形状：
//! - `RequestBuilder::build()` → `Result<Request, Error>`（GET/DELETE）
//! - `RequestBuilder::json(&value)` → `Result<Request, Error>`（POST/PUT）
//! - `Request::send()` → `Future<Result<Response, Error>>`

use async_trait::async_trait;
use pomodoro::api::{Api, ApiError, ApiResult};
use pomodoro::model::{CreateRecordRequest, DailyStat, PomodoroRecord, UpdateRecordRequest};

/// 基于 `fetch` 的客户端。
#[derive(Clone)]
pub struct FetchApi {
    base_url: String,
    token: Option<String>,
}

impl FetchApi {
    /// 新建客户端。`base_url` 不带尾部斜杠。
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token,
        }
    }

    /// 设置/更新访问令牌。
    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// 构造带鉴权头的请求构造器。
    fn builder(&self, method: gloo_net::http::Method, path: &str) -> gloo_net::http::RequestBuilder {
        let mut req = gloo_net::http::RequestBuilder::new(&self.url(path));
        req = req.method(method);
        req = req.header("content-type", "application/json");
        if let Some(token) = &self.token {
            req = req.header("authorization", &format!("Bearer {token}"));
        }
        req
    }
}

/// 把 HTTP 状态码映射成 `ApiError`。
fn status_error(status: u16, body: &str) -> ApiError {
    match status {
        401 | 403 => ApiError::Unauthorized,
        404 => ApiError::NotFound(body.to_string()),
        _ => ApiError::Server(format!("{status}: {body}")),
    }
}

/// 把 `gloo-net` 的响应转成 `ApiResult<String>`（状态码非 2xx 直接报错）。
async fn read_body(resp: gloo_net::http::Response) -> ApiResult<String> {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(status_error(status, &text));
    }
    Ok(text)
}

#[async_trait(?Send)]
impl Api for FetchApi {
    async fn list(&self, from: Option<i64>, to: Option<i64>) -> ApiResult<Vec<PomodoroRecord>> {
        let mut path = "/api/records".to_string();
        let mut params = Vec::new();
        if let Some(f) = from {
            params.push(format!("from={f}"));
        }
        if let Some(t) = to {
            params.push(format!("to={t}"));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }

        let req = self
            .builder(gloo_net::http::Method::GET, &path)
            .build()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let text = read_body(req.send().await.map_err(|e| ApiError::Network(e.to_string()))?).await?;
        serde_json::from_str(&text).map_err(|e| ApiError::Server(e.to_string()))
    }

    async fn create(&self, req: CreateRecordRequest) -> ApiResult<PomodoroRecord> {
        let body = serde_json::json!({
            "id": req.id,
            "title": req.title,
            "phase": req.phase,
            "duration_secs": req.duration_secs,
            "completed_at": req.completed_at,
        });

        let request = self
            .builder(gloo_net::http::Method::POST, "/api/records")
            .json(&body)
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let text = read_body(
            request
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?,
        )
        .await?;
        serde_json::from_str(&text).map_err(|e| ApiError::Server(e.to_string()))
    }

    async fn update(&self, id: &str, req: UpdateRecordRequest) -> ApiResult<PomodoroRecord> {
        let body = serde_json::json!({ "title": req.title });

        let request = self
            .builder(gloo_net::http::Method::PUT, &format!("/api/records/{id}"))
            .json(&body)
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let text = read_body(
            request
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?,
        )
        .await?;
        serde_json::from_str(&text).map_err(|e| ApiError::Server(e.to_string()))
    }

    async fn delete(&self, id: &str) -> ApiResult<()> {
        let req = self
            .builder(
                gloo_net::http::Method::DELETE,
                &format!("/api/records/{id}"),
            )
            .build()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let resp = req.send().await.map_err(|e| ApiError::Network(e.to_string()))?;
        let status = resp.status();
        if !(200..300).contains(&status) {
            let text = resp.text().await.unwrap_or_default();
            return Err(status_error(status, &text));
        }
        Ok(())
    }

    async fn get_stats(&self, days: u32) -> ApiResult<Vec<DailyStat>> {
        let req = self
            .builder(
                gloo_net::http::Method::GET,
                &format!("/api/stats/daily?days={}", days.clamp(1, 365)),
            )
            .build()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let text = read_body(
            req.send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?,
        )
        .await?;
        serde_json::from_str(&text).map_err(|e| ApiError::Server(e.to_string()))
    }
}
