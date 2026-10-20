// 让番茄钟可以离线打开，并在手机上作为独立应用启动。
//
// 缓存策略必须区分两类请求，否则重新打包后会出现「白屏/卡在启动页」：
//
//   - **导航请求（HTML）**：network-first。`index.html` 里写着带哈希的资源名，
//     一旦缓存优先，新版本装上去后仍会拿到旧 HTML，它引用的旧 JS 已经不在包里，
//     于是 404、界面永远停在启动页。
//   - **带哈希的静态资源（/assets/*、wasm）**：cache-first。内容变了文件名就变，
//     缓存永远不会读到错的东西。
//
// 原生壳（Capacitor）里不注册 Service Worker：APK 本身就是离线包，
// 注册只会引入上面那类陈旧缓存问题。见 index.html 的注册条件。
//
// # 缓存名必须带版本
//
// 缓存名如果写死（`pomodoro-v2`），重新发版后旧缓存不会被清掉：
// 导航请求虽然走 network-first 能拿到新 HTML，但新 HTML 引用的带哈希资源
// 在旧缓存里没有、服务器上又已经换成新名字，cache-first 命中不了就 404。
//
// 所以缓存名由**构建期注入的版本号**决定（见 `vite.config.js` 的
// `sw-version` 插件）。版本一变，缓存名就变，activate 阶段会清掉所有旧缓存。
// 版本号取自构建产物里 wasm 的内容哈希，因此内容变了版本一定变。

const CACHE = 'pomodoro-__BUILD_VERSION__';

/** 只预缓存不含哈希入口信息的文件。 */
const PRECACHE = ['./manifest.webmanifest', './icons/icon-192.png', './icons/icon-512.png'];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(PRECACHE))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      // 版本号变了就等于换了一个缓存空间，旧缓存全部作废。
      .then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  // 导航：先网络，失败再回落到缓存（离线可用）。
  if (request.mode === 'navigate') {
    event.respondWith(
      fetch(request)
        .then((response) => {
          if (response.ok) {
            const copy = response.clone();
            caches.open(CACHE).then((cache) => cache.put(request, copy));
          }
          return response;
        })
        .catch(() => caches.match(request).then((cached) => cached ?? caches.match('./')))
    );
    return;
  }

  // 带哈希的资源：缓存优先。
  event.respondWith(
    caches.match(request).then((cached) => {
      if (cached) return cached;
      return fetch(request).then((response) => {
        if (response.ok && response.type === 'basic') {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
        }
        return response;
      });
    })
  );
});
