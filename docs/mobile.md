# 手机上安装：三条路线

番茄钟的界面是 Rust + GPUI 画的，浏览器里跑的是编译成 WebAssembly 的**同一份代码**。
「装到安卓/iPhone」因此有三条路，代价和产出完全不同：

| 路线 | 能不能上架 | 现在能不能做 | 工作量 | 风险 |
| --- | --- | --- | --- | --- |
| **A. PWA** | 不上架，加到主屏幕 | ✅ 已完成 | 0 | 无 |
| **B. WebView 壳**（Capacitor / Tauri v2） | ✅ App Store / Play Store | ✅ 脚手架已就绪 | 半天，需 Mac + Xcode / Android Studio | 低 |
| **C. 原生 GPUI**（`gpui-pre-mobile`） | ✅ 可以 | ❌ 需先做版本对齐 | 数周 | 高，实验性 |

推荐顺序：**先 A，要上架就 B**，C 等上游稳定。

---

## A. PWA（已完成）

`crates/pomodoro-web/www/dist` 已包含 `manifest.webmanifest`、`sw.js`、192/512
图标和 `apple-mobile-web-app-*` 元标签。

- **Android（Chrome / Edge）**：打开站点 → 菜单「安装应用 / 添加到主屏幕」。
- **iOS（Safari）**：分享 → 「添加到主屏幕」。

安装后以独立窗口启动，Service Worker 让它在断网时也能打开（首次加载需要网络，
wasm 22 MB / gzip 6.9 MB）。

触摸设备上 GPUI 的隐藏输入框会被设成 `readonly` + `inputmode="none"`，
避免只是滚动页面就弹出软键盘（见 `www/src/main.js`）。

**PWA 的边界**：canvas 渲染没有 DOM 无障碍树，屏幕阅读器读不到内容；手机上也
没法往输入框里打字。番茄钟这种以按钮为主的界面适合；需要重文本输入的界面不适合。

---

## B. WebView 壳：真正能上架的路

思路：把 `www/dist` 这份静态产物原样塞进原生 WebView，用 Capacitor 生成
Xcode / Gradle 工程，然后按平台常规流程签名上架。**Rust 代码一行都不用改**，
也不需要移动端 GPU 平台实现。

当前状态：脚手架已就绪，Android 工程已生成，web 产物（含 22 MB wasm）已复制进
`android/app/src/main/assets/public`；图标/启动图已生成；本地通知已接通并在
真实浏览器里验证过调用链。缺 Android SDK / Xcode 才能出安装包。

### 前置

- 先产出 web 产物：`make web-release`
- Android：Android Studio + SDK（API 34+）+ JDK 17+
- iOS：macOS + Xcode 15+（**iOS 构建只能在 Mac 上做**）

### 步骤

```sh
cd mobile
bun install          # 已执行

# 首次：生成原生工程（读 capacitor.config.json 的 webDir）
./node_modules/.bin/capacitor add android    # 已执行，生成 mobile/android/
./node_modules/.bin/capacitor add ios        # 只能在 macOS 上执行

# 图标与启动图：源图在 mobile/assets/{icon-only,splash}.png
./node_modules/.bin/capacitor-assets generate --android

# 每次重新构建 web 产物之后
./node_modules/.bin/capacitor sync

# 打开原生 IDE，跑真机 / 打包
./node_modules/.bin/capacitor open android
./node_modules/.bin/capacitor open ios
```

`webDir` 指向 `../crates/pomodoro-web/www/dist`，所以 `sync` 复制的是发布构建，
不是源码目录。改了 Rust 之后要重新 `make web-release` 再 `sync`。

### Android 出包（需先装 SDK）

```sh
# 装 Android Studio 或 commandline-tools，并把 SDK 路径写进
# mobile/android/local.properties：sdk.dir=C\\Users\\<you>\\AppData\\Local\\Android\\Sdk
cd mobile/android
./gradlew assembleDebug          # app/build/outputs/apk/debug/app-debug.apk
./gradlew bundleRelease          # app/build/outputs/bundle/release/app-release.aab（上架 Play）
```

`local.properties` 不要提交到版本库。

### 需要自己补的东西

壳里没有的东西都得自己加，Capacitor 的插件生态覆盖这些：

| 需求 | 做法 | 状态 |
| --- | --- | --- |
| 应用图标 / 启动图 | `@capacitor/assets`，从 1024×1024 PNG 生成全平台尺寸 | ✅ 已生成 25 个资源 |
| 计时结束的本地通知 | `@capacitor/local-notifications` | ✅ 已接通 |
| 状态持久化 | `@capacitor/preferences`，或改用 Rust 侧 Web 存储 | ⬜ 未做 |
| 安全区 / 刘海 | canvas 铺满视口，内容居中；如要贴边，读 `env(safe-area-inset-*)` 传入视图 | ⬜ 未做 |
| 状态栏文字颜色 | `@capacitor/status-bar` | ⬜ 未做 |

### 本地通知是怎么接的

关键点：**在阶段开始时就预约，不是等结束再发**。应用退到后台后 WebView 会暂停
JS，靠完成那一刻的回调永远发不出去。

