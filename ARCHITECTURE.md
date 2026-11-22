# Architecture & Scale-Up Guide

## 1. All-Rust Workspace Architecture

`flood-map-in-rust` is a single 3-crate Rust workspace compiling two independent WebAssembly targets (`wasm32-unknown-unknown`) that share identical domain types and deterministic hydrology algorithms:

1. **`crates/flood-shared` (Pure Rust)**
   - Zero platform/browser dependencies (`serde`, `serde_json`, `thiserror`).
   - 50 curated Indonesian river stations (15 Jawa, 13 Sumatera, 9 Kalimantan, 7 Sulawesi, 3 Bali-Nustra, 3 Maluku-Papua) + 12 coastal Rob corridors.
   - Multi-mechanism hydrological classifier: separates **Fluvial Basin Flood** ($Q / Q_{75}$ & 7-day forecast surge), **Urban Pluvial Drainage** (24h/72h rain + 2-layer soil moisture + nearby PetaBencana reports), and **Coastal Rob** (MSL + wave height × subsidence sensitivity).
   - Pure-Rust RSS 2.0 XML parser (BMKG CAP Nowcast + CNN/Tempo/Republika Indonesian flood news), RSS 2.0 alert feed generator, deterministic AI SitRep synthesizer (`Publik`, `BPBD`, `Riset`, `AI Agent`), natural-language query parser, and 3 historical Replay scenarios (`jabodetabek-2020`, `barito-2021`, `pantura-wulan-2024`).

2. **`crates/flood-worker` (Rust Worker + SQLite Durable Object `FloodHub`)**
   - Uses `worker = "0.9"` (`workers-rs`) compiled via `worker-build`.
   - Single-coordinator **SQLite-backed Durable Object** (`new_sqlite_classes = ["FloodHub"]`) running on Cloudflare Workers Free tier (`$0/mo`).
   - Stores live snapshots (`snapshots`), ingestion telemetry (`ingest_log`), and zero-PII push topics (`subscriptions`) inside embedded Durable Object SQLite (`SqlStorage`).
   - Refreshes automatically via `#[event(scheduled)]` every 15 minutes (`*/15 * * * *`) with Durable Object `alarm()` fallback and lazy bootstrap if empty/stale (>20 min).

3. **`crates/flood-web` (Rust Leptos 0.8 CSR WASM + Minimal MapLibre Bridge)**
   - Compiled via `Trunk` (`--release`, `wasm-opt -Oz`) into static `[assets]`.
   - Renders full Coastal Editorial UI, scientific SVG hydrographs (observed + 7d forecast + ensemble uncertainty band + Q75/Q95 baselines), mobile 3-state snap bottom sheet, news reader, free alert hub (`ntfy.sh` + WhatsApp share + RSS), and AI command bar directly in Rust.
   - Only ~190 lines of thin ES glue (`assets/map_bridge.js`) connect `wasm-bindgen` to locally bundled `maplibre-gl` v5.

## 2. Scale-Up Roadmap (When Traffic or Station Count Grows)

- **Horizontal Durable Object Sharding by Island / Wilayah Sungai**: Currently one global coordinator `FloodHub("nusantara-v1")` fits 50 stations + 12 Rob corridors comfortably in ~6 batched upstream fetches per 15 min. To scale to **500+ stations** (e.g., all Balai Wilayah Sungai), shard `FLOOD_HUB` by island (`sumatera`, `jawa`, `kalimantan`, `sulawesi`, `maluku-papua`) + a lightweight aggregator `Cache` API layer at the Worker edge.
- **Real-Time WebSocket Hibernation Alerts**: Cloudflare SQLite Durable Objects natively support **WebSocket Hibernation API** (`state.accept_web_socket`) — enabling live push updates to thousands of open BPBD Posko dashboards with zero idle duration charges.
- **Official Telemetry Connectors**: Add authenticated connectors in `hub.rs` for BBWS/Perum Jasa Tirta AWLR gauges or InaSAFE exposure layers without changing `flood-shared` schemas.
