# 009 · 联网数据 CRUD + 云端同步

- **状态**: 🟡 进行中 (M0 ✅ / M1 ✅ / M2 ✅ / M3 ✅ / M4 ⬜)
- **优先级**: P0 (用户指定，覆盖原 M1)
- **分支**: `feat/009-cloud-crud`
- **依赖**: 003 持久化 (可并行，先内存版)
- **预估**: 3-3.5 天 (M0 1天 + M1 1天 + M2 1天 + 联调0.5天)
- **可行性**: 桌面/Web/PWA 5.0 / Capacitor壳 4.5 / 原生GPUI 1.5(不做)

## 进度

| 阶段 | 状态 | 说明 |
|---|---|---|
| **M0 本地CRUD** | ✅ 完成 | `model.rs`/`api.rs`/`store.rs` + 增删改查UI + 35 测试 |
| **M1 后端** | ✅ 完成 | `server/` Axum + SQLite + JWT，8 集成测试 |
| **M2 联调** | ✅ 完成 | `HttpApi`(reqwest) + `FetchApi`(gloo-net) + 离线队列 + 4 端到端测试 |
| **M3 持久化** | ✅ 完成 | `FilePersistence`(桌面) + `LocalStoragePersistence`(Web) |
| **M4 收尾** | ⬜ 待做 | 登录界面、Token 刷新、统计页接入 |

---

## 1. 背景与目标

当前 `Pomodoro { completed: u32 }` 仅内存，关掉即丢。目标是将**产生的番茄数据联网**，实现**增删改查+展示**，四端(Desktop/Web/Android/iOS)共享同一份数据，离线可用、联网自动同步。

**默认数据范围**: 番茄历史记录 (若需任务清单，把 `Record` 换 `Task` 即可，结构同构)
```rust
// crates/pomodoro/src/model.rs (与 server 共享 serde)
struct PomodoroRecord {
  id: String,              // uuid v4
  user_id: String,         // 单用户阶段 hardcode "local"，多用户后填 JWT sub
  title: Option<String>,   // 这次专注做什么
  phase: Phase,            // Focus/Break
  duration_secs: u32,      // 实际用时
  completed_at: i64,       // unix秒
  created_at: i64,
}
struct DailyStat { date: String, completed: u32, focus_minutes: u32 }
```

---

## 2. 架构总览 (Offline First)

```
┌──────────────────────────────────────────────┐
│ 四端同一份 Rust 核心 (零 #[cfg])              │
│ crates/pomodoro/src/{model,store,api}.rs     │ ← CRUD状态机+离线队列+乐观更新
│  UI: RecordsView (Button/Input/Tag 列表)     │
├──────────────────────────────────────────────┤
│ 平台注入层 (各端2行)                          │
│ Desktop: ReqwestApi + file token/keyring     │
│ Web: FetchApi(gloo-net) + localStorage token │
│ Mobile(Capacitor): 同Web (https://localhost) │
└──────────────┬───────────────────────────────┘
               │ HTTPS + JWT (短期15m + refresh 7d)
               ▼
┌──────────────────────────────┐
│ server/ (新增 Axum)          │
│ GET    /api/records          │
│ POST   /api/records          │
│ PUT    /api/records/:id      │
│ DELETE /api/records/:id      │
│ GET    /api/stats/daily      │
│ POST   /api/auth/login|refresh│
│ SQLite(起步) → Postgres(后期) │
└──────────────────────────────┘
```

**铁律**: 离线先写本地 + 入 `pending_queue`，`background_executor` 空闲时 `sync()`，断网不卡UI，`ticker 200ms` 思路复用。

---

## 3. 必须考虑的 10 项 (可行性分析浓缩)

