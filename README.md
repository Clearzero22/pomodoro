# 番茄钟 · GPUI Kit 全端示例

一个最小但完整的番茄计时器，**同一份业务代码**同时运行在桌面端和浏览器端。

```
pomodoro/
├── crates/
│   ├── pomodoro/           业务与界面（平台无关，零 #[cfg]）
│   │   ├── src/lib.rs        Pomodoro 视图 + 计时状态机
│   │   └── tests/timer.rs    无头窗口里的行为测试
│   ├── pomodoro-desktop/   桌面入口  fn main()
│   └── pomodoro-web/       浏览器入口 #[wasm_bindgen] run()
│       ├── fonts/             内嵌字体（Web 没有系统字体库）
│       ├── scripts/           build-wasm.sh
│       └── www/               Vite + JS 加载器
└── scripts/                 桌面构建 / 运行辅助
```

平台差异只有两个入口文件，`crates/pomodoro` 里没有任何 `target_family = "wasm"`。

## 需求

- Rust（nightly，见 `rust-toolchain.toml`）
- Web：`wasm32-unknown-unknown` target、`wasm-bindgen-cli`、[Bun](https://bun.sh)

```sh
rustup target add wasm32-unknown-unknown
# 版本必须与 Cargo.lock 里的 wasm-bindgen 完全一致
cargo install wasm-bindgen-cli --version 0.2.129
```

## 桌面端

```sh
# 开发
cargo run -p pomodoro-desktop

# 发布
cargo build --release -p pomodoro-desktop
./target/release/pomodoro        # Windows: target\release\pomodoro.exe
```

单文件可执行，图标等资源由 `gpui-kit-assets` 编译进二进制。

## 浏览器端 / WebAssembly

```sh
cd crates/pomodoro-web

# 1. 编译 WASM + 生成 JS 绑定（输出到 www/src/wasm/）
./scripts/build-wasm.sh              # debug
./scripts/build-wasm.sh --release    # 发布

# 2. 开发服务器
cd www && bun install && bun run dev    # http://localhost:3001

# 3. 生产构建（产物在 www/dist/）
cd www && bun run build
```

改了 Rust 代码要重跑 `build-wasm.sh` 再刷新页面；Vite 不会替你编译 Rust。
只改 `www/` 下的 JS/CSS 时 Vite 自己热更新。

### 图标资源

Web 端图标不打包进 WASM，按需从 `assets/icons/` 下载。开发时由 Vite 中间件
从本地检出的 `gpui-kit` 提供，生产环境通过 `run(asset_endpoint)` 指定：

```js
await wasm.run(import.meta.env.VITE_ASSET_ENDPOINT ?? '/');
```

线上需要把 `gpui-kit/crates/assets/assets/icons/*.svg` 部署到该地址下的
`icons/` 目录，或指向 CDN。

## 测试

```sh
cargo test -p pomodoro
```
9 个行为测试跑在 GPUI 的无头窗口里，断言状态而不是截图：

| 测试 | 盯的回归 |
| --- | --- |
| `starts_paused_at_the_full_focus_duration` | 初始状态 |
| `running_clock_derives_from_the_deadline` | **丢拍不会冻结界面**：ticker 一拍没跑，剩余时间也必须真减少 |
| `pause_freezes_and_start_resumes` | 暂停期间时钟不动，恢复后继续 |
| `reset_returns_to_the_phase_start` | 重置回整段 |
| `switching_phase_resets_the_clock` | 换阶段后暂停在新区间 |
| `a_finished_focus_phase_advances_to_break` | 阶段边界推进 + 计数 |
| `a_long_absence_carries_the_overshoot_into_later_phases` | **后台一小时回来**：跨过多个阶段且超出时间结转到当前阶段，不是把当前阶段重计 |
| `timer_events_report_start_pause_reset_and_completion` | 通知预约依赖的事件内容 |
| `timer_runs_without_an_event_handler` | 没接通知时零副作用 |

## 打包与分发

`cargo build --release` 只产出可执行文件，不是安装包。安装包、签名、图标和发布
校验由应用自己负责，并且必须在目标平台上构建。完整说明见 [docs/packaging.md](docs/packaging.md)。

| 目标 | 产物 | 命令 |
| --- | --- | --- |
| Windows | `dist/pomodoro-windows-x64.zip` | `scripts\package-windows.ps1` |
| Windows | `dist/Pomodoro-0.1.0-Setup.exe` | 同上，需装 Inno Setup 6 |
| macOS | `.app` / `.dmg` | `docs/packaging.md`（需在 Mac 上做） |
| Linux | `.tar.gz` / `.deb` | `docs/packaging.md`（需在 Linux 上做） |
| 浏览器 | `www/dist/` 静态站点 | `make web-release` |
| Android / iOS | 可安装 PWA | 同上，部署后「添加到主屏幕」 |
| Android / iOS 上架 | WebView 壳（Capacitor） | `make android` / `docs/mobile.md` |

```powershell
# Windows
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
```

```sh
# 浏览器生产包
make web-release
cd crates/pomodoro-web/www && bun run preview --port 3002
```

Web 产物 22 MB（wasm 22.4 MB，gzip 6.9 MB），带 `manifest.webmanifest` +
`sw.js` + 192/512 图标，因此在安卓 Chrome 和 iOS Safari 都能「添加到主屏幕」
以独立窗口启动并离线可用。

### 手机上安装

现在有两条路，完整对比见 [docs/mobile.md](docs/mobile.md)：

| 路线 | 能上架 | 状态 |
| --- | --- | --- |
| **PWA** | 不上架，添加到主屏幕 | ✅ 已完成 |
| **WebView 壳**（Capacitor） | ✅ App Store / Play Store | ✅ 脚手架已就绪，需 SDK 出包 |
| 原生 GPUI（`gpui-pre-mobile`） | ✅ | ❌ 需先做 GPUI 版本对齐 |

- **Android（Chrome）**：菜单 →「安装应用 / 添加到主屏幕」
- **iOS（Safari）**：分享 →「添加到主屏幕」

要上架就用 Capacitor 壳，它把 `www/dist` 原样塞进原生 WebView，Rust 代码不改：

```sh
make mobile-sync            # web-release + 同步进 mobile/ 壳
cd mobile && ./node_modules/.bin/capacitor open android
```

Android 工程已经生成在 `mobile/android/`，装好 Android SDK 后 `make android`
即可出 debug APK；iOS 只能在 Mac 上 `capacitor add ios` 后走 Xcode。

原生 iOS/Android（GPUI 直接渲染）目前不可行：`gpui-pre-mobile` 锁在
`gpui-pre 0.3.4` 而本仓库用 `0.3.9`，类型不兼容；且官方只验证过 iOS 模拟器和
单个聊天场景。业务代码无需修改，等上游对齐后 `Pomodoro` 视图可原样复用。

## 设计要点

**计时按截止时刻算，不按「每秒减一」。** 剩余时间 = `deadline - now()`，ticker
只负责到点换阶段和 `notify()`。这样任何一拍丢失（窗口重建、任务被取消、更新
失败）只影响刷新频率，不影响正确性——早期的「每秒递减」写法一旦丢拍，界面就
永久冻结在没减过的数字上，而且不会报错。

**时钟用 `instant::Instant`。** `std::time::Instant::now()` 在
`wasm32-unknown-unknown` 上会 panic（`time not implemented on this platform`）；
`instant` 在浏览器里走 `performance.now()`。

**ticker 用 `cx.spawn` 而不是 `spawn_in`。** 它不需要窗口，因此不会因为「实体
没有关联窗口」这类失败而中断；`update` 的失败被显式忽略，而不是当成「用户停止了」。

**Web 端必须自己注册字体。** 浏览器没有系统字体数据库，`add_fonts` 要在第一个
窗口之前调用，且必须包含 `.SystemUIFont` 解析到的族（IBM Plex Sans），否则首帧
文本测量就 panic。

## 已实现 vs 未实现

已实现：专注/休息切换、开始/暂停/重置、进度环、完成计数、空格与 ⌘R 快捷键、
无头行为测试、桌面 + WASM 双端构建。

未实现（示例范围外）：计时结束的提示音与系统通知、时长设置界面、跨会话持久化、
托盘/迷你窗、桌面安装包（`.dmg`/`.msi`/`.AppImage`）。
