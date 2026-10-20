//! 后端 API 集成测试。
//!
//! 每个测试用独立的临时 SQLite 文件，跑完整的 HTTP 栈：
//! 登录 → 建记录 → 列出 → 改标题 → 统计 → 删除 → 鉴权失败。
//!
//! 运行：`cargo test -p pomodoro-server`

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pomodoro_server::{AppState, init_db, router};
use serde_json::{Value, json};
use tower::ServiceExt;

use std::sync::Arc;

/// 建一个带临时数据库的 app。
async fn make_app() -> (axum::Router, Arc<()>) {
    let dir = std::env::temp_dir().join(format!(
        "pomodoro-server-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let url = format!("sqlite://{}", db_path.display());

    let pool = init_db(&url).await.unwrap();
    let state = AppState {
        db: pool,
        jwt_secret: "test-secret".to_string(),
    };
    // 保留临时目录句柄，避免测试结束时目录被提前删掉
    let keep = Arc::new(());
    (router(state), keep)
}

async fn json_body(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// 登录拿 token。
async fn login(app: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": "a@b.com", "password": "secret123" }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn health_is_public() {
    let (app, _keep) = make_app().await;
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn records_require_auth() {
    let (app, _keep) = make_app().await;
    let req = Request::builder()
        .uri("/api/records")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invalid_token_is_rejected() {
    let (app, _keep) = make_app().await;
    let req = Request::builder()
        .uri("/api/records")
        .header("authorization", "Bearer not-a-real-token")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn full_crud_cycle() {
    let (app, _keep) = make_app().await;
    let token = login(&app).await;

    // CREATE
    let req = Request::builder()
        .method("POST")
        .uri("/api/records")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "id": "11111111-2222-3333-4444-555555555555",
                "title": "写文档",
                "phase": "focus",
                "duration_secs": 1500,
                "completed_at": 0
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created = json_body(resp).await;
    let id = created["id"].as_str().unwrap().to_string();
    // 服务端必须原样保存客户端给的 id，否则两端对不上
    assert_eq!(id, "11111111-2222-3333-4444-555555555555");
    assert_eq!(created["title"], "写文档");
    assert_eq!(created["phase"], "focus");
    assert_eq!(created["duration_secs"], 1500);

    // READ (list)
    let req = Request::builder()
        .uri("/api/records")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list = json_body(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    // UPDATE
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/records/{id}"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({ "title": "改过的标题" }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let updated = json_body(resp).await;
    assert_eq!(updated["title"], "改过的标题");
    assert_eq!(updated["id"], id.as_str());

    // STATS
    let req = Request::builder()
        .uri("/api/stats/daily?days=7")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let stats = json_body(resp).await;
    let arr = stats.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["completed"], 1);
    assert_eq!(arr[0]["focus_minutes"], 25);

    // DELETE
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/records/{id}"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 再列一次，应该是空的
    let req = Request::builder()
        .uri("/api/records")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let list = json_body(resp).await;
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn deleting_a_missing_record_returns_404() {
    let (app, _keep) = make_app().await;
    let token = login(&app).await;
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/records/does-not-exist")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_duration_is_rejected() {
    let (app, _keep) = make_app().await;
    let token = login(&app).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/records")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "id": "99999999-8888-7777-6666-555555555555",
                "title": null,
                "phase": "focus",
                "duration_secs": 0,
                "completed_at": 0
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn wrong_password_is_rejected() {
    let (app, _keep) = make_app().await;
    // 先注册
    let _ = login(&app).await;
    // 再用错误密码登录
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": "a@b.com", "password": "wrong" }).to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_are_isolated_from_each_other() {
    let (app, _keep) = make_app().await;
    let token_a = login(&app).await;

    // 第二个用户
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": "c@d.com", "password": "other" }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = json_body(resp).await;
    let token_b = body["token"].as_str().unwrap().to_string();

    // A 建一条
    let req = Request::builder()
        .method("POST")
        .uri("/api/records")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token_a}"))
        .body(Body::from(
            json!({
                "id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
                "title": "A的",
                "phase": "focus",
                "duration_secs": 1500,
                "completed_at": 0
            })
            .to_string(),
        ))
        .unwrap();
    app.clone().oneshot(req).await.unwrap();

    // B 列表应该是空的
    let req = Request::builder()
        .uri("/api/records")
        .header("authorization", format!("Bearer {token_b}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let list = json_body(resp).await;
    assert!(list.as_array().unwrap().is_empty());
}