| # | 项 | 必须怎么处理 | 不处理后果 |
|---|---|---|---|
| 1 | **WASM网络** | `gloo-net 0.6` 或 `reqwest wasm` 底层走 `fetch`，别用 `std::net` | 编译不过 |
| 2 | **CORS** | `server` 加 `tower-http::CorsLayer`，开发 `permissive()`，生产 `allow_origin([app域])` | Web 403 |
| 3 | **`file://`陷阱** | 用 `Capacitor` (Android `https://localhost` / iOS `capacitor://localhost`)，别手写 `file://` | `wasm fetch` 被拦 |
| 4 | **时钟** | 复用 `instant::Instant`，`server` 用 `chrono::Utc::now()` | WASM panic |
| 5 | **后台暂停** | 写入本地队列，前台恢复补发；`TimerEvent::PhaseCompleted` 自动 `store.create()` | 后台丢数据 |
| 6 | **存储分层** | `Desktop→dirs::config_dir()`，`Web→localStorage("pomodoro:records")`，`store` 只认 `trait Persistence` | 四端存储分裂 |
| 7 | **认证** | `JWT + Argon2`，`Web localStorage`，`Desktop keyring/file`，短token | 泄露 |
| 8 | **包体积** | `wasm 22MB/gzip6.9MB`，`sw.js cache-first` + `gzip/brotli` + 懒加载 | 首屏白屏 |
| 9 | **安全区** | `env(safe-area-inset-*)` 传给 Rust 视图 (008 联动) | 刘海遮挡 |
| 10 | **原生GPUI坑** | `gpui-pre-mobile 0.3.4 vs 0.3.9` 不兼容，**本任务不做原生GPUI**，走WebView壳 | 数周重做 |

---

## 4. 文件与职责 (回答：Rust还是什么文件)

**结论: CRUD 100% Rust，JS仅1个文件做token透传**

```
pomodoro/
├── crates/pomodoro/src/
│   ├── model.rs        # Record/Stat + serde (前后端共享)
│   ├── api.rs          # trait Api { list/create/update/delete } + DTO
│   ├── store.rs        # Store { records, pending_queue, sync() } + 乐观更新
│   ├── lib.rs          # Pomodoro 视图 + RecordsView 列表/输入框
│   └── tests/store.rs  # CRUD无头测试
├── crates/pomodoro-desktop/src/main.rs  # impl Api for ReqwestApi
├── crates/pomodoro-web/src/
│   ├── lib.rs          # impl Api for FetchApi + localStorage Persistence
│   └── www/src/auth.js # 唯一JS: login页 → localStorage.token
├── server/             # 新增后端
│   ├── Cargo.toml      # axum, tokio, sqlx, jsonwebtoken, tower-http
│   ├── src/{main.rs, routes/records.rs, routes/auth.rs, db.rs, auth.rs}
│   └── migrations/001_records.sql
└── mobile/             # 0改动，capacitor sync 自动同步 www/dist
```

**各端注入示例 (2行):**
```rust
// desktop
let api = ReqwestApi::new(env!("API_URL"), token);
cx.new(|cx| Pomodoro::new_with_api(window, cx, api, FilePersistence::new()))

// web
let api = FetchApi::new(env!("API_URL"), token);
```

---

## 5. 后端 API 契约

> 实际实现见 [`docs/server.md`](../docs/server.md)。

### 实际实现与原设计的差异

| 项 | 原设计 | 实际 |
|---|---|---|
| Api trait | `async fn` 直接用 | `async_trait(?Send)`，WASM 单线程需要 |
| Store 泛型 | `Store<A: Api, P: Persistence>` | `Store` 内部用 `Rc<dyn Api>`/`Rc<dyn Persistence>`，平台注入不影响视图 |
| 记录 id | server 生成 | **客户端生成并上传**，服务端原样保存（否则合并出重复） |
| phase 序列化 | 默认 `"Focus"` | `rename_all = "lowercase"` → `"focus"`，与服务端对齐 |
| 同步写法 | `store.sync()` 持 `&mut self` | 拆成 `push_pending`/`pull_remote`（无 `&mut`）+ `apply_push`/`merge_remote`（写回） |

> 后两条是实际跑起来才发现的 bug，原因已记在 `docs/server.md`。

```
POST   /api/auth/login          {email, password} -> {token, refresh_token}
POST   /api/auth/refresh        {refresh_token} -> {token}
GET    /api/records?from=&to=   -> Record[]
POST   /api/records             Record -> Record (server生成id/时间)
PUT    /api/records/:id         {title} -> Record
DELETE /api/records/:id         -> 204
GET    /api/stats/daily?days=7  -> DailyStat[]
```

**DB (SQLite起步):**
```sql
create table records (
  id text primary key, user_id text not null, title text,
  phase text not null, duration_secs integer not null,
  completed_at integer not null, created_at integer not not null
);
create index idx_user_completed on records(user_id, completed_at);
```

---

## 6. 实施步骤 (分3里程碑)

**M0 本地CRUD (1天, 不联网可验收)**
- [ ] 1. `model.rs` + `api.rs trait` + `store.rs` 内存实现
- [ ] 2. `RecordsView` UI: 列表 + 输入 + 删除按钮 (复用 Button/Tag)
- [ ] 3. `cargo test -p pomodoro` 新增 4 用例通过

