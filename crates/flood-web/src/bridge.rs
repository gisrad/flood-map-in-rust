use flood_shared::model::{FloodMechanism, FloodSnapshot};
use serde_json::json;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/assets/map_bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name = initMap)]
    pub fn js_init_map(
        container_id: &str,
        theme: &str,
        on_select: &Closure<dyn FnMut(String)>,
    );

    #[wasm_bindgen(js_name = updateMapData)]
    pub fn js_update_map_data(
        rivers_json: &str,
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

    #[wasm_bindgen(js_name = setUserLocation)]
    pub fn js_set_user_location(lon: f64, lat: f64);

    #[wasm_bindgen(js_name = clearUserLocation)]
    pub fn js_clear_user_location();
}

pub fn build_map_geojsons(
    snap: &FloodSnapshot,
    mech_filter: Option<FloodMechanism>,
) -> (String, String, String, String) {
    let river_features: Vec<_> = snap
        .stations
        .iter()
        .filter(|s| {
            mech_filter
                .map(|m| m != FloodMechanism::CoastalRob && s.mechanism == m)
                .unwrap_or(true)
        })
        .map(|s| {
            json!({
                "type": "Feature",
                "geometry": { "type": "LineString", "coordinates": s.river_polyline },
                "properties": {
                    "id": s.id,
                    "alert_rank": s.alert_level.rank(),
                    "ratio": s.anomaly_ratio
                }
            })
        })
        .collect();

    let station_features: Vec<_> = snap
        .stations
        .iter()
        .filter(|s| {
            mech_filter
                .map(|m| m != FloodMechanism::CoastalRob && s.mechanism == m)
                .unwrap_or(true)
        })
        .map(|s| {
            json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [s.lon, s.lat] },
                "properties": {
                    "id": s.id,
                    "name": s.name,
                    "alert_rank": s.alert_level.rank(),
                    "mechanism": s.mechanism.slug(),
                    "ratio": s.anomaly_ratio
                }
            })
        })
        .collect();

    let rob_features: Vec<_> = snap
        .coastal_rob
        .iter()
        .filter(|_| {
            mech_filter
                .map(|m| m == FloodMechanism::CoastalRob)
                .unwrap_or(true)
        })
        .map(|r| {
            json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [r.lon, r.lat] },
                "properties": {
                    "id": r.id,
                    "name": r.name,
                    "alert_rank": r.alert_level.rank()
                }
            })
        })
        .collect();

    let report_features: Vec<_> = snap
        .citizen_reports
        .iter()
        .map(|rp| {
            let depth = rp
                .depth_cm
                .map(|d| format!(" · kedalaman ~{d} cm"))
                .unwrap_or_default();
            json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [rp.lon, rp.lat] },
                "properties": {
                    "id": rp.pkey,
                    "title": rp.title,
                    "meta": format!("{} ({}){}", rp.source, rp.status, depth),
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
        wrap(river_features),
        wrap(station_features),
        wrap(rob_features),
        wrap(report_features),
    )
}

pub fn set_html_theme(theme: &str) {
    if let Some(win) = web_sys::window() {
        if let Some(doc) = win.document() {
            if let Some(root) = doc.document_element() {
                let _ = root.set_attribute("data-theme", theme);
            }
        }
    }
}

pub fn copy_text(text: &str) {
    if let Some(win) = web_sys::window() {
        let nav = win.navigator();
        let cb = nav.clipboard();
        let _ = cb.write_text(text);
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
    let _ = win.open_with_url_and_target(&url, "_blank");
}

pub fn read_url_param(key: &str) -> Option<String> {
    let win = web_sys::window()?;
    let search = win.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get(key)
}

pub fn update_url_param(key: &str, val: Option<&str>) {
    let Some(win) = web_sys::window() else { return };
    let Ok(search) = win.location().search() else { return };
    let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
        return;
    };
    match val {
        Some(v) if !v.is_empty() => params.set(key, v),
        _ => params.delete(key),
    }
    let qs: String = params.to_string().into();
    let pathname = win.location().pathname().unwrap_or_else(|_| "/".to_string());
    let next_url = if qs.is_empty() {
        pathname
    } else {
        format!("{pathname}?{qs}")
    };
    if let Ok(hist) = win.history() {
        let _ = hist.replace_state_with_url(&JsValue::NULL, "", Some(&next_url));
    }
}
