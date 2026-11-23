mod csp;
mod hub;

use flood_shared::ai::{build_llms_txt, build_openapi_spec};
use worker::*;

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let url_host = url.host_str().unwrap_or("");
    let hdr_host = req.headers().get("host")?.unwrap_or_default();
    let is_local = url_host == "127.0.0.1"
        || url_host == "localhost"
        || hdr_host.starts_with("127.0.0.1")
        || hdr_host.starts_with("localhost");

    // Enforce HTTPS redirect in production (non-local)
    if !is_local && url.scheme() == "http" {
        if matches!(req.method(), Method::Get | Method::Head) {
            let mut https_url = url.clone();
            let _ = https_url.set_scheme("https");
            return Response::redirect_with_status(https_url, 308);
        }
        return Response::error("HTTPS required", 403);
    }

    let path = url.path();

    // Machine-readable AI discovery endpoints served directly at edge
    if path == "/llms.txt" {
        let base = format!(
            "{}://{}",
            url.scheme(),
            url.host_str().unwrap_or("flood.transbulent.id")
        );
        let mut resp = Response::ok(build_llms_txt(&base))?;
        let headers = resp.headers_mut();
        headers.set("content-type", "text/plain; charset=utf-8")?;
        headers.set("cache-control", "public, max-age=3600")?;
        apply_security_headers(headers, false)?;
        return Ok(resp);
    }

    if path == "/api/openapi.json" {
        let base = format!(
            "{}://{}",
            url.scheme(),
            url.host_str().unwrap_or("flood.transbulent.id")
        );
        let mut resp = Response::from_json(&build_openapi_spec(&base))?;
        let headers = resp.headers_mut();
        headers.set("cache-control", "public, max-age=3600")?;
        headers.set("access-control-allow-origin", "*")?;
        apply_security_headers(headers, true)?;
        return Ok(resp);
    }

    // Route all dynamic /api/* traffic to the SQLite-backed FloodHub Durable Object
    if path.starts_with("/api/") {
        // CORS hanya untuk endpoint baca GET/HEAD; POST (langganan) tidak boleh dipanggil lintas situs.
        let cors = matches!(req.method(), Method::Get | Method::Head) && is_public_read(path);
        let ns = env.durable_object("FLOOD_HUB")?;
        let stub = ns.id_from_name("nusantara-v1")?.get_stub()?;
        let raw = stub.fetch_with_request(req).await?;
        let mut resp = make_mutable(raw)?;
        let headers = resp.headers_mut();
        if cors {
            headers.set("access-control-allow-origin", "*")?;
        }
        apply_security_headers(headers, true)?;
        return Ok(resp);
    }

    // Delegate static frontend (Leptos WASM + MapLibre GL + fonts + SEO files) to ASSETS
    let fetcher = env.assets("ASSETS")?;
    let mut raw = fetcher.fetch_request(req).await?;
    let is_html = path == "/" || path.ends_with(".html") || !path.contains('.');
    let html_body = raw.status_code() == 200
        && raw
            .headers()
            .get("content-type")?
            .is_some_and(|ct| ct.contains("text/html"));
    // Skrip inline (pemuat Trunk) diizinkan lewat hash, bukan 'unsafe-inline'. Hash dihitung dari
    // HTML yang benar-benar disajikan, sehingga tetap sah setelah setiap build.
    let (mut resp, script_hashes) = if html_body {
        let html = raw.text().await?;
        let hashes = csp::inline_script_hashes(&html);
        (Response::from_html(html)?, hashes)
    } else {
        (make_mutable(raw)?, Vec::new())
    };
    let headers = resp.headers_mut();
    if is_html {
        headers.set("cache-control", "public, max-age=300")?;
    } else if path == "/sw.js" {
        // Service worker harus selalu diperiksa ulang agar pembaruan cangkang cepat terpasang.
        headers.set("cache-control", "no-cache")?;
        headers.set("content-type", "text/javascript; charset=utf-8")?;
    } else if path == "/manifest.webmanifest" {
        headers.set("cache-control", "public, max-age=3600")?;
        headers.set("content-type", "application/manifest+json")?;
    } else if path.contains(".wasm") || path.contains(".woff2") || path.contains("/vendor/") {
        headers.set("cache-control", "public, max-age=604800")?;
    } else if path.starts_with("/art/") || path.starts_with("/data/") {
        // Nama berkas tanpa hash: segarkan di latar belakang.
        headers.set(
            "cache-control",
            "public, max-age=86400, stale-while-revalidate=604800",
        )?;
    }
    apply_security_headers(headers, false)?;
    if !script_hashes.is_empty() {
        headers.set(
            "content-security-policy",
            &csp::content_security_policy(&script_hashes),
        )?;
    }
    Ok(resp)
}

#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    if let Ok(ns) = env.durable_object("FLOOD_HUB") {
        if let Ok(id) = ns.id_from_name("nusantara-v1") {
            if let Ok(stub) = id.get_stub() {
                let mut init = RequestInit::new();
                init.with_method(Method::Post);
                if let Ok(req) = Request::new_with_init("https://internal/internal/ingest", &init) {
                    let _ = stub.fetch_with_request(req).await;
                }
            }
        }
    }
}

fn make_mutable(resp: Response) -> Result<Response> {
    let status = resp.status_code();
    let headers = Headers::new();
    for (k, v) in resp.headers().entries() {
        let _ = headers.set(&k, &v);
    }
    let (_, body) = resp.into_parts();
    Ok(Response::from_body(body)?
        .with_status(status)
        .with_headers(headers))
}

/// Endpoint baca publik yang boleh dipanggil dari situs lain.
fn is_public_read(path: &str) -> bool {
    matches!(
        path,
        "/api/flood"
            | "/api/stations.geojson"
            | "/api/health"
            | "/api/openapi.json"
            | "/api/ai/briefing"
            | "/api/ai/query"
            | "/api/alerts.rss"
    ) || path.starts_with("/api/station/")
}

fn apply_security_headers(headers: &mut Headers, is_api: bool) -> Result<()> {
    headers.set("x-content-type-options", "nosniff")?;
    headers.set("referrer-policy", "strict-origin-when-cross-origin")?;
    headers.set("permissions-policy", "geolocation=(self)")?;
    headers.set("x-frame-options", "SAMEORIGIN")?;
    // Tanpa includeSubDomains: subdomain lain transbulent.id dilayani di luar Worker ini.
    headers.set("strict-transport-security", "max-age=31536000")?;
    if is_api {
        headers.set("x-robots-tag", "noindex")?;
    } else {
        headers.set(
            "content-security-policy",
            &csp::content_security_policy(&[]),
        )?;
    }
    Ok(())
}
