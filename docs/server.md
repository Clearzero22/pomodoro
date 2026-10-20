# 云端后端

番茄钟数据的云端存储。**Desktop / Web / Android / iOS 四端连的是同一个后端**，
数据按用户隔离。

## 快速开始

```sh
# 启动（默认 127.0.0.1:3000，SQLite 文件 ./pomodoro.db）
make server

# 或手动
cd server
cargo run -p pomodoro-server
```

环境变量：

| 变量 | 默认 | 说明 |
|---|---|---|
| `DATABASE_URL` | `sqlite://pomodoro.db` | 数据库连接串。Postgres 用 `postgres://user:pass@host/db` |
| `JWT_SECRET` | `dev-secret-change-me` | 令牌签名密钥。**生产必须显式设置** |
| `BIND` | `127.0.0.1:3000` | 监听地址 |

## 接口

```text
GET    /health                 → {"status":"ok"}          （公开）

POST   /api/auth/login         {email, password} → {token, refresh_token, user_id, email}
POST   /api/auth/refresh       {refresh_token}   → {token, ...}

GET    /api/records?from=&to=  → Record[]         （需鉴权）
POST   /api/records            CreateRequest     → Record
PUT    /api/records/:id        UpdateRequest     → Record
DELETE /api/records/:id        → 204
GET    /api/stats/daily?days=7 → DailyStat[]
```

鉴权用 `Authorization: Bearer <token>`。访问令牌 15 分钟过期，刷新令牌 7 天。

`POST /api/auth/login` 在邮箱不存在时会**自动注册**——单用户阶段省事，
正式多用户时应该拆出独立的 `/api/auth/register`。

## 数据模型

```json
{
  "id": "uuid-v4",
  "user_id": "uuid-v4",
  "title": "写文档",
  "phase": "focus",
  "duration_secs": 1500,
  "completed_at": 1792514654,
  "created_at": 1792514654
}
```

**`id` 和 `completed_at` 由客户端生成并上传，服务端原样保存。**
这样本地记录和云端记录主键一致，同步时才能把两边对上——否则两端各生成
一个 id，合并时会变成两条重复记录。

`phase` 的线上格式是 `"focus"` / `"break"`（小写），客户端
`crates/pomodoro/src/lib.rs` 的 `Phase` 用 `serde(rename_all = "lowercase")`
保证一致。

## 客户端怎么连

```rust
// Desktop
let store = Store::new(
    Some(HttpApi::new("http://127.0.0.1:3000", Some(token))),
    FilePersistence::new(),
);

// Web
let store = Store::new(
    Some(FetchApi::new("http://127.0.0.1:3000", Some(token))),
    LocalStoragePersistence::new(),
);
```

Web 端要设 `VITE_API_URL` 环境变量才启用联网模式：

```sh
VITE_API_URL=http://127.0.0.1:3000 bun run build
```

没设就是纯本地模式（数据在 `localStorage`），界面完全一样，只是不上云。

## 测试

```sh
make server-test          # 或 cargo test -p pomodoro-server
```

8 个集成测试跑完整的 HTTP 栈：健康检查、鉴权失败、完整 CRUD、
404、参数校验、密码错误、用户隔离。

桌面端还有一组**对着真后端**的端到端测试：

```sh
cargo test -p pomodoro-desktop
```

## 部署

```sh
# Dockerfile
FROM rust:1.90 AS build
WORKDIR /app
COPY . .
RUN cargo build --release -p pomodoro-server

FROM debian:stable-slim
COPY --from=build /app/target/release/pomodoro-server /usr/local/bin/
ENV BIND=0.0.0.0:3000
EXPOSE 3000
CMD ["pomodoro-server"]
```

SQLite 需要挂持久卷：

```sh
docker run -d -p 3000:3000 \
  -v pomodoro-data:/data \
  -e DATABASE_URL=sqlite:///data/pomodoro.db \
  -e JWT_SECRET=<你的密钥> \
  pomodoro-server
```

**生产环境注意：**
1. `JWT_SECRET` 必须换成随机长密钥
2. CORS 现在是 `permissive()`，上线前收紧到具体域名
3. 换 Postgres：改 `DATABASE_URL` + 表结构（SQLite → Postgres 的类型差异很小）

## 数据表

```sql
create table records (
    id            text primary key,
    user_id       text not null,
    title         text,
    phase         text not null,
    duration_secs integer not null,
    completed_at  integer not null,
    created_at    integer not null
);
create index idx_records_user_completed on records(user_id, completed_at);

create table users (
    id            text primary key,
    email         text not null unique,
    password_hash text not null,
    created_at    integer not null
);
```

密码用 **Argon2** 哈希，不存明文。
