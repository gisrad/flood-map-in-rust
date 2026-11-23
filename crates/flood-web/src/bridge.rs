use crate::format::{fmt_datetime_short, parse_utc_ms};
use flood_shared::model::{FloodMechanism, FloodSnapshot};
use serde_json::json;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/assets/map_bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name = initMap)]
    pub fn js_init_map(container_id: &str, theme: &str, on_select: &Closure<dyn FnMut(String)>);

    #[wasm_bindgen(js_name = updateMapData)]
    pub fn js_update_map_data(
        stations_json: &str,
        robs_json: &str,
        reports_json: &str,
        selected_id: &str,
    );

    #[wasm_bindgen(js_name = flyToPoint)]
    pub fn js_fly_to_point(lon: f64, lat: f64, zoom: f64);

    #[wasm_bindgen(js_name = resetIndonesiaView)]
    pub fn js_reset_indonesia_view();

    #[wasm_bindgen(js_name = setMapTheme)]
    pub fn js_set_map_theme(theme: &str);

    #[wasm_bindgen(js_name = toggle3DTerrain)]
    pub fn js_toggle_3d_terrain(enabled: bool);

    #[wasm_bindgen(js_name = setInundationLayer)]
    pub fn js_set_inundation_layer(enabled: bool);

    #[wasm_bindgen(js_name = setUserLocation)]
    pub fn js_set_user_location(lon: f64, lat: f64);

    #[wasm_bindgen(js_name = clearUserLocation)]
    pub fn js_clear_user_location();
}

/// GeoJSON titik untuk peta: (stasiun sungai, rob, laporan warga).
/// Bentuk ikon mengikuti mekanisme (`mech`), warna mengikuti `level`.
/// Filter `FluvialBasin`/`UrbanPluvial` menampilkan semua stasiun dengan level mekanisme itu.
pub fn build_map_geojsons(
    snap: &FloodSnapshot,
    mech_filter: Option<FloodMechanism>,
) -> (String, String, String) {
    let station_features: Vec<_> = if mech_filter == Some(FloodMechanism::CoastalRob) {
        Vec::new()
    } else {
        snap.stations
            .iter()
            .map(|s| {
                let (level, mech) = match mech_filter {
                    Some(FloodMechanism::FluvialBasin) => (s.fluvial_level, "fluvial"),
                    Some(FloodMechanism::UrbanPluvial) => (s.pluvial_level, "pluvial"),
                    _ => (
                        s.level,
                        if s.mechanism == FloodMechanism::UrbanPluvial {
                            "pluvial"
                        } else {
                            "fluvial"
                        },
                    ),
                };
                json!({
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [s.lon, s.lat] },
                    "properties": {
                        "id": s.id,
                        "name": s.name,
                        "level": level.slug(),
                        "level_label": level.label_id(),
                        "river_level": s.fluvial_level.slug(),
                        "mech": mech
                    }
                })
            })
            .collect()
    };

    let rob_features: Vec<_> = if matches!(mech_filter, None | Some(FloodMechanism::CoastalRob)) {
        snap.coastal_rob
            .iter()
            .map(|r| {
                json!({
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [r.lon, r.lat] },
                    "properties": {
                        "id": r.id,
                        "name": r.name,
                        "level": r.level.slug(),
                        "level_label": format!("Rob · {}", r.level.label_id())
                    }
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    let report_features: Vec<_> = snap
        .citizen_reports
        .iter()
        .map(|rp| {
            let mut meta = rp.source.clone();
            if let Some(ms) = parse_utc_ms(&rp.created_at) {
                meta = format!("{meta} · {}", fmt_datetime_short(ms, 7));
            }
            if let Some(d) = rp.depth_cm {
                meta = format!("{meta} · kedalaman ±{d} cm");
            }
            json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [rp.lon, rp.lat] },
                "properties": {
                    "id": rp.pkey,
                    "title": if rp.title.trim().is_empty() { "Laporan banjir warga".to_string() } else { rp.title.clone() },
                    "meta": meta,
                    "text": rp.text
                }
            })
        })
        .collect();

    let wrap = |feats: Vec<serde_json::Value>| {
        serde_json::to_string(&json!({ "type": "FeatureCollection", "features": feats }))
            .unwrap_or_else(|_| r#"{"type":"FeatureCollection","features":[]}"#.to_string())
    };

    (
        wrap(station_features),
        wrap(rob_features),
        wrap(report_features),
    )
}

pub fn set_html_theme(theme: &str) {
    if let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let _ = root.set_attribute("data-theme", theme);
    }
}

pub fn remove_splash() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("splash"))
    {
        el.remove();
    }
}

pub fn prefers_dark() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        .map(|m| m.matches())
        .unwrap_or(false)
}

pub fn copy_text(text: &str) {
    if let Some(win) = web_sys::window() {
        let _ = win.navigator().clipboard().write_text(text);
    }
}

pub fn download_text_file(filename: &str, mime: &str, content: &str) {
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let arr = js_sys::Array::of1(&JsValue::from_str(content));
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type(mime);
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&arr, &bag) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Ok(el) = doc.create_element("a") {
        if let Ok(a) = el.dyn_into::<web_sys::HtmlAnchorElement>() {
            a.set_href(&url);
            a.set_download(filename);
            a.click();
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

pub fn open_whatsapp_text(message: &str) {
    let Some(win) = web_sys::window() else { return };
    let encoded: String = js_sys::encode_uri_component(message).into();
    let url = format!("https://wa.me/?text={encoded}");
    let _ = win.open_with_url_and_target_and_features(&url, "_blank", "noopener");
}

pub fn read_url_param(key: &str) -> Option<String> {
    let win = web_sys::window()?;
    let search = win.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get(key).filter(|v| !v.is_empty())
}

pub fn update_url_param(key: &str, val: Option<&str>) {
    let Some(win) = web_sys::window() else { return };
    let Ok(search) = win.location().search() else {
        return;
    };
    let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
        return;
    };
    match val {
        Some(v) if !v.is_empty() => params.set(key, v),
        _ => params.delete(key),
    }
    let qs: String = params.to_string().into();
    let pathname = win
        .location()
        .pathname()
        .unwrap_or_else(|_| "/".to_string());
    let next_url = if qs.is_empty() {
        pathname
    } else {
        format!("{pathname}?{qs}")
    };
    if let Ok(hist) = win.history() {
        let _ = hist.replace_state_with_url(&JsValue::NULL, "", Some(&next_url));
    }
}

/// URL halaman saat ini (dengan state permalink).
pub fn current_url() -> String {
    web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_else(|| "https://flood.transbulent.id/".to_string())
}

/// URL kanonis untuk sebuah titik.
pub fn station_url(id: &str) -> String {
    let origin = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "https://flood.transbulent.id".to_string());
    let encoded: String = js_sys::encode_uri_component(id).into();
    format!("{origin}/?s={encoded}")
}
