// Render PWA and desktop app icons from crates/flood-web/public/favicon.svg.
// Usage: node scripts/art/render_app_icons.mjs   (PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH optional)
import { chromium } from '@playwright/test';
import { mkdir, readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const favicon = await readFile(resolve(ROOT, 'crates/flood-web/public/favicon.svg'), 'utf8');
const inner = favicon.replace(/^[\s\S]*?<svg[^>]*>/, '').replace(/<\/svg>\s*$/, '')
  .replace(/<rect[^>]*\/>/, ''); // artwork without its rounded background
const bg = /<rect[^>]*fill="([^"]+)"/.exec(favicon)[1];

// "any": the favicon itself (rounded tile, transparent corners).
const rounded = favicon;
// "maskable"/apple: full-bleed background, artwork scaled into the central safe zone
// (platform masks may crop up to a circle of 80% diameter).
const fullBleed = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" fill="none">
  <rect width="64" height="64" fill="${bg}"/>
  <g transform="translate(32 32) scale(0.72) translate(-32 -32)">${inner}</g>
</svg>`;

const outputs = [
  ['crates/flood-web/public/icons/icon-192.png', 192, rounded],
  ['crates/flood-web/public/icons/icon-512.png', 512, rounded],
  ['crates/flood-web/public/icons/icon-maskable-512.png', 512, fullBleed],
  ['crates/flood-web/public/icons/apple-touch-icon.png', 180, fullBleed],
  ['desktop/src-tauri/app-icon.png', 1024, rounded],
];

const browser = await chromium.launch({
  ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}),
});
try {
  const page = await browser.newPage({ deviceScaleFactor: 1 });
  for (const [path, size, svg] of outputs) {
    await page.setViewportSize({ width: size, height: size });
    const sized = svg.replace('<svg ', `<svg width="${size}" height="${size}" `);
    await page.setContent(`<!doctype html><html><body style="margin:0;background:transparent">${sized}</body></html>`);
    await mkdir(dirname(resolve(ROOT, path)), { recursive: true });
    await page.screenshot({ path: resolve(ROOT, path), omitBackground: true, clip: { x: 0, y: 0, width: size, height: size } });
    console.log(`${path} ${size}x${size}`);
  }
} finally {
  await browser.close();
}
