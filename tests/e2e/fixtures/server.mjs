// Test-only HTTP origin: serves unchanged production dist plus a synthetic /api/flood.
// This is NOT a backend emulator and never performs ingestion, subscriptions or external I/O.
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { readFile, stat } from 'node:fs/promises';
import { resolve, relative, extname, isAbsolute } from 'node:path';
import { ROOT, buildSnapshot, repoText } from './snapshot.mjs';

export const DIST = resolve(ROOT, 'crates/flood-web/dist');
const mime = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8', '.wasm': 'application/wasm', '.json': 'application/json',
  '.geojson': 'application/geo+json', '.svg': 'image/svg+xml', '.png': 'image/png',
  '.webp': 'image/webp', '.jpg': 'image/jpeg', '.woff2': 'font/woff2', '.txt': 'text/plain; charset=utf-8',
};

// Read the actual worker policy rather than maintaining a second policy. These literals are
// deliberately narrow; an unsupported Rust refactor fails startup with an actionable message.
function policyFor(html) {
  const source = repoText('crates/flood-worker/src/csp.rs');
  const body = source.slice(source.indexOf('pub fn content_security_policy('));
  const literal = '"((?:[^"\\\\]|\\\\[\\s\\S])*)"';
  const script = new RegExp(`String::from\\(${literal}\\)`).exec(body);
  const dem = new RegExp(`const DEM: &str = ${literal};`).exec(body);
  const template = new RegExp(`format!\\(\\s*${literal}`).exec(body);
  if (!script || !dem || !template) throw new Error('Update the E2E worker CSP literal reader for csp.rs changes.');
  const decode = (text) => text.replace(/\\\r?\n\s*/g, '').replace(/\\"/g, '"').replace(/\\\\/g, '\\');
  const hashes = [];
  for (const match of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi)) {
    if (/src\s*=|application\/ld\+json/i.test(match[1]) || !match[2].trim()) continue;
    hashes.push(`'sha256-${createHash('sha256').update(match[2]).digest('base64')}'`);
  }
  return decode(template[1]).replaceAll('{DEM}', decode(dem[1]))
    .replace('{script_src}', [decode(script[1]), ...hashes].join(' '));
}

export async function startFixtureServer() {
  let index;
  try {
    index = await readFile(resolve(DIST, 'index.html'), 'utf8');
  } catch (error) {
    throw new Error('Production dist is required. Run trunk build --release --locked in crates/flood-web before npm run test:e2e.', { cause: error });
  }
  if (!index.includes('.wasm') || !index.includes('type="module"')) {
    throw new Error('dist/index.html is not a built Trunk production page.');
  }
  const csp = policyFor(index);
  const mixed = JSON.stringify(buildSnapshot());
  const server = createServer(async (request, response) => {
    const send = (status, type, body) => {
      response.writeHead(status, {
        'content-type': type, 'cache-control': 'no-store', 'x-content-type-options': 'nosniff',
        'content-security-policy': csp, 'referrer-policy': 'strict-origin-when-cross-origin',
      });
      response.end(request.method === 'HEAD' ? undefined : body);
    };
    try {
      if (!['GET', 'HEAD'].includes(request.method)) return send(405, 'text/plain', 'Test server is read-only.');
      const url = new URL(request.url, 'http://127.0.0.1');
      if (url.pathname === '/api/flood') return send(200, 'application/json', mixed);
      if (url.pathname.startsWith('/api/')) return send(404, 'text/plain', 'No synthetic fixture for this API route.');
      if (url.pathname === '/__e2e__/style.json') {
        const origin = `http://${request.headers.host}`;
        return send(200, 'application/json', JSON.stringify({
          version: 8, name: 'Synthetic E2E background (not a geographic basemap)',
          glyphs: `${origin}/__e2e__/glyphs/{fontstack}/{range}.pbf`, sources: {},
          layers: [{ id: 'synthetic-background', type: 'background', paint: { 'background-color': '#F5F3EE' } }],
        }));
      }
      // Valid empty protobuf: labels intentionally have no glyphs. MapLibre's real station icons,
      // selected rings and GeoJSON sources still render; no UI, map library or app state is faked.
      if (url.pathname.startsWith('/__e2e__/glyphs/') && url.pathname.endsWith('.pbf')) {
        return send(200, 'application/x-protobuf', Buffer.alloc(0));
      }
      const path = resolve(DIST, `.${decodeURIComponent(url.pathname === '/' ? '/index.html' : url.pathname)}`);
      const rel = relative(DIST, path);
      if (rel.startsWith('..') || isAbsolute(rel)) return send(403, 'text/plain', 'Forbidden');
      try {
        if (!(await stat(path)).isFile()) return send(404, 'text/plain', 'Not found');
        return send(200, mime[extname(path)] || 'application/octet-stream', await readFile(path));
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
        return send(404, 'text/plain', 'Not found');
      }
    } catch (error) {
      send(500, 'text/plain', `E2E fixture server failed: ${error.message}`);
    }
  });
  await new Promise((resolveReady, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveReady);
  });
  return {
    origin: `http://127.0.0.1:${server.address().port}`,
    close: () => new Promise((resolveClosed, reject) => {
      server.close((error) => error ? reject(error) : resolveClosed());
      server.closeAllConnections();
    }),
  };
}
