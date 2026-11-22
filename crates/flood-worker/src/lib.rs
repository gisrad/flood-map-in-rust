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
        let ns = env.durable_object("FLOOD_HUB")?;
        let stub = ns.id_from_name("nusantara-v1")?.get_stub()?;
        let raw = stub.fetch_with_request(req).await?;
        let mut resp = make_mutable(raw)?;
        let headers = resp.headers_mut();
        headers.set("access-control-allow-origin", "*")?;
        apply_security_headers(headers, true)?;
        return Ok(resp);
    }

    // Delegate static frontend (Leptos WASM + MapLibre GL + fonts + SEO files) to ASSETS
    let fetcher = env.assets("ASSETS")?;
    let raw = fetcher.fetch_request(req).await?;
    let mut resp = make_mutable(raw)?;
    let is_html = path == "/" || path.ends_with(".html") || !path.contains('.');
    let headers = resp.headers_mut();
    if is_html {
        headers.set("cache-control", "public, max-age=300")?;
    } else if path.contains(".wasm") || path.contains(".woff2") || path.contains("/vendor/") {
        headers.set("cache-control", "public, max-age=604800")?;
    }
    apply_security_headers(headers, false)?;
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

fn apply_security_headers(headers: &mut Headers, is_api: bool) -> Result<()> {
    headers.set("x-content-type-options", "nosniff")?;
    headers.set("referrer-policy", "strict-origin-when-cross-origin")?;
    headers.set("permissions-policy", "geolocation=(self)")?;
    headers.set("x-frame-options", "SAMEORIGIN")?;
    if is_api {
        headers.set("x-robots-tag", "noindex")?;
    } else {
        // Strict CSP allowing Leptos WASM ('wasm-unsafe-eval'), Trunk inline loader, MapLibre WebGL workers (blob:), and map tiles
        let csp = concat!(
            "default-src 'self'; ",
            "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' https://static.cloudflareinsights.com; ",
            "style-src 'self' 'unsafe-inline'; ",
            "img-src 'self' data: blob: https://*.basemaps.cartocdn.com https://tiles.openfreemap.org https://s3.amazonaws.com https://elevation-tiles-prod.s3.amazonaws.com; ",
            "font-src 'self' data:; ",
            "connect-src 'self' https://*.basemaps.cartocdn.com https://tiles.openfreemap.org https://s3.amazonaws.com https://elevation-tiles-prod.s3.amazonaws.com https://static.cloudflareinsights.com https://cloudflareinsights.com; ",
            "worker-src 'self' blob:; ",
            "child-src 'self' blob:; ",
            "object-src 'none'; ",
            "base-uri 'self'; ",
            "frame-ancestors 'self'"
        );
        headers.set("content-security-policy", csp)?;
    }
    Ok(())
}
