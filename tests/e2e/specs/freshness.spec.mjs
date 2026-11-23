import { test, expect, openApp, screenshot } from '../support/test.mjs';
import { buildSnapshot, FIXTURE_TIME } from '../fixtures/snapshot.mjs';

for (const viewport of [{ width: 1440, height: 900 }, { width: 390, height: 844 }]) {
  test.describe(`refresh ${viewport.width}px`, () => {
    test.use({ viewport, expectedHttpErrors: ['/api/flood'] });
    test('retains last data with an offline/failed-refresh warning, hides it in replay, and recovers', async ({ page, context }, testInfo) => {
      await openApp(page);
      const previousKpis = await page.locator('.kpi-strip').textContent();
      await expect(page.locator('.data-notice')).toHaveCount(0);
      await context.setOffline(true);
      await expect(page.getByRole('alert')).toContainText('Tidak ada koneksi internet.');
      await expect(page.getByRole('alert')).toContainText('Data terakhir per');
      await expect(page.locator('.kpi-strip')).toHaveText(previousKpis);
      await expect(page.getByRole('button', { name: 'Coba perbarui', exact: true })).toBeDisabled();
      await screenshot(page, testInfo, `offline-${viewport.width}`);

      await context.route('**/api/flood', (route) => route.fulfill({ status: 503, body: 'Synthetic refresh outage' }));
      await context.setOffline(false);
      await expect(page.getByRole('alert')).toContainText('Pembaruan data gagal.');
      await expect(page.getByRole('alert')).toContainText('status 503');
      await expect(page.locator('.kpi-strip')).toHaveText(previousKpis);
      await page.getByRole('button', { name: 'Menu', exact: true }).click();
      await page.getByRole('button', { name: /Replay kejadian historis/ }).click();
      await expect(page.locator('.replay')).toBeVisible();
      await expect(page.locator('.data-notice')).toHaveCount(0);
      await page.locator('.replay-exit').click();
      await expect(page.getByRole('alert')).toContainText('Pembaruan data gagal.');

      await context.unroute('**/api/flood');
      await page.getByRole('button', { name: 'Coba perbarui', exact: true }).click();
      await expect(page.locator('.data-notice')).toHaveCount(0);
      await expect(page.locator('.kpi-strip')).toHaveText(previousKpis);
      await screenshot(page, testInfo, `refresh-recovered-${viewport.width}`);
    });
  });
}

test('a fresh snapshot does not conceal degraded sources behind its new timestamp', async ({ page, context }, testInfo) => {
  const degraded = buildSnapshot();
  degraded.upstream.glofas_ok = false;
  degraded.upstream.bmkg_nowcast_ok = false;
  await context.route('**/api/flood', (route) => route.fulfill({ json: degraded }));
  await openApp(page);
  await expect(page.getByRole('alert')).toContainText('Sebagian sumber belum mutakhir.');
  await expect(page.getByRole('alert')).toContainText('GloFAS, BMKG');
  await page.getByRole('tab', { name: 'Tentang data', exact: true }).click();
  await expect(page.getByRole('columnheader', { name: 'Data terakhir berhasil diambil', exact: true })).toBeVisible();
  const row = page.getByRole('row').filter({ has: page.getByRole('rowheader', { name: 'Debit sungai GloFAS', exact: true }) });
  await expect(row).toContainText('Gagal / kedaluwarsa');
  await expect(row).toContainText('22 Nov, 14.00 WIB');
  await screenshot(page, testInfo, 'degraded-sources');
});

test('an old snapshot is labelled even when the HTTP response and source flags say success', async ({ page, context }, testInfo) => {
  const stale = buildSnapshot();
  stale.generated_at_utc = new Date(Date.parse(FIXTURE_TIME) - 31 * 60_000).toISOString();
  await context.route('**/api/flood', (route) => route.fulfill({ json: stale }));
  await openApp(page);
  await expect(page.getByRole('alert')).toContainText('Data belum diperbarui.');
  await expect(page.getByRole('alert')).toContainText('30 menit');
  await expect(page.getByRole('alert')).toContainText('16.29 WIB');
  await expect(page.locator('.kpi-strip > div')).toHaveCount(4);
  await screenshot(page, testInfo, 'stale-snapshot');
});

test.describe('mobile degraded station', () => {
  test.use({ viewport: { width: 360, height: 740 } });
  test('notice does not cover map controls or the selected station behind the sheet', async ({ page, context }, testInfo) => {
    const data = buildSnapshot();
    data.upstream.weather_ok = false;
    const station = data.stations.find((entry) => entry.id === 'ciliwung-manggarai');
    await context.route('**/api/flood', (route) => route.fulfill({ json: data }));
    await openApp(page, `?s=${station.id}`);
    await expect(page.getByRole('button', { name: 'Menu', exact: true })).toBeVisible();
    await expect(page.getByRole('alert')).toContainText('hujan');
    await expect.poll(() => page.evaluate(({ lon, lat }) => {
      const map = window.__e2eMap.map;
      const r = map.getContainer().getBoundingClientRect();
      const point = map.project([lon, lat]);
      const hit = document.elementFromPoint(r.left + point.x, r.top + point.y);
      return { idle: !map.isMoving(), onMap: Boolean(hit && map.getContainer().contains(hit)), canvasFits: Math.abs(map.getCanvas().clientHeight - r.height) < 2 };
    }, station)).toEqual({ idle: true, onMap: true, canvasFits: true });
    for (const name of ['Relief 3D', 'Lokasi saya']) await expect(page.getByRole('button', { name, exact: true })).toBeVisible();
    await screenshot(page, testInfo, 'mobile-degraded-station');
  });
});
