// Periksa koneksi ke situs lalu buka; tanpa koneksi tampilkan pesan jujur dan coba lagi otomatis.
const SITE = 'https://flood.transbulent.id/';
const status = document.getElementById('status');
const offline = document.getElementById('offline');
let busy = false;

async function reachable() {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 8000);
  try {
    // /api/health mengizinkan CORS publik; respons apa pun berarti server terjangkau.
    const response = await fetch(`${SITE}api/health`, { cache: 'no-store', signal: controller.signal });
    return response.status < 500;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

async function connect() {
  if (busy) return;
  busy = true;
  offline.hidden = true;
  status.textContent = 'Menghubungkan ke flood.transbulent.id…';
  if (await reachable()) {
    location.replace(SITE);
    return;
  }
  status.textContent = 'Tidak dapat terhubung ke flood.transbulent.id.';
  offline.hidden = false;
  busy = false;
}

document.getElementById('retry').addEventListener('click', connect);
window.addEventListener('online', connect);
setInterval(() => { if (!offline.hidden) connect(); }, 30000);
connect();
