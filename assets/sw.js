// Cognitheon PWA Service Worker —— 网络优先 + 立即接管，保证部署后用户即时拿到新版。
//
// 旧版是「缓存优先 + 固定文件名」，导致每次部署用户仍看旧版、需手动清缓存。这里改为：
// - install 时 skipWaiting，新 SW 立即激活，不等所有旧标签关闭；
// - activate 时删除所有旧版本缓存并 clients.claim 立即接管；
// - fetch 网络优先（拿到新内容就更新缓存），断网时回退缓存——既即时更新又保留离线可用。
const CACHE_NAME = 'cognitheon-v2';
const FILES_TO_CACHE = ['./', './index.html', './cognitheon.js', './cognitheon_bg.wasm'];

self.addEventListener('install', (e) => {
  self.skipWaiting();
  e.waitUntil(caches.open(CACHE_NAME).then((cache) => cache.addAll(FILES_TO_CACHE)));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (e) => {
  if (e.request.method !== 'GET') {
    return;
  }
  // 网络优先：成功则顺手刷新缓存；失败（离线）才回退缓存。
  e.respondWith(
    fetch(e.request)
      .then((resp) => {
        const copy = resp.clone();
        caches.open(CACHE_NAME).then((cache) => cache.put(e.request, copy));
        return resp;
      })
      .catch(() => caches.match(e.request))
  );
});
