import { test, expect, openApp, screenshot, mapData, elevated, statusBody } from '../support/test.mjs';
import { buildSnapshot, MARKER } from '../fixtures/snapshot.mjs';

const snapshot = buildSnapshot();
const kpis = (data) => ({
  'Sungai ≥ Tinggi': String(elevated(data.kpis.river)),
  'Hujan lokal ≥ Tinggi': String(elevated(data.kpis.pluvial)),
  'Rob ≥ Tinggi': String(elevated(data.kpis.rob)),
  'Laporan warga 24 jam': String(data.kpis.citizen_flood_reports_24h),
});
const readKpis = (page) => page.locator('.kpi-strip > div').evaluateAll((rows) => Object.fromEntries(
  rows.map((row) => [row.querySelector('dt').textContent, row.querySelector('dd').textContent]),
));

test('live-mode status, independent KPIs, disclaimer and real map data', async ({ page }, testInfo) => {
  await openApp(page);
  await expect(page.locator('.topbar .status-body')).toHaveText(statusBody(snapshot));
  await expect(page.locator('.status-stamp')).toHaveText('Minggu, 22 Nov 2026 · 17.00 WIB');
  await expect.poll(() => readKpis(page)).toEqual(kpis(snapshot));
  await expect(page.locator('.disclaimer')).toContainText('Indikasi model, bukan peringatan resmi.');
  await expect(page.locator('.legend')).toContainText('bukan status resmi');
  const priorities = page.locator('section.block').filter({ has: page.getByRole('heading', { name: 'Perlu diperhatikan', exact: true }) });
  const expected = [...snapshot.stations, ...snapshot.coastal_rob]
    .filter((point) => ['high', 'very_high', 'extreme'].includes(point.level)).map((point) => point.name).sort();
  expect((await priorities.locator('.point-row b').allTextContents()).sort()).toEqual(expected);
  expect(await mapData(page)).toHaveLength(snapshot.stations.length);
  expect(await mapData(page, 'robs')).toHaveLength(snapshot.coastal_rob.length);
  expect(await mapData(page, 'reports')).toHaveLength(snapshot.citizen_reports.length);
  await expect.poll(() => page.evaluate(() => window.__e2eMap.map.queryRenderedFeatures({ layers: ['stations-icon'] }).length))
    .toBeGreaterThan(0);
  await screenshot(page, testInfo, 'live-desktop-synthetic');

  await page.getByRole('tab', { name: 'Berita', exact: true }).click();
  await expect(page.locator('.news-item').first()).toContainText(MARKER);
  await expect(page.getByRole('heading', { name: `Laporan banjir warga 24 jam (${snapshot.citizen_reports.length})` })).toBeVisible();
  await page.getByRole('tab', { name: 'Tentang data', exact: true }).click();
  await expect(page.locator('.about')).toContainText(MARKER);
});

test('station permalink opens real detail, suppresses tour and updates URL on return', async ({ page }, testInfo) => {
  const station = snapshot.stations.find((point) => point.id === 'ciliwung-manggarai');
  await openApp(page, `?s=${station.id}&m=sungai&theme=dark`);
  await expect(page.locator('.detail h2')).toHaveText(station.name);
  await expect(page.locator('.detail .reason')).toHaveText(station.level_reason);
  await expect(page.locator('.detail .meaning p')).toHaveText(station.public_advice);
  await expect(page.locator('.hydro-svg')).toHaveCount(1);
  await expect(page.locator('.hg-fc')).toHaveCount(1);
  await expect(page.locator('.hg-band-iqr')).toHaveCount(1);
  await expect(page.locator('.seg-btn[aria-pressed="true"]')).toHaveText('Sungai');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.locator('.tour-card')).toHaveCount(0);
  await screenshot(page, testInfo, 'live-station-deeplink-synthetic');
  await page.getByRole('button', { name: '← Semua titik', exact: true }).click();
  await expect(page.locator('.detail')).toHaveCount(0);
  await expect.poll(() => new URL(page.url()).searchParams.has('s')).toBe(false);
  await page.locator('.seg-btn').filter({ hasText: /^Rob$/ }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('m')).toBe('rob');
  await expect(page.getByRole('heading', { name: `Semua titik pantau (${snapshot.coastal_rob.length})`, exact: true })).toHaveCount(1);
  expect(await mapData(page)).toHaveLength(0);
});

test.describe('empty feeds', () => {
  test.use({ scenario: 'empty' });
  test('does not turn absent news/reports or elevated points into an alert', async ({ page }, testInfo) => {
    await openApp(page);
    await expect.poll(() => readKpis(page)).toEqual(kpis(buildSnapshot('empty')));
    await expect(page.locator('.status-body')).toHaveText(statusBody(buildSnapshot('empty')));
    await expect(page.getByText('Tidak ada titik pada level Tinggi atau lebih saat ini.', { exact: true })).toBeVisible();
    await expect(page.getByText('Tidak ada peringatan dini cuaca BMKG yang aktif.', { exact: true })).toBeVisible();
    await page.getByRole('tab', { name: 'Berita', exact: true }).click();
    await expect(page.getByText('Tidak ada berita banjir 72 jam terakhir.', { exact: true })).toBeVisible();
    await expect(page.getByText('Tidak ada laporan banjir warga di PetaBencana.id dalam 24 jam terakhir.', { exact: true })).toBeVisible();
    await screenshot(page, testInfo, 'empty-feeds-synthetic');
  });
});

test.describe('unavailable API', () => {
  test.use({ expectedHttpErrors: ['/api/flood'] });
  test('shows the actual HTTP error, not a fabricated safe status; replay remains usable', async ({ context, page }, testInfo) => {
    await context.route('**/api/flood', (route) => route.fulfill({ status: 503, contentType: 'text/plain', body: 'Synthetic test outage' }));
    await openApp(page);
    await expect(page.getByRole('alert')).toContainText('Data belum dapat dimuat.');
    await expect(page.getByRole('alert')).toContainText('Server mengembalikan status 503.');
    await expect(page.locator('.topbar .status-body')).toHaveText('Data belum tersedia');
    await expect(page.locator('.kpi-strip')).toHaveCount(0);
    await screenshot(page, testInfo, 'api-unavailable');
    await page.locator('.menu-btn').click();
    await page.getByRole('button', { name: /Replay kejadian historis/ }).click();
    await expect(page.locator('.replay')).toBeVisible();
    await expect(page.locator('.status-stamp')).toHaveText('Replay historis');
    await expect(page.locator('.kpi-strip > div')).toHaveCount(3);
  });
});
