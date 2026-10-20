# 打包与安装

`cargo build --release` 只产出可执行文件，不是安装包。安装包、签名、图标、
发布校验由应用自己负责，并且**必须在目标平台上构建**（Windows 包在 Windows 上
打，macOS 包在 Mac 上打）。

| 目标 | 可移植产物 | 安装产物 | 本仓库是否已提供 |
| --- | --- | --- | --- |
| Windows | `dist/pomodoro-windows-x64.zip` | Inno Setup `Setup.exe` | 脚本已提供，`ISCC` 缺失时只出 ZIP |
| macOS | `.app`（装在 zip/dmg 里） | `.dmg` + Developer ID 签名/公证 | 未提供（需在 Mac 上做） |
| Linux | `.tar.gz` | `.deb` | 未提供（需在 Linux 上做） |
| 浏览器 | `www/dist/` 静态站点 | 任意静态托管 / 可安装 PWA | 已提供 |
| Android / iOS | — | PWA（添加到主屏幕） | 已提供 |
| Android / iOS 原生 | — | 需接 `gpui-pre-mobile` | 见文末「移动端」 |

## Windows

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
```

- 总是产出 `dist\pomodoro-windows-x64.zip`（解压即用，但不注册卸载项和快捷方式）。
- 装了 [Inno Setup 6](https://jrsoftware.org/isinfo.php) 后，同一脚本再调用
  `packaging\pomodoro.iss`，产出 `dist\Pomodoro-0.1.0-Setup.exe`，带开始菜单、
  桌面快捷方式和卸载项。
- `AppId` 跨版本必须保持不变，否则新版本会被当成另一个应用而不是升级。

正式发布前还要做两件脚本不会替你做==的事：

1. **先签 `pomodoro.exe`，再放进 ZIP 或安装包**；安装包本身签在最后。
2. Inno Setup 的卸载程序内嵌在 `Setup.exe` 里，签名 `Setup.exe` **不会**顺带签
   卸载程序，需要配置 signed uninstaller 工作流。

验证：把 ZIP 解压到一个全新目录再启动，不要只测 `target\release\pomodoro.exe`。

## macOS

在 Mac 上：

```sh
cargo build --release --locked -p pomodoro-desktop

mkdir -p "dist/番茄钟.app/Contents/MacOS" "dist/番茄钟.app/Contents/Resources"
cp target/release/pomodoro "dist/番茄钟.app/Contents/MacOS/pomodoro"
cat > "dist/番茄钟.app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>番茄钟</string>
  <key>CFBundleDisplayName</key><string>番茄钟</string>
  <key>CFBundleIdentifier</key><string>com.example.pomodoro</string>
  <key>CFBundleExecutable</key><string>pomodoro</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
</dict></plist>
PLIST
plutil -lint "dist/番茄钟.app/Contents/Info.plist"
open "dist/番茄钟.app"
```

对外分发（App Store 之外）需要：Developer ID Application 证书 + hardened runtime
签名 → `codesign --verify --deep --strict` 通过 → 打成 DMG → 签 DMG →
`xcrun notarytool submit --wait` 返回 **Accepted** → `stapler staple` →
`stapler validate`。`--sign -`（临时签名）只能本机用。

## Linux

```sh
cargo build --release --locked -p pomodoro-desktop

# 可移植包
mkdir -p dist/portable/Pomodoro/bin
cp target/release/pomodoro dist/portable/Pomodoro/bin/
tar -czf dist/pomodoro-linux-x64.tar.gz -C dist/portable Pomodoro

# Debian/Ubuntu
mkdir -p dist/deb-root/DEBIAN dist/deb-root/usr/lib/pomodoro \
         dist/deb-root/usr/share/applications \
         dist/deb-root/usr/share/icons/hicolor/256x256/apps
