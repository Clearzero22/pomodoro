//! 桌面端的联网 `Api` 实现：用 `reqwest` 访问后端。
//!
//! 与 Web 端唯一的区别是 HTTP 客户端和 JSON 解析方式，其余（URL 拼装、
//! 状态码判定、错误映射）完全一致，因此两端的同步行为可以逐条对照。

use async_trait::async_trait;
use pomodoro::api::{Api, ApiError, ApiResult};
use pomodoro::model::{CreateRecordRequest, DailyStat, PomodoroRecord, UpdateRecordRequest};

/// 基于 `reqwest` 的客户端。
#[derive(Clone)]
pub struct HttpApi {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl HttpApi {
    /// 新建客户端。`base_url` 不带尾部斜杠。
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
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

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.request(method, self.url(path));
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        req
    }
}

/// 把 HTTP 状态码映射成 `ApiError`。
fn status_error(status: reqwest::StatusCode, body: &str) -> ApiError {
    match status.as_u16() {
        401 | 403 => ApiError::Unauthorized,
        404 => ApiError::NotFound(body.to_string()),
        _ => ApiError::Server(format!("{status}: {body}")),
    }
}

#[async_trait(?Send)]
impl Api for HttpApi {
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

        let resp = self
            .request(reqwest::Method::GET, &path)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(status_error(status, &text));
        }
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

        let resp = self
            .request(reqwest::Method::POST, "/api/records")
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(status_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Server(e.to_string()))
    }

    async fn update(&self, id: &str, req: UpdateRecordRequest) -> ApiResult<PomodoroRecord> {
        let body = serde_json::json!({ "title": req.title });

        let resp = self
            .request(reqwest::Method::PUT, &format!("/api/records/{id}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(status_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Server(e.to_string()))
    }

    async fn delete(&self, id: &str) -> ApiResult<()> {
        let resp = self
            .request(reqwest::Method::DELETE, &format!("/api/records/{id}"))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(status_error(status, &text));
        }
        Ok(())
    }

    async fn get_stats(&self, days: u32) -> ApiResult<Vec<DailyStat>> {
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/api/stats/daily?days={}", days.clamp(1, 365)),
            )
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(status_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Server(e.to_string()))
    }
}
