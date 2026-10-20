//! 桌面端 `HttpApi` 的端到端测试：对着真的后端跑完整 CRUD。
//!
//! 后端在测试进程内起（`tower::ServiceExt::oneshot`），`HttpApi` 用
//! `reqwest` 打真的 HTTP 端口，因此覆盖了序列化、状态码判定、错误映射。
//!
//! 运行：`cargo test -p pomodoro-desktop`

use pomodoro::api::{Api, ApiError};
use pomodoro::model::CreateRecordRequest;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// 起一个真的后端，返回地址和 token。
async fn spawn_server() -> (SocketAddr, String) {
    use pomodoro_server::{AppState, init_db, router};

    // 每个测试一个独立数据库：同邮箱注册两次会撞 UNIQUE 约束。
    let dir = std::env::temp_dir().join(format!("pomodoro-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}/test.db", dir.display());

    let pool = init_db(&url).await.unwrap();
    let state = AppState {
        db: pool,
        jwt_secret: "e2e-secret".into(),
    };
    let app = router(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // 登录拿 token
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/api/auth/login"))
        .json(&serde_json::json!({
            "email": "e2e@example.com",
            "password": "e2e-pass"
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    assert!(
        status.is_success(),
        "login failed: {status} body={text}"
    );
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    let token = body["token"]
        .as_str()
        .unwrap_or_else(|| panic!("no token in response: {text}"))
        .to_string();

    (addr, token)
}

fn api(addr: SocketAddr, token: &str) -> pomodoro_desktop::api::HttpApi {
    pomodoro_desktop::api::HttpApi::new(format!("http://{addr}"), Some(token.to_string()))
}

#[tokio::test]
async fn http_api_crud_roundtrip() {
    let (addr, token) = spawn_server().await;
    let api = api(addr, &token);

    // CREATE
    let id = uuid::Uuid::new_v4().to_string();
    let created = api
        .create(CreateRecordRequest {
            id: id.clone(),
            title: Some("写文档".into()),
            phase: pomodoro::Phase::Focus,
            duration_secs: 1500,
            completed_at: 0,
        })
        .await
        .unwrap();
    assert_eq!(created.id, id, "服务端必须原样保存客户端的 id");
    assert_eq!(created.title.as_deref(), Some("写文档"));

    // READ
    let list = api.list(None, None).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, id);

    // UPDATE
    let updated = api
        .update(
            &id,
            pomodoro::model::UpdateRecordRequest {
                title: Some("改过的".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.title.as_deref(), Some("改过的"));

    // STATS
    let stats = api.get_stats(7).await.unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].completed, 1);
    assert_eq!(stats[0].focus_minutes, 25);

    // DELETE
    api.delete(&id).await.unwrap();
    assert!(api.list(None, None).await.unwrap().is_empty());

    // 再删一次应该 404
    let err = api.delete(&id).await.unwrap_err();
    assert!(matches!(err, ApiError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn http_api_without_token_is_unauthorized() {
    let (addr, _token) = spawn_server().await;
    let api = pomodoro_desktop::api::HttpApi::new(format!("http://{addr}"), None);

    let err = api.list(None, None).await.unwrap_err();
    assert!(matches!(err, ApiError::Unauthorized), "got {err:?}");
}

#[tokio::test]
async fn http_api_with_bad_token_is_unauthorized() {
    let (addr, _token) = spawn_server().await;
    let api = pomodoro_desktop::api::HttpApi::new(
        format!("http://{addr}"),
        Some("garbage-token".to_string()),
    );

    let err = api.list(None, None).await.unwrap_err();
    assert!(matches!(err, ApiError::Unauthorized), "got {err:?}");
}

#[tokio::test]
async fn http_api_unreachable_server_is_network_error() {
    // 127.0.0.1:1 上不会有服务
    let api = pomodoro_desktop::api::HttpApi::new("http://127.0.0.1:1", None);
    let err = api.list(None, None).await.unwrap_err();
    assert!(matches!(err, ApiError::Network(_)), "got {err:?}");
}
