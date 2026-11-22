# Hidrosentinel Nusantara — Indonesia Flood & River Discharge Observatory (`flood.transbulent.id`)

**100% Rust Full-Stack (`Leptos` CSR WebAssembly + `workers-rs` + Cloudflare SQLite Durable Objects + MapLibre GL v5 3D)**

Observatorium banjir dan hidrometeorologi *real-time* seluruh Indonesia yang memisahkan tiga mekanisme utama banjir — **Banjir Kiriman Hulu DAS**, **Genangan Drainase Kota**, dan **Banjir Rob Pesisir** — dengan hidrograf ensemble 21 hari, berita banjir regional RSS, peringatan dini BMKG Nowcast, kanal notifikasi gratis (`ntfy.sh` + WhatsApp Share + RSS), serta API ramah *AI Agent* (`/llms.txt` & `/api/openapi.json`).

---

## Architecture / Arsitektur

```mermaid
flowchart LR
    subgraph Upstream["Open Telemetry & RSS Feeds (0 API Keys)"]
        GL["Open-Meteo Copernicus GloFAS v4\n(50 River Basins · 14d Obs + 7d Forecast)"]
        WX["Open-Meteo IFS/ERA5\n(Rain 24h/72h & 2-Layer Soil Moisture)"]
        MR["Open-Meteo Marine\n(12 Coastal Rob MSL & Waves)"]
        PB["PetaBencana.id\n(24h Citizen Flood Reports)"]
        BM["BMKG CAP Nowcast RSS"]
        NW["Indonesian National & Regional RSS\n(CNN Indonesia, Tempo.co, Republika)"]
    end

    subgraph Edge["Cloudflare Workers Free Plan ($0/mo)"]
        WK["crates/flood-worker\n(Rust wasm32 · HTTPS + CSP + /api/* Router)"]
        DO["Durable Object: FloodHub\n(Embedded SQLite SqlStorage + 15m Cron/Alarm)"]
        AS["Static [assets]\n(crates/flood-web · Leptos 0.8 WASM + MapLibre GL v5)"]
        NT["ntfy.sh Free Push\n(Zero-PII Topic Alerts)"]
    end

    GL & WX & MR & PB & BM & NW -->|"Batched ~8 subrequests / 15m"| DO
    WK <-->|"RPC + SQLite Snapshots"| DO
    WK -->|"Serves SPA & /api/*"| AS
    DO -->|"Deduplicated Threshold Alerts"| NT
```

---

## Preview (Desktop & Mobile)

