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

const CACHE = 'pomodoro-v2';

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
