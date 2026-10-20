// 浏览器入口：加载 WASM 模块并启动应用。
//
// GPUI 的 Web 平台把键盘和输入法事件交给一个 1x1 的透明 `<input>`，并在窗口
// 打开和每次 pointerdown 时聚焦它。在手机上，聚焦输入框会弹出软键盘——用户
// 只是滑动页面或点一个按钮，却弹出了键盘。触摸设备因此把该元素标记为
// readonly，并关闭 inputmode：iOS 对只读字段不弹键盘，元素仍可聚焦，
// GPUI 依然能跟踪窗口激活和按键。

import { installRustBridge } from './notifications.js';

const touchOnly =
  window.matchMedia?.('(hover: none) and (pointer: coarse)').matches ?? false;
function keepKeyboardClosed(node) {
  if (node.tagName !== 'INPUT') return;

  if (touchOnly) {
    node.readOnly = true;
    node.setAttribute('inputmode', 'none');
  }
}

function watchPlatformInput() {
  document.querySelectorAll('body > input').forEach(keepKeyboardClosed);
  new MutationObserver((records) => {
    for (const record of records) {
      record.addedNodes.forEach(keepKeyboardClosed);
    }
  }).observe(document.body, { childList: true });
}

async function init() {
  const loadingEl = document.getElementById('loading');

  watchPlatformInput();

  // 必须在 wasm.run() 之前装好：Rust 侧在窗口创建后就开始计时，
  // 之后的每个计时事件都会调到这里。
  installRustBridge();

  try {
    const wasm = await import('./wasm/pomodoro_web.js');
    await wasm.default();

    // 资源地址：开发时由 Vite 提供，生产环境换成 CDN。
    // `document.baseURI` 跟随 Vite 的 base，部署到子路径时无需改代码。
    const assetEndpoint =
      import.meta.env.VITE_ASSET_ENDPOINT ?? new URL('.', document.baseURI).pathname;

    // 原生壳（Capacitor）里强制 WebGL2：WebView 的 WebGPU 探测会挂住，
    // 使画面永远停在启动页。浏览器里保留 Auto（先试 WebGPU，再退 WebGL2）。
    const backend = import.meta.env.VITE_WEB_BACKEND ??
      (globalThis.Capacitor?.isNativePlatform?.() ? 'webgl' : 'auto');

    // 后端地址。设了 `VITE_API_URL` 才启用联网模式；
    // 没设就是纯本地（数据在 localStorage），界面完全一样。
    const apiUrl = import.meta.env.VITE_API_URL ?? null;

    await wasm.run(assetEndpoint, backend, apiUrl);

    // 只有真正开窗并画出第一帧之后才算启动完成。
    loadingEl?.remove();
  } catch (error) {
    console.error('Failed to initialize:', error);

    if (loadingEl) {
      loadingEl.innerHTML = `
        <div class="error">
          <h2>番茄钟启动失败</h2>
          <p>${error.message || error}</p>
          <p style="margin-top: 10px; font-size: 14px;">请查看浏览器控制台。</p>
        </div>
      `;
    }
  }
}

init();