> Live URL: **[https://flood.transbulent.id/](https://flood.transbulent.id/)**

*(Preview screenshots generated via headless Chromium audit in [`docs/`](docs/))*

| Desktop (`1440×900`) | Mobile (`390×844`) |
|---|---|
| ![Desktop Preview](docs/preview-desktop.png) | ![Mobile Preview](docs/preview-mobile.png) |

---

## Ringkasan Bahasa Indonesia

**Hidrosentinel Nusantara** dibangun seluruhnya dalam bahasa **Rust** dari *backend* Cloudflare Worker hingga antarmuka *frontend* WebAssembly (hanya menjembatani kanvas WebGL **MapLibre GL v5** melalui modul ES tipis).

### Keunggulan Utama
1. **Pemisahan 3 Mekanisme Banjir (Ilmiah & Deterministik)**
   * **Banjir Kiriman Hulu DAS**: rasio debit $Q / Q_{75}$, estimasi ambang $Q_{95}$, tren 24 jam, dan prakiraan lonjakan 7 hari (*lead-time watch*) berbasis **Copernicus GloFAS v4**.
   * **Drainase & Genangan Kota**: kombinasi curah hujan 24j/72j, kejenuhan air tanah 2 lapis (`0–7 cm` dan `28–100 cm`), serta laporan warga *real-time* dari **PetaBencana.id**.
   * **Banjir Rob Pesisir**: pemantauan *mean sea level* (MSL) + tinggi gelombang di **12 koridor pesisir rawan amblesan** (Jakarta Utara, Muara Gembong, Pekalongan, Semarang–Sayung, Belawan, Pontianak, Banjarmasin, Makassar, Merauke, dll.).
2. **All-Rust + SQLite Durable Object (`FloodHub`)**
   * Data disimpan di **Cloudflare Durable Object SQLite** (`new_sqlite_classes = ["FloodHub"]`) — aktif otomatis di paket gratis Cloudflare Workers tanpa server terpisah.
   * Berbagi tipe data & logika hidrologi antara server dan klien lewat crate `flood-shared`.
3. **Multi-Perspektif + Mobile-First + Berita & Peringatan Gratis**
   * Switcher perspektif (**Publik**, **BPBD/Pemda**, **Akademisi/Riset**, **AI Agent**).
   * **Kanal Berita Banjir**: agregasi otomatis berita banjir nasional/regional + peringatan dini cuaca ekstrem **BMKG Nowcast CAP**.
   * **Peringatan Gratis Tanpa Biaya SMS/WA Gateway**: langganan notifikasi push HP gratis via **`ntfy.sh`** (tanpa menyimpan nomor HP/PII), tombol **Bagikan WhatsApp 1-Klik**, dan **RSS Feed Wilayah (`/api/alerts.rss`)** siap sambung ke bot Telegram/n8n.
4. **Mode Replay Historis (Terpisah Jelas dari Data Live)**
   * Putar ulang skenario **Jabodetabek 1 Jan 2020 (377 mm/hari)**, **DAS Barito Kalsel Jan 2021**, dan **Tanggul Wulan–Demak 2024** dengan label `DATA REPLAY HISTORIS`.
5. **AI-Driven & Ramah Agen Eksternal**
   * Kolom **AI Tanya Data** di aplikasi (`GET /api/ai/query?q=...`) mengarahkan kamera peta & filter secara otomatis.
   * Mendukung discovery agen eksternal lewat **`/llms.txt`**, **`/api/openapi.json`**, dan **`/api/ai/briefing`**.

---

## AI-Driven & Machine-Readable Endpoints

| Endpoint | Purpose |
|---|---|
| `GET /llms.txt` | Concise LLM discovery manifest with hydrological caveats |
| `GET /api/openapi.json` | OpenAPI 3.1 schema for AI agents & tool integrations |
| `GET /api/flood` | Full snapshot: 50 river stations, 12 coastal Rob corridors, PetaBencana reports, BMKG nowcasts, news |
| `GET /api/station/{id}` | 21-day observed + 7-day forecast GloFAS hydrograph & soil telemetry |
| `GET /api/ai/briefing?region=&audience=public\|gov\|academia\|ai_agent&format=json\|md` | Deterministic multi-audience SitRep briefing |
| `GET /api/ai/query?q=...` | Natural-language Indonesian/English river & alert search with map `fly_to` coordinates |
| `GET /api/alerts.rss?region=...` | Regional/national RSS 2.0 warning feed |
| `POST /api/alerts/subscribe` | Register a zero-PII `ntfy.sh` push notification topic |
| `GET /api/health` | SQLite Durable Object freshness & upstream health |

---

## Local Development & Deployment

### Prerequisites
* Rust toolchain (`rustc` + `cargo`) with `wasm32-unknown-unknown` target:
  ```bash
  rustup target add wasm32-unknown-unknown
  cargo install trunk --locked
  cargo install worker-build --locked
  ```
* Wrangler CLI (`npx wrangler` or global `wrangler`)

### Test & Check
```bash
cargo test -p flood-shared
cargo check -p flood-worker --target wasm32-unknown-unknown
cargo check -p flood-web --target wasm32-unknown-unknown
```

### Build & Deploy to Cloudflare Workers
1. Copy `crates/flood-worker/wrangler.toml.example` to `crates/flood-worker/wrangler.toml` (gitignored) and fill your Cloudflare `account_id` and domain routes.
2. Build the Leptos WASM frontend:
   ```bash
   cd crates/flood-web
   trunk build --release
   ```
3. Build and deploy the Rust Worker + SQLite Durable Object + static `[assets]`:
   ```bash
   cd ../flood-worker
   npx wrangler deploy
   ```

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the horizontal Durable Object sharding and WebSocket Hibernation scale-up roadmap.

---

## License & Attribution

* Code: **MIT License** ([`LICENSE`](LICENSE)).
* Open datasets & bundled assets: see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) and [`CITATION.cff`](CITATION.cff).
