# Security Policy

## Architecture & Privacy Boundaries

- **Zero-secret public feeds**: Live ingestion pulls open public feeds (Open-Meteo Copernicus GloFAS v4, Open-Meteo Forecast/Marine, PetaBencana.id, BMKG Weather Nowcast CAP RSS, Indonesian national news RSS) without private API keys.
- **SQLite parameterization in Durable Objects**: All `FloodHub` SQLite queries (`state.storage().sql().exec(...)`) use positional parameter bindings (`?`).
- **Zero PII / No Phone Numbers**: The free push notification system (`ntfy.sh` topics) explicitly validates topic names (`4–48` alphanumeric/`-`/`_` chars) and rejects numeric phone-number-like topics. Browser geolocation (`Lokasi`) computes station proximity purely inside Rust/WASM on the client and never sends coordinates to the server.
- **Strict Edge Security Headers**: `crates/flood-worker/src/lib.rs` enforces HTTPS `308` redirects, `Content-Security-Policy` (`script-src 'self' 'wasm-unsafe-eval'`, `worker-src 'self' blob:`), `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy: geolocation=(self)`, and `X-Robots-Tag: noindex` on `/api/*`.
- **Environment Hygiene**: `wrangler.toml`, `.env*`, and `.dev.vars` are excluded via `.gitignore`. Only sanitized `crates/flood-worker/wrangler.toml.example` is tracked.

## Supported Versions

Only the `main` branch and live deployment at `https://flood.transbulent.id` are supported.

## Reporting a Vulnerability

Please report security issues privately via GitHub Security Advisories:

- <https://github.com/gisrad/flood-map-in-rust/security/advisories/new>