install -m 0755 target/release/pomodoro dist/deb-root/usr/lib/pomodoro/pomodoro
# 写入 DEBIAN/control 与 usr/share/applications/pomodoro.desktop 后：
dpkg-deb --root-owner-group --build dist/deb-root dist/pomodoro_0.1.0_amd64.deb
```

Linux 的运行前提是**真实的图形会话 + Vulkan 驱动**；只有 Vulkan loader 的机器
渲染不出窗口。用 `objdump -p target/release/pomodoro` 检查直接依赖，并在你支持的
最老发行版上实测。

## 浏览器（静态站点）

```sh
make web-release        # = wasm release + vite build
# 产物：crates/pomodoro-web/www/dist/  （22 MB，其中 wasm 22.4 MB，gzip 后约 6.9 MB）
```

本地预览：

```sh
cd crates/pomodoro-web/www && bun run preview --port 3002
```

### 部署要点

1. **MIME 类型**：`.wasm` 必须是 `application/wasm`，否则浏览器拒绝流式实例化。
2. **子路径部署**：`VITE_BASE=/pomodoro/ bun run build`，同时把图标目录放在
   `<base>/icons/`，或设置 `VITE_ASSET_ENDPOINT` 指向 CDN。
3. **COOP/COEP 头**（`Cross-Origin-Opener-Policy: same-origin`、
   `Cross-Origin-Embedder-Policy: require-corp`）：当前是单线程 WASM，不加也能跑；
   以后切换到多线程 WASM 时必须加，且此时所有资源都需要 CORS 或同源。
4. **体积**：release wasm 22.4 MB / gzip 6.9 MB。首次加载慢，Service Worker 之后
   走缓存。

## 手机上安装：现在的可行答案是 PWA

`www/dist/` 已经带了 `manifest.webmanifest`、`sw.js`、192/512 图标和
`apple-mobile-web-app-*` 元标签，因此：

- **Android（Chrome/Edge）**：打开站点 → 菜单「添加到主屏幕 / 安装应用」，
  以独立窗口启动，离线可用。
- **iOS（Safari）**：分享 → 「添加到主屏幕」，以 standalone 模式启动，离线可用。
- 手机上没有键盘，所以 `www/src/main.js` 对触摸设备关闭了软键盘弹出：
  GPUI 用隐藏 `<input>` 收键盘事件，触摸设备上会给它加 `readonly` +
  `inputmode="none"`，避免只是滚动页面就弹键盘。

限制（诚实说明）：canvas 渲染没有 DOM 无障碍树，屏幕阅读器读不到内容；点按
可用，但"往输入框里打字"在手机上基本不可用——番茄钟这类以按钮为主的界面适合，
需要重文本输入的界面不适合。

## 移动端原生（实验性，暂不适用本示例）

GPUI Kit 的定位是桌面优先。移动端支持基于社区的
[gpui-mobile](https://github.com/itsbalamurali/gpui-mobile)，通过
`gpui-pre-mobile` 提供一个 iOS/Android 平台实现，让 GPUI 视图作为**原生界面里的
一个子视图**（UIKit `UIViewController` / Android `View`），导航、安全区、键盘避让
仍由原生端负责。

要接到本示例上，当前有三个硬门槛：

1. **GPUI 版本必须对齐**。`gpui-pre-mobile` 的兼容分支锁在 `gpui-pre 0.3.4`，
   而本仓库用的 `gpui-kit` 固定在 `gpui-pre 0.3.9`。两版 GPUI 类型不兼容
   （`App`/`Window` 会被当成两个不同的类型）。必须先把移动平台和渲染器升级到
   同一个 GPUI 版本并验证，才能用本地 path 依赖接入。
2. **移动端没有 `gpui_kit::application()`**。iOS/Android 目标下这个导出被排除，
   宿主必须自己构造 `Application::with_platform(...)`，并在 Rust 回调里
   `gpui_kit::init(cx)` 之后挂一个 `component::Root`。
3. **验证范围有限**。官方只验证过 iOS 模拟器路径，以及一个由 TextView、Button、
   Menu、Popover、Scrollbar、Input、Textarea 组成的聊天场景；完整应用布局和
   Android 宿主路径都尚未验证。

所以「安装到安卓手机和 iPhone」现在推荐走上面的 PWA；等 `gpui-pre-mobile` 追上
GPUI 版本后，再把 `crates/pomodoro` 原样接进 Swift/Android 宿主，业务代码不需要改。

## 发布前检查清单

1. 文件名、内嵌版本号、架构、包标识、校验和一致。
2. 把产物**解压/安装到新位置**后启动，不依赖源码树或当前工作目录。
3. 在干净的机器或每种 OS/架构的 VM 上验证：首次启动、窗口创建、文字与图标、
   键盘与指针、断网时是否仍可用。
4. 装 N 版再装 N+1 版，检查数据与快捷方式；再卸载，确认遗留文件符合设计。
5. 校验签名/公证，发布 SHA-256，并**重新下载产物**比对校验和。
