import { test, expect, openApp, screenshot } from '../support/test.mjs';

test.describe('installable app shell', () => {
  test.use({ serviceWorkers: 'allow', expectedHttpErrors: ['/api/flood', '/sw.js'] });

  test('manifest and icons are valid; the shell opens offline without fabricating flood data', async ({ page, context }, testInfo) => {
    await openApp(page);
    const manifest = await page.evaluate(async () => {
      const href = document.querySelector('link[rel="manifest"]').href;
      const response = await fetch(href);
      return { type: response.headers.get('content-type'), body: await response.json() };
    });
    expect(manifest.type).toContain('application/manifest+json');
    expect(manifest.body).toMatchObject({ id: '/', start_url: '/', scope: '/', display: 'standalone', short_name: 'Hidrosentinel' });
    expect(manifest.body.description).toContain('bukan peringatan resmi');
    for (const icon of [...manifest.body.icons, ...manifest.body.screenshots]) {
      const size = await page.evaluate(async (src) => {
        const response = await fetch(src);
        if (!response.ok) return `HTTP ${response.status}`;
        if (src.endsWith('.svg')) return 'any';
        const bitmap = await createImageBitmap(await response.blob());
        return `${bitmap.width}x${bitmap.height}`;
      }, icon.src);
      expect(size, icon.src).toBe(icon.sizes);
    }
    expect(manifest.body.icons.some((icon) => icon.purpose === 'maskable' && icon.sizes === '512x512')).toBe(true);

    // The worker caches the current hashed shell, never /api/*.
    await page.evaluate(() => navigator.serviceWorker.ready);
    await expect.poll(() => page.evaluate(async () => {
      const keys = [];
      for (const name of await caches.keys()) {
        for (const request of await (await caches.open(name)).keys()) keys.push(new URL(request.url).pathname);
      }
      return keys;
    })).toEqual(expect.arrayContaining(['/', '/vendor/maplibre-gl.js', expect.stringMatching(/^\/flood-web-[0-9a-f]+_bg\.wasm$/),
      expect.stringMatching(/^\/snippets\/flood-web-[0-9a-f]+\/assets\/map_bridge\.js$/), expect.stringMatching(/^\/main-[0-9a-f]+\.css$/)]));
    const apiCached = await page.evaluate(async () => {
      for (const name of await caches.keys()) {
        for (const request of await (await caches.open(name)).keys()) if (new URL(request.url).pathname.startsWith('/api/')) return true;
      }
      return false;
    });
    expect(apiCached).toBe(false);

    await context.setOffline(true);
    await page.reload({ waitUntil: 'load' });
    await expect(page.locator('#splash')).toHaveCount(0);
    await expect(page.locator('.app-shell')).toBeVisible();
    const alert = page.getByRole('alert');
    // Chromium's offline emulation may still report navigator.onLine on a fresh document; either
    // honest message is acceptable, but never cached or fabricated flood data.
    await expect(alert).toContainText(/Tidak ada koneksi internet\.|Data belum dapat dimuat\./);
    await expect(alert).not.toContainText('Data terakhir per');
    await expect(page.locator('.kpi-strip > div')).toHaveCount(0);
    await screenshot(page, testInfo, 'offline-shell');
    await context.setOffline(false);
  });
});
