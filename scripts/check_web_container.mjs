import { chromium, expect } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';

const base = process.argv[2] ?? 'http://127.0.0.1:8080';
const out = process.argv[3] ?? 'target/web-container-proof';
await mkdir(out, { recursive: true });
const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const page = await context.newPage();
const errors = [];
const failedResponses = [];
page.on('response', async response => {
  if (response.url().startsWith(base) && response.status() >= 400) {
    failedResponses.push({ url: response.url(), status: response.status(), headers: response.headers(), body: await response.text() });
  }
});
page.on('pageerror', error => errors.push(String(error)));
page.on('console', message => {
  if (message.type() === 'error' && /Content Security Policy|Refused to execute/i.test(message.text())) {
    errors.push(message.text());
  }
});
try {
  await page.goto(base);
  await expect(page.locator('.kpi-strip > div')).toHaveCount(4, { timeout: 30000 });
  await expect(page.locator('.maplibregl-canvas')).toBeVisible();
  const skip = page.getByRole('button', { name: 'Lewati', exact: true });
  if (await skip.isVisible()) await skip.click();
  const responses = await page.evaluate(async () => {
    const result = {};
    for (const path of ['/healthz', '/api/health', '/api/flood', '/api/openapi.json', '/llms.txt', '/sw.js', '/manifest.webmanifest', '/missing-container-asset.js']) {
      const r = await fetch(path);
      result[path] = { status: r.status, type: r.headers.get('content-type'), cache: r.headers.get('cache-control') };
      if (path === '/api/health' || path === '/api/flood') result[path].body = await r.json();
    }
    return result;
  });
  for (const [path, r] of Object.entries(responses)) expect(r.status, path).toBe(path === '/missing-container-asset.js' ? 404 : 200);
  expect(responses['/api/health'].body.ok).toBe(true);
  expect(responses['/api/flood'].body.schema_version).toBe(2);
  expect(responses['/api/flood'].body.stations).toHaveLength(50);
  expect(responses['/sw.js'].cache).toBe('no-cache');
  expect(responses['/manifest.webmanifest'].type).toContain('application/manifest+json');
  const provenance = await page.evaluate(async () => (await fetch('/BUILD_PROVENANCE.json')).json());
  await page.screenshot({ path: `${out}/desktop.png` });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole('button', { name: 'Menu', exact: true })).toBeVisible();
  await page.screenshot({ path: `${out}/mobile.png` });
  expect(errors).toEqual([]);
  const report = { source_commit: provenance.source_commit, api: 'https://flood.transbulent.id', pageErrors: errors, responses };
  await writeFile(`${out}/report.json`, JSON.stringify(report, null, 2));
  console.log(`Container web UI, live API, MIME, CSP and missing-asset checks passed; web source ${provenance.source_commit}`);
} catch (error) {
  await page.screenshot({ path: `${out}/failure.png` });
  await writeFile(`${out}/failure.json`, JSON.stringify({ error: String(error), pageErrors: errors, failedResponses }, null, 2));
  throw error;
} finally {
  await browser.close();
}
