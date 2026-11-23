import { test, expect, openApp, screenshot, assertReplayLayout, statusBody } from '../support/test.mjs';
import { replays, buildSnapshot } from '../fixtures/snapshot.mjs';

for (const size of [
  { name: 'desktop', viewport: { width: 1440, height: 900 }, id: 'jabodetabek-2020' },
  { name: 'mobile', viewport: { width: 390, height: 844 }, id: 'pantura-wulan-2024' },
  { name: 'small-mobile', viewport: { width: 360, height: 740 }, id: 'pantura-wulan-2024' },
]) {
  test.describe(size.name, () => {
    test.use({ viewport: size.viewport });
    test('replay keeps event center and real controls uncovered, switches days and exits', async ({ page }, testInfo) => {
      const scenario = replays.find((entry) => entry.id === size.id);
      await openApp(page, `?replay=${scenario.id}`);
      await expect(page.locator('.replay-period')).toHaveText(scenario.period);
      await expect(page.locator('.replay-pick')).toHaveValue(scenario.id);
      await expect(page.locator('.step')).toHaveCount(scenario.frames.length);
      await expect(page.locator('.step[aria-pressed="true"]')).toHaveText(scenario.frames[0].step_label);
      await expect(page.locator('.status-stamp')).toHaveText('Replay historis');
      await expect(page.locator('.tabs [role="tab"]')).toHaveText(['Ringkasan', 'Tentang data']);
      await expect(page.locator('.kpi-strip > div')).toHaveCount(3);
      await expect(page.locator('.replay-outcome li').first()).toBeVisible();
      await expect(page.locator('.replay-cite')).toBeHidden();
      await expect.poll(() => page.evaluate(() => window.__e2eMap.map.getZoom())).toBeCloseTo(scenario.zoom, 1);
      await assertReplayLayout(page, { lon: scenario.center_lon, lat: scenario.center_lat });
      await screenshot(page, testInfo, `replay-${size.name}-compact`);

      // Forty-seven Wulan days exercise horizontal scrolling, not just the first button.
      await page.locator('.step').last().click();
      await expect(page.locator('.step[aria-pressed="true"]')).toHaveText(scenario.frames.at(-1).step_label);
      const visible = await page.locator('.step[aria-pressed="true"]').evaluate((button) => {
        const row = button.closest('.steps').getBoundingClientRect();
        const active = button.getBoundingClientRect();
        return active.left >= row.left - 1 && active.right <= row.right + 1;
      });
      expect(visible, 'Active replay day remains within the scroll strip').toBe(true);
      await page.getByRole('button', { name: 'Putar', exact: true }).click();
      await expect(page.locator('.step[aria-pressed="true"]')).toHaveText(scenario.frames[0].step_label, { timeout: 5_000 });
      await page.getByRole('button', { name: 'Jeda', exact: true }).click();
      await page.locator('.replay-more').click();
      await expect(page.locator('.replay-more')).toHaveAttribute('aria-expanded', 'true');
      await expect(page.locator('.replay-cite')).toBeVisible();
      await expect(page.locator('.replay-cite')).toContainText(scenario.citation);
      await screenshot(page, testInfo, `replay-${size.name}-expanded`);

      await page.locator('.replay-pick').selectOption('barito-2021');
      await expect(page.locator('.replay-period')).toHaveText(replays.find((entry) => entry.id === 'barito-2021').period);
      await expect.poll(() => new URL(page.url()).searchParams.get('replay')).toBe('barito-2021');
      await page.locator('.replay-exit').click();
      await expect(page.locator('.replay')).toHaveCount(0);
      await expect(page.locator('.topbar .status-body')).toHaveText(statusBody(buildSnapshot()));
      await expect.poll(() => new URL(page.url()).searchParams.has('replay')).toBe(false);
    });
  });
}

test.describe('mobile replay station permalink', () => {
  test.use({ viewport: { width: 390, height: 844 } });
  test('raised detail sheet leaves replay controls and selected point visible', async ({ page }, testInfo) => {
    await openApp(page, '?replay=barito-2021&s=barito-banjarmasin');
    await expect(page.locator('.detail h2')).toHaveText('Barito–Martapura · Banjarmasin');
    await expect(page.locator('.inspector')).toHaveClass(/snap-half/);
    await expect(page.locator('.detail .m-lbl')).toHaveText(['Debit simulasi', 'Hujan ERA5 24 jam']);
    await expect(page.locator('.share-row')).toHaveCount(0);
    await expect(page.locator('.replay-narr')).toBeHidden();
    // Camera padding must take BOTH replay and the raised sheet into account.
    await expect.poll(() => page.evaluate(() => window.__e2eMap.map.getZoom())).toBeCloseTo(9.6, 1);
    await assertReplayLayout(page, { lon: 114.588, lat: -3.319 });
    await screenshot(page, testInfo, 'mobile-replay-station-deeplink');
  });
});
