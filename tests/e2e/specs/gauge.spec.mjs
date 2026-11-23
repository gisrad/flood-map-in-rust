import { test, expect, openApp, screenshot } from '../support/test.mjs';
import { gauges } from '../fixtures/snapshot.mjs';

const fact = (page, label) => page.locator('.detail .facts > div')
  .filter({ has: page.locator('dt').filter({ hasText: new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`) }) }).locator('dd');

test('Riset retains Bojonegoro published-Q2 weakness instead of calling it operational validation', async ({ page }, testInfo) => {
  const check = gauges.stations['solo-bojonegoro'];
  // These caveats are applicable because the repository explicitly supplies this comparison.
  expect(check?.flood?.kind).toBe('published');
  expect(check.flood.status).toBe('preliminary');
  await openApp(page, '?s=solo-bojonegoro&lens=riset');
  await expect(page.locator('.lens-btn[aria-pressed="true"]')).toHaveText('Riset');
  await expect(fact(page, 'Pos duga air (GRDC)')).toContainText(check.gauge);
  await expect(fact(page, 'Q2 pos (makalah terbit)')).toHaveCount(1);
  const quality = fact(page, 'Mutu uji Q2');
  await quality.scrollIntoViewIfNeeded();
  await expect(quality).toContainText('Pendahuluan, lebih lemah dari data pos');
  await expect(quality).toContainText('tahun dan panjang rekaman tidak dilaporkan');
  await expect(quality).toContainText('tidak dapat diperiksa ulang');
  await expect(quality).toContainText(check.flood.citation);
  await expect(fact(page, 'Q2 GloFAS / terukur')).toContainText('bukan bukti ambang akurat');
  await screenshot(page, testInfo, 'riset-bojonegoro-published-caveat');
});

test('Jabon mean-flow caveat remains public; Riset never fabricates an unassessed flood Q2', async ({ page }, testInfo) => {
  const check = gauges.stations['brantas-mojokerto'];
  expect(check.gauge).toContain('Jabon');
  expect(check.flood).toBeNull();
  await openApp(page, '?s=brantas-mojokerto');
  const caveat = page.locator('.detail > p.muted').filter({ hasText: 'pos duga air Jabon' });
  await expect(caveat).toContainText('kemungkinan lebih besar dari aliran sebenarnya');
  await expect(caveat).toContainText(check.gauge_years);
  await screenshot(page, testInfo, 'jabon-public-caveat-synthetic');
  await page.getByRole('button', { name: 'Riset', exact: true }).click();
  await expect.poll(() => new URL(page.url()).searchParams.get('lens')).toBe('riset');
  const gauge = fact(page, 'Pos duga air (GRDC)');
  await gauge.scrollIntoViewIfNeeded();
  await expect(gauge).toContainText(check.gauge);
  await expect(fact(page, 'Mutu uji Q2')).toHaveCount(0);
  await expect(fact(page, 'Q2 GloFAS / terukur')).toHaveCount(0);
  await expect(page.locator('.detail .facts dt').filter({ hasText: /^Estimasi Q2 pos/ })).toHaveCount(0);
  await screenshot(page, testInfo, 'riset-jabon-unassessed');
});
