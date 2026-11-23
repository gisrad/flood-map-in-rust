// Browsers only expose serviceWorker in secure contexts (HTTPS or localhost).
if ('serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch((error) => console.warn('Service worker tidak terpasang:', error));
  });
}