**M1 后端 (1天)**
- [ ] 4. `server/Cargo.toml` + `main.rs` + `/health`
- [ ] 5. `records` CRUD + `SQLite` + `sqlx` 编译时检查
- [ ] 6. `JWT` 登录 + `CorsLayer` + `curl` 验证

**M2 联调与四端 (1.5天)**
- [ ] 7. `ReqwestApi` / `FetchApi` 实现 `trait Api`
- [ ] 8. `Store::sync()` 离线队列 + 乐观更新 + 500ms防抖写本地
- [ ] 9. `Desktop make desktop` + `Web make web` + `Capacitor sync + gradle` 真机
- [ ] 10. `auth.js` 登录页 + token注入

---

## 7. 如何测试 (单元/集成/E2E/手动/性能/安全)

### 7.1 自动化

| 层级 | 命令/工具 | 覆盖 | 关键用例 |
|---|---|---|---|
| **单元** | `cargo test -p pomodoro` (复用现有 `TestAppContext` 无头窗口) | `store.rs` | `create_adds_to_list`, `delete_removes`, `update_changes_title`, `pending_queue_survives_restart` |
| **单元** | `cargo test -p server` | `routes` | `create_record_returns_201`, `auth_rejects_without_token`, `sqlx_query_compiles` |
| **集成** | `cargo test -- --nocapture` + `httptest` | `Api` | `FetchApi_mock_server CRUD`，`ReqwestApi_mock` |
| **E2E(Web)** | `bun run build && bun run preview:3002` + `Playwright` | 浏览器 | `完成1番茄→列表+1→刷新仍在→删除→消失` |
| **E2E(Desktop)** | `cargo run -p pomodoro-desktop` 人工 + `cargo test` 截图对比 (可选) | 桌面 | 同上 |

### 7.2 手动清单 (必须逐项打勾)

- [ ] **离线**: 断网→创建3条→列表乐观显示→联网→`sync()`后 `GET /records` 为3条
- [ ] **跨端同步**: Desktop建1条→Web刷新出现1条
- [ ] **后台**: Web后台标签1小时→切回前台 `overshoot` 结转正确 + `pending_queue` 补发
- [ ] **重启**: `kill` 后重开，`localStorage/file` 恢复记录
- [ ] **权限**: 未登录 `GET /records` 401，带过期token 401，`refresh` 后200
- [ ] **移动**: `make android && adb install` 真机 60fps，`https://localhost` 无CORS错
- [ ] **PWA**: 飞行模式打开 `www/dist`，列表可读 (读本地缓存)

### 7.3 性能与安全

- [ ] **体积**: `wasm` gzip后 <7MB，`server` `/records` 分页 `?limit=50`
- [ ] **压测**: `oha -n 1000 -c 20 http://localhost:3000/api/records` p95 <200ms (SQLite)
- [ ] **安全**: `cargo audit`, `sqlx` 预编译防注入, `JWT` 短有效期, `CORS` 生产收紧

### 7.4 测试文件位置

```
crates/pomodoro/tests/store.rs   # 新增 CRUD 4用例
crates/pomodoro/tests/sync.rs    # 新增 离线队列 2用例
server/tests/api_test.rs         # 新增 5用例
www/e2e/crud.spec.ts             # Playwright (可选)
```

---

## 8. 验收标准 (全部绿才算完成)

- [ ] `cargo test -p pomodoro` 13通过 (原9 + 新4)
- [ ] `cargo test -p server` 5通过
- [ ] `make desktop` 增删改查可用，重启不丢
- [ ] `make web` + `make web-release` + `bun run preview` 同上，断网可用
- [ ] `make mobile-sync && make android` 真机同上
- [ ] `curl -H "Authorization: Bearer $TOKEN" https://api.xxx.com/api/records | jq length` == 列表长度
- [ ] 无 `#[cfg(target_family)]` 污染 `crates/pomodoro`

---

## 9. 风险与对策

见本文件 §3 表格，最大风险是 `原生GPUI` 版本错配→对策是**不做**，走壳。

---

## 10. 后续可拆

- 009-1 本地CRUD (可独立PR)
- 009-2 后端 (可独立PR)
- 009-3 联调 (依赖前两者)

> 完成后回填 `TODO.md` 看板状态为 ✅，并更新 `README.md` 的 已实现/未实现 表。