```
Rust（crates/pomodoro/src/lib.rs）
  TimerEvent::{Started|Paused|Reset|PhaseCompleted}
        │  set_on_event(handler)
        ▼
Rust（crates/pomodoro-web/src/lib.rs）
  #[wasm_bindgen(inline_js = "...")] → globalThis.__pomodoroNotify.schedule(seconds, title, body)
        ▼
JS（crates/pomodoro-web/www/src/notifications.js）
  Capacitor 原生 → LocalNotifications.schedule({ at: now + seconds })
  浏览器 PWA   → Web Notifications + setTimeout
  其他         → no-op
```

三种环境的降级是必须的：同一份 `dist` 既跑在壳里，也跑在普通浏览器里。
浏览器要求通知权限由**用户手势**触发，所以第一次 `pointerdown`/`keydown` 时
就预请求，`schedule()` 里只检查权限。任何失败都只 `console.warn`，不影响计时。

实测（真实浏览器点击，非注入事件）：

```
点击「开始」 → ["schedule", 1500, "专注结束"]
点击「暂停」 → ["cancel"]
```

Android 侧的权限由插件 manifest 自带（已验证）：`POST_NOTIFICATIONS`、
`WAKE_LOCK`、`RECEIVE_BOOT_COMPLETED`，并注册了 `TimedNotificationPublisher`
与开机恢复 receiver。`cap sync` 输出确认：

```
[info] Found 1 Capacitor plugin for android:
       @capacitor/local-notifications@7.0.7
```

iOS 首次 `schedule` 会弹系统授权框；拒绝后不预约，计时照常。

### 必须实测的点

- **渲染后端**：`WebBackendPreference::Auto` 先试 WebGPU，失败退 WebGL2。iOS 上
  WebGPU 只有新版 Safari 支持，旧系统会走 WebGL2；Android WebView 的 WebGPU 仍属
  实验特性。**两条路径都要在真机上确认画面正常**，模拟器不算。
- **性能**：wasm + canvas 在手机上能不能到 60 fps，需要真机 release 构建实测。
- **`file://` 与自定义 scheme**：Capacitor 在 Android 用 `https://localhost`，
  iOS 用 `capacitor://localhost`，wasm 的 `fetch` 都能工作；若换成手写 WebView 壳，
  注意 `file://` 下 wasm 的 `fetch` 会被拦截。
- **图标资源**：本示例没有用 `IconName`，所以 `dist` 里不需要 `icons/` 目录。
  一旦你用了 Kit 的图标，就要把 `gpui-kit/crates/assets/assets/icons/*.svg` 复制进
  `dist/icons/`，或设置 `VITE_ASSET_ENDPOINT` 指向 CDN——WebView 里没有开发服务器
  帮你转发。
- **上架审核**：Apple 对「只是网页套壳」的应用审核较严（Guideline 4.2 最低功能
  要求）。要么补足原生能力（本地通知、离线、桌面小组件），要么改用路线 C。

---

## C. 原生 GPUI（实验性，暂不适用本示例）

GPUI Kit 的定位是桌面优先，移动端支持基于社区的
[gpui-mobile](https://github.com/itsbalamurali/gpui-mobile)；Kit 用的是它的兼容
分支 `gpui-pre-mobile`。GPUI 视图作为**原生界面里的一个子视图**运行，导航、
安全区、键盘避让仍由 UIKit / Android 负责。

接本示例有三个硬门槛：

1. **GPUI 版本必须对齐。** `gpui-pre-mobile` 锁在 `gpui-pre 0.3.4`，本仓库的
   `gpui-kit` 固定在 `0.3.9`。Cargo 会同时选两版，`App`/`Window` 变成不同类型。
   必须先把移动平台和渲染器升到同一个 GPUI 版本并验证，才能用本地 path 依赖。
2. **移动端没有 `gpui_kit::application()`。** iOS/Android 下该导出被排除
   （`crates/kit/src/lib.rs`），宿主必须自己构造
   `Application::with_platform(...)`，在 Rust 回调里 `gpui_kit::init(cx)` 后挂一个
   `component::Root`，并通过 `gpui_ios_set_embedded()` 等桥接函数避免平台再开一个
   原生窗口。
3. **验证范围有限。** 官方只验证过 iOS 模拟器路径，以及一个由 TextView、Button、
   Menu、Popover、Scrollbar、Input、Textarea 组成的聊天场景；完整应用布局和
   Android 宿主路径都未验证。

好消息是业务代码不用改：`crates/pomodoro` 里没有任何 `#[cfg(target_family)]`，
接上移动平台后 `Pomodoro` 视图可以原样复用。`gpui_kit::is_mobile()` 是编译期的
`const fn`，需要触摸友好的尺寸时用它分支。

---

## 选型建议

- 只想让人在手机上用：**A**，现在就能用。
- 要上架、要本地通知、要应用图标：**B**，一天内能出包。
- 要原生性能、要长期维护原生体验：**C**，先等 `gpui-pre-mobile` 追上 GPUI 版本，
  或者自己把版本对齐做完并回馈上游。
