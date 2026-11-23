import { test as base, expect } from '@playwright/test';
import { startFixtureServer } from '../fixtures/server.mjs';
import { buildSnapshot, FIXTURE_TIME } from '../fixtures/snapshot.mjs';

export { expect };
export const test = base.extend({
  server: [async ({}, use) => {
    const server = await startFixtureServer();
    try { await use(server); } finally { await server.close(); }
  }, { scope: 'worker' }],
  baseURL: async ({ server }, use) => use(server.origin),
  tourSeen: [true, { option: true }],
  scenario: ['mixed', { option: true }],
  expectedHttpErrors: [[], { option: true }],
  page: async ({ page }, use) => {
    // Freeze Date only: timers and real MapLibre animation still run normally.
    await page.clock.setFixedTime(new Date(FIXTURE_TIME));
    await use(page);
  },
  context: async ({ context, server, tourSeen, scenario, expectedHttpErrors }, use, testInfo) => {
    const issues = [];
    const allowed = (url) => expectedHttpErrors.some((part) => url.includes(part))
      // Missing optional OSM detail files are explicitly handled by map_bridge.js.
      || /\/data\/rivers\/[^/]+\.geojson$/.test(url);
    context.on('weberror', (event) => issues.push({ type: 'pageerror', text: event.error().stack || event.error().message }));
    context.on('console', (message) => {
      if (message.type() !== 'error') return;
      const url = message.location().url;
      if (allowed(url) && /Failed to load resource/i.test(message.text())) return;
      issues.push({ type: 'console', url, text: message.text() });
    });
    context.on('requestfailed', (request) => {
      const text = request.failure()?.errorText || '';
      // Navigation and theme changes legitimately cancel superseded resource requests.
      if (/ERR_ABORTED/.test(text) || allowed(request.url())) return;
      issues.push({ type: 'requestfailed', url: request.url(), text });
    });
    context.on('response', (response) => {
      if (response.status() >= 400 && !allowed(response.url())) {
        issues.push({ type: 'http', url: response.url(), status: response.status() });
      }
    });
    await context.route('**/*', async (route) => {
      const url = new URL(route.request().url());
      if (url.origin === server.origin) return route.continue();
      if (url.href === 'https://tiles.openfreemap.org/styles/positron') {
        // Only replace the network basemap dependency. Production JS/WASM and MapLibre remain
        // unchanged; the local style still exercises real data layers and theme rebuilding.
        const response = await context.request.get(`${server.origin}/__e2e__/style.json`);
        return route.fulfill({ response });
      }
      issues.push({ type: 'external-network', url: url.href });
      return route.abort('blockedbyclient');
    });
    if (scenario === 'empty') {
      await context.route('**/api/flood', (route) => route.fulfill({ json: buildSnapshot('empty') }));
    }
    await context.addInitScript(({ seen }) => {
      if (seen) {
        try { localStorage.setItem('hidrosentinel-tour', 'v1'); } catch { /* opaque initial origin */ }
      }
      // Read-only instrumentation: retain the real MapLibre instance so assertions can inspect
      // rendered data and project camera targets. No controls, app state or DOM are fabricated.
      const probe = { map: null, error: null };
      Object.defineProperty(window, '__e2eMap', { value: probe });
      let library;
      Object.defineProperty(window, 'maplibregl', {
        configurable: true,
        get: () => library,
        set(value) {
          library = value;
          try {
            const RealMap = value.Map;
            value.Map = new Proxy(RealMap, {
              construct(target, args) {
                const instance = Reflect.construct(target, args);
                probe.map = instance;
                return instance;
              },
            });
          } catch (error) { probe.error = String(error); }
        },
      });
    }, { seen: tourSeen });
    try {
      await use(context);
    } finally {
      await testInfo.attach('browser-diagnostics', {
        body: Buffer.from(JSON.stringify(issues, null, 2)), contentType: 'application/json',
      });
      expect(issues, 'Uncaught errors, failed resources or unisolated external requests').toEqual([]);
    }
  },
});

export async function openApp(page, query = '') {
  await page.goto(`/${query}`, { waitUntil: 'load' });
  await expect(page.locator('#splash')).toHaveCount(0);
  await expect(page.locator('.app-shell')).toBeVisible();
  await expect.poll(() => page.evaluate(() => {
    const map = window.__e2eMap?.map;
    return Boolean(map && map.isStyleLoaded() && map.getLayer('stations-icon') && !map.isMoving());
  }), { message: 'Real MapLibre map loads with the local style and production data layers' }).toBe(true);
  await page.evaluate(() => document.fonts.ready);
}

