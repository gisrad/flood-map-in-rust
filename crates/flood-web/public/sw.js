// Service worker Hidrosentinel: hanya cangkang aplikasi (HTML, JS, WASM, CSS, font, data statis).
// /api/* TIDAK pernah disimpan atau disajikan dari cache: data banjir selalu dari jaringan, dan
// aplikasi sendiri menampilkan "Tidak ada koneksi" / "Data terakhir per …" bila gagal.
// Ubin peta pihak ketiga juga tidak di-cache.
const CACHE = 'hidrosentinel-shell-v1';
const SHELL = [
  '/',
  '/manifest.webmanifest',
  '/favicon.svg',
  '/icons/icon-192.png',
  '/icons/icon-512.png',
  '/assets/theme.js',
  '/assets/pwa.js',
  '/assets/fonts/SchibstedGrotesk-wght.woff2',
  '/vendor/maplibre-gl.js',
  '/vendor/maplibre-gl.css',
  // Dimuat saat start: latar splash, garis sungai, sel DEM lensa Riset.
  '/art/hero-indonesia.webp',
  '/data/rivers.geojson',
  '/data/gem15-cells.json',
];
// Keluaran Trunk berhash (modul, WASM, CSS, snippet wasm-bindgen): aman disajikan dari cache
// selamanya; versi lama dibuang ketika HTML baru tidak lagi merujuknya.
const HASHED = /^\/(?:flood-web-[0-9a-f]+(?:_bg\.wasm|\.js)|main-[0-9a-f]+\.css|snippets\/flood-web-[0-9a-f]+\/[^"'?#]+)$/;

function hashedAssets(html) {
  // Atribut href/src dan string impor pemuat Trunk inline.
  const found = new Set();
  for (const match of html.matchAll(/["'](\/[^"'?#\s]+)["']/g)) {
    if (HASHED.test(match[1])) found.add(match[1]);
  }
  return [...found];
}

async function storeShell(cache, response) {
  const html = await response.clone().text();
  await cache.put('/', response);
  const current = hashedAssets(html);
  await Promise.all(current.map(async (path) => {
    if (!(await cache.match(path))) await cache.add(path);
  }));
  for (const request of await cache.keys()) {
    const path = new URL(request.url).pathname;
    if (HASHED.test(path) && !current.includes(path)) await cache.delete(request);
  }
}

self.addEventListener('install', (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    await cache.addAll(SHELL.filter((path) => path !== '/'));
    const index = await fetch('/', { cache: 'no-cache' });
    if (!index.ok) throw new Error(`index ${index.status}`);
    await storeShell(cache, index);
    await self.skipWaiting();
  })());
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    for (const name of await caches.keys()) {
      if (name.startsWith('hidrosentinel-') && name !== CACHE) await caches.delete(name);
    }
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (url.pathname.startsWith('/api/') || url.pathname === '/sw.js') return;

  if (request.mode === 'navigate') {
    // Jaringan dulu: HTML terbaru menentukan aset berhash; cache hanya saat luring.
    event.respondWith((async () => {
      const cache = await caches.open(CACHE);
      try {
        const response = await fetch(request);
        if (response.ok && url.pathname === '/') event.waitUntil(storeShell(cache, response.clone()));
        return response;
      } catch (error) {
        const cached = await cache.match('/');
        if (cached) return cached;
        throw error;
      }
    })());
    return;
  }

  event.respondWith((async () => {
    const cache = await caches.open(CACHE);
    const cached = await cache.match(request, { ignoreSearch: HASHED.test(url.pathname) });
    if (cached && HASHED.test(url.pathname)) return cached;
    try {
      const response = await fetch(request);
      if (response.ok && response.type === 'basic') event.waitUntil(cache.put(request, response.clone()));
      return response;
    } catch (error) {
      if (cached) return cached;
      throw error;
    }
  })());
});
