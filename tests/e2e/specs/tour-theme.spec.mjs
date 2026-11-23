import { test, expect, openApp, screenshot } from '../support/test.mjs';

for (const size of [
  { name: 'desktop', viewport: { width: 1440, height: 900 } },
  { name: 'mobile', viewport: { width: 390, height: 844 } },
]) {
  test.describe(`${size.name} first visit`, () => {
    test.use({ viewport: size.viewport, tourSeen: false });
    test('all six tour cards stay onscreen, keyboard works and completion persists', async ({ page }, testInfo) => {
      await openApp(page);
      const dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();
      await expect(page.getByRole('button', { name: 'Mulai tur', exact: true })).toBeFocused();
      for (let step = 1; step <= 6; step++) {
        await expect(page.locator('.tour-count')).toHaveText(`${step} dari 6`);
        // Tour recalculates its layout after a 260 ms timer; wait for that actual late update.
        await page.waitForTimeout(300);
        await expect(dialog).toBeInViewport({ ratio: 1 });
        await expect(page.locator('.tour-title')).not.toBeEmpty();
        await expect(page.locator('.tour-spot')).toHaveCount(step === 1 ? 0 : 1);
        if (step === 1 || step === 6) await screenshot(page, testInfo, `tour-${size.name}-step-${step}`);
        await page.keyboard.press(step === 1 || step === 6 ? 'Enter' : 'ArrowRight');
      }
      await expect(dialog).toHaveCount(0);
      expect(await page.evaluate(() => localStorage.getItem('hidrosentinel-tour'))).toBe('v1');
      await page.reload({ waitUntil: 'load' });
      await expect(page.locator('.kpi-strip')).toHaveCount(1);
      // Auto-open, if regressed, would happen 1200 ms after the first snapshot.
      await page.waitForTimeout(1500);
      await expect(dialog).toHaveCount(0);
      await page.locator('.menu-btn').click();
      await page.getByRole('button', { name: /Tur singkat fitur/ }).click();
      await expect(page.locator('.tour-count')).toHaveText('1 dari 6');
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);
    });
  });
}

test.describe('deep-link first visit', () => {
  test.use({ tourSeen: false });
  test('point link never auto-opens the tour, but the menu can open it', async ({ page }) => {
    await openApp(page, '?s=ciliwung-manggarai');
    await expect(page.locator('.detail')).toHaveCount(1);
    await page.waitForTimeout(1500);
    await expect(page.locator('.tour-card')).toHaveCount(0);
    await page.locator('.menu-btn').click();
    await page.getByRole('button', { name: /Tur singkat fitur/ }).click();
    await expect(page.locator('.tour-card')).toBeVisible();
    await page.getByRole('button', { name: 'Lewati', exact: true }).click();
    await expect(page.locator('.tour-card')).toHaveCount(0);
  });
});

const background = (page) => page.evaluate(() => {
  const map = window.__e2eMap.map;
  const layer = map.getStyle().layers.find((entry) => entry.type === 'background');
  return map.getPaintProperty(layer.id, 'background-color');
});

test.describe('system theme', () => {
  test.use({ colorScheme: 'dark' });
  test('tracks the system until manual selection, recolors real map and preserves explicit permalink', async ({ page }, testInfo) => {
    await openApp(page);
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    const darkBackground = await background(page);
    await page.emulateMedia({ colorScheme: 'light' });
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    await expect.poll(() => background(page)).not.toBe(darkBackground);
    const lightBackground = await background(page);
    expect(new URL(page.url()).searchParams.has('theme')).toBe(false);
    await page.getByRole('button', { name: 'Mode gelap', exact: true }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await expect.poll(() => background(page)).toBe(darkBackground);
    await expect.poll(() => new URL(page.url()).searchParams.get('theme')).toBe('dark');
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.emulateMedia({ colorScheme: 'light' });
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await screenshot(page, testInfo, 'manual-dark-synthetic');
    await page.reload({ waitUntil: 'load' });
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    // A fresh unqualified visit goes back to the system: manual theme is NOT localStorage.
    await openApp(page);
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    await expect.poll(() => background(page)).toBe(lightBackground);
  });
});
