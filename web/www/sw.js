// Keeps the table playable offline. The network is tried first, so a new
// version is picked up as soon as there is a connection; the cache answers
// only when the network does not, or when the page itself is slow to come
// on a connection that stalls instead of failing.
const CACHE = 'poker-v2';
const FILES = ['./', 'index.html', 'style.css', 'app.js', 'claude.js', 'hands.js', 'worker.js',
  'pkg/poker_web.js', 'pkg/poker_web_bg.wasm', 'manifest.webmanifest', 'icon.svg'];
// How long the page may take before a cached copy is shown instead.
const PAGE_WAIT_MS = 4000;

self.addEventListener('install', (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(FILES)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (event) => {
  event.waitUntil(caches.keys()
    .then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
    .then(() => self.clients.claim()));
});

self.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET' || new URL(request.url).origin !== location.origin) return;
  const cached = () => caches.match(request, { ignoreSearch: true });
  const network = fetch(request).then((response) => {
    if (response.ok) {
      const copy = response.clone();
      // Kept alive until the copy is stored, not just until the page has it.
      event.waitUntil(caches.open(CACHE).then((cache) => cache.put(request, copy)));
    }
    return response;
  });
  const answer = network.catch(() => cached().then((hit) => hit ?? Response.error()));
  // Only the page waits a limited time: its scripts and the engine must come
  // from the same version, so they always wait for the network if it answers.
  if (request.mode !== 'navigate') {
    event.respondWith(answer);
    return;
  }
  const slow = new Promise((resolve) => setTimeout(resolve, PAGE_WAIT_MS))
    .then(cached)
    .then((hit) => hit ?? answer);
  event.respondWith(Promise.race([answer, slow]));
});
