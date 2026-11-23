# Security Policy

## Architecture & Privacy Boundaries

- **Zero-secret public feeds**: Live ingestion pulls open public feeds (Open-Meteo Copernicus GloFAS v4, Open-Meteo Forecast/Marine, PetaBencana.id, BMKG Weather Nowcast CAP RSS, Indonesian national news RSS) without private API keys.
- **SQLite parameterization in Durable Objects**: All `FloodHub` SQLite queries (`state.storage().sql().exec(...)`) use positional parameter bindings (`?`).
- **Zero PII / No Phone Numbers**: The free push notification system (`ntfy.sh` topics) explicitly validates topic names (`8–64` alphanumeric/`-`/`_` chars) and rejects numeric phone-number-like topics. Subscriptions are unauthenticated, so an existing topic cannot be re-subscribed (`409`) and `POST` requires `Content-Type: application/json` (`415`). Browser geolocation (`Lokasi`) computes station proximity purely inside Rust/WASM on the client and never sends coordinates to the server.
- **Strict Edge Security Headers**: `crates/flood-worker/src/lib.rs` and `csp.rs` enforce HTTPS `308` redirects, `Strict-Transport-Security: max-age=31536000`, `Content-Security-Policy` (`script-src 'self' 'wasm-unsafe-eval'` plus SHA-256 hashes of the inline Trunk loader computed from the served HTML, no `'unsafe-inline'` for scripts; `form-action 'self'`; `worker-src 'self' blob:`; DEM tiles limited to the `elevation-tiles-prod` bucket), `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy: geolocation=(self)`, and `X-Robots-Tag: noindex` on `/api/*`. CORS (`Access-Control-Allow-Origin: *`) is set only on read-only `GET`/`HEAD` endpoints. News and BMKG links are accepted only with an `https://` scheme.
- **Environment Hygiene**: `wrangler.toml`, `.env*`, and `.dev.vars` are excluded via `.gitignore`. Only sanitized `crates/flood-worker/wrangler.toml.example` is tracked.

## Supported Versions

Only the `main` branch and live deployment at `https://flood.transbulent.id` are supported.

## Reporting a Vulnerability

Please report security issues privately via GitHub Security Advisories:

- <https://github.com/gisrad/flood-map-in-rust/security/advisories/new>