export async function screenshot(page, testInfo, name) {
  const path = testInfo.outputPath(`${name}.png`);
  await page.screenshot({ path, fullPage: false });
  await testInfo.attach(name, { path, contentType: 'image/png' });
}

export async function mapData(page, source = 'stations') {
  return page.evaluate(async (id) => {
    const data = await window.__e2eMap.map.getSource(id).getData();
    return data.features.map((feature) => feature.properties);
  }, source);
}

export function elevated(counts) {
  return counts.high + counts.very_high + counts.extreme;
}

export function statusBody(snapshot) {
  const phrase = (counts) => [['extreme', 'Ekstrem'], ['very_high', 'Sangat tinggi'], ['high', 'Tinggi']]
    .filter(([key]) => counts[key] > 0).map(([key, label]) => `${counts[key]} ${label}`).join(', ');
  const k = snapshot.kpis;
  return [phrase(k.river) ? `Sungai: ${phrase(k.river)}` : 'Tidak ada sungai di atas ambang',
    phrase(k.pluvial) && `Hujan lokal: ${phrase(k.pluvial)}`, phrase(k.rob) && `Rob: ${phrase(k.rob)}`,
    `${k.bmkg_nowcasts} peringatan cuaca BMKG`, `${k.citizen_flood_reports_24h} laporan banjir warga`]
    .filter(Boolean).join(' · ');
}

export async function assertReplayLayout(page, target) {
  // Hit-testing checks actual stacking/overflow, not just display:none or CSS declarations.
  const facts = await page.evaluate(({ lon, lat }) => {
    const rect = (selector) => {
      const el = document.querySelector(selector);
      if (!el || getComputedStyle(el).display === 'none') return null;
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) return null;
      return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
    };
    const panel = document.querySelector('.replay');
    const canvas = rect('#map-canvas');
    const point = window.__e2eMap.map.project([lon, lat]);
    const x = canvas.left + point.x;
    const y = canvas.top + point.y;
    const hit = document.elementFromPoint(x, y);
    const controls = [...document.querySelectorAll('.tool-rail button, .seg button, .replay-exit, .replay-pick, .replay-steps .play')]
      .filter((el) => el.getBoundingClientRect().width && getComputedStyle(el).display !== 'none')
      .map((el) => {
        const r = el.getBoundingClientRect();
        const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return { label: el.getAttribute('aria-label') || el.textContent.trim(), reachable: Boolean(top && (top === el || el.contains(top))) };
      });
    return {
      replay: rect('.replay'), inspector: rect('.inspector'), rail: rect('.tool-rail'), seg: rect('.seg'),
      viewport: { width: innerWidth, height: innerHeight }, overflow: panel.scrollWidth - panel.clientWidth,
      controls, target: { x, y, onMap: Boolean(hit && document.querySelector('#map-canvas').contains(hit)) },
    };
  }, target);
  const r = facts.replay;
  expect(r).not.toBeNull();
  expect(r.left).toBeGreaterThanOrEqual(0);
  expect(r.top).toBeGreaterThanOrEqual(0);
  expect(r.right).toBeLessThanOrEqual(facts.viewport.width + 1);
  expect(r.bottom).toBeLessThanOrEqual(facts.viewport.height + 1);
  expect(facts.overflow, 'Replay must not overflow horizontally').toBeLessThanOrEqual(1);
  for (const key of ['inspector', 'rail', 'seg']) {
    const other = facts[key];
    if (!other) continue;
    const area = Math.max(0, Math.min(r.right, other.right) - Math.max(r.left, other.left))
      * Math.max(0, Math.min(r.bottom, other.bottom) - Math.max(r.top, other.top));
    expect(area, `Replay must not cover ${key}`).toBeLessThanOrEqual(1);
  }
  expect(facts.controls.length).toBeGreaterThanOrEqual(10);
  expect(facts.controls.filter((control) => !control.reachable), 'Every visible map/replay control receives pointer hits').toEqual([]);
  expect(facts.target.x).toBeGreaterThan(0);
  expect(facts.target.y).toBeGreaterThan(0);
  expect(facts.target.x).toBeLessThan(facts.viewport.width);
  expect(facts.target.y).toBeLessThan(facts.viewport.height);
  expect(facts.target.onMap, 'The actual projected event/point center must remain on uncovered map').toBe(true);
}
