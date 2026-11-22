use crate::bridge::{
    build_map_geojsons, js_clear_user_location, js_fly_to_point, js_init_map,
    js_reset_indonesia_view, js_set_map_theme, js_set_user_location, js_toggle_3d_terrain,
    js_update_map_data, read_url_param, set_html_theme, update_url_param,
};
use crate::components::inspector::{InspectorDrawer, InspectorTab, SheetSnap};
use crate::components::replay_bar::ReplayBar;
use crate::components::top_bar::TopBar;
use flood_shared::catalog::RIVER_STATIONS;
use flood_shared::hydrology::haversine_km;
use flood_shared::model::{Audience, FloodMechanism, FloodSnapshot};
use flood_shared::replay::build_replay_snapshot;
use gloo_net::http::Request;
use gloo_timers::callback::Interval;
use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn App() -> impl IntoView {
    let initial_theme = read_url_param("theme")
        .filter(|t| t == "dark" || t == "light")
        .unwrap_or_else(|| "light".to_string());
    let initial_aud = read_url_param("perspective")
        .map(|s| Audience::from_str_lossy(&s))
        .unwrap_or(Audience::Public);
    let initial_mech = read_url_param("mechanism").and_then(|s| match s.as_str() {
        "fluvial" => Some(FloodMechanism::FluvialBasin),
        "pluvial" => Some(FloodMechanism::UrbanPluvial),
        "rob" => Some(FloodMechanism::CoastalRob),
        _ => None,
    });
    let initial_basin = read_url_param("basin");
    let initial_replay = read_url_param("replay");

    let live_snapshot = RwSignal::new(Option::<FloodSnapshot>::None);
    let loading = RwSignal::new(true);
    let error_msg = RwSignal::new(Option::<String>::None);

    let selected_id = RwSignal::new(initial_basin);
    let mech_filter = RwSignal::new(initial_mech);
    let audience = RwSignal::new(initial_aud);
    let theme = RwSignal::new(initial_theme.clone());
    let terrain_3d = RwSignal::new(false);

    let replay_active = RwSignal::new(initial_replay.is_some());
    let scenario_id = RwSignal::new(initial_replay.unwrap_or_else(|| "jabodetabek-2020".to_string()));
    let frame_idx = RwSignal::new(0usize);
    let playing = RwSignal::new(false);

    let active_tab = RwSignal::new(InspectorTab::Summary);
    let drawer_open = RwSignal::new(true);
    let mobile_snap = RwSignal::new(SheetSnap::Peek);
    let ai_focus_tick = RwSignal::new(0u32);

    // User GPS card state
    let user_card_msg = RwSignal::new(Option::<String>::None);
    let user_coords = RwSignal::new(Option::<(f64, f64)>::None);

    // Fetch initial live snapshot from Cloudflare Durable Object Worker
    spawn_local(async move {
        match Request::get("/api/flood").send().await {
            Ok(resp) if resp.ok() => match resp.json::<FloodSnapshot>().await {
                Ok(snap) => {
                    live_snapshot.set(Some(snap));
                    loading.set(false);
                }
                Err(e) => {
                    error_msg.set(Some(format!("Gagal parsing JSON: {e}")));
                    loading.set(false);
                }
            },
            Ok(resp) => {
                error_msg.set(Some(format!("API /api/flood HTTP {}", resp.status())));
                loading.set(false);
            }
            Err(e) => {
                error_msg.set(Some(format!("Gagal memuat telemetri: {e}")));
                loading.set(false);
            }
        }
    });

    let active_snapshot = Signal::derive(move || {
        if replay_active.get() {
            build_replay_snapshot(&scenario_id.get(), frame_idx.get())
        } else {
            live_snapshot.get()
        }
    });

    let sync_label = Signal::derive(move || {
        if replay_active.get() {
            "REPLAY HISTORIS".to_string()
        } else if loading.get() {
            "Memuat D.O.…".to_string()
        } else if let Some(s) = live_snapshot.get() {
            if s.generated_at_utc.len() >= 16 {
                format!("Live · {} UTC", &s.generated_at_utc[11..16])
            } else {
                "Live · D.O. SQLite".to_string()
            }
        } else {
            "Offline".to_string()
        }
    });

    // Initialize MapLibre once mounted
    Effect::new(move |_| {
        set_html_theme(&theme.get_untracked());
        let cb = Closure::wrap(Box::new(move |id: String| {
            selected_id.set(Some(id.clone()));
            drawer_open.set(true);
            mobile_snap.set(SheetSnap::Half);
            if let Some(snap) = active_snapshot.get_untracked() {
                if let Some(st) = snap.stations.iter().find(|s| s.id == id) {
                    js_fly_to_point(st.lon, st.lat, 8.8);
                } else if let Some(rb) = snap.coastal_rob.iter().find(|r| r.id == id) {
                    js_fly_to_point(rb.lon, rb.lat, 9.2);
                }
            }
        }) as Box<dyn FnMut(String)>);
        js_init_map("map-canvas", &theme.get_untracked(), &cb);
        cb.forget();
    });

    // Keep HTML attribute, MapLibre style, 3D terrain, and URL params in sync
    Effect::new(move |_| {
        let th = theme.get();
        set_html_theme(&th);
        js_set_map_theme(&th);
        update_url_param("theme", if th == "dark" { Some("dark") } else { None });
    });

    Effect::new(move |_| {
        js_toggle_3d_terrain(terrain_3d.get());
    });

    Effect::new(move |_| {
        let sel = selected_id.get();
        update_url_param("basin", sel.as_deref());
    });

    Effect::new(move |_| {
        let m = mech_filter.get();
        update_url_param("mechanism", m.map(|x| x.slug()));
    });

    Effect::new(move |_| {
        let a = audience.get();
        let slug = match a {
            Audience::Public => None,
            Audience::Gov => Some("gov"),
            Audience::Academia => Some("academia"),
            Audience::AiAgent => Some("ai"),
        };
        update_url_param("perspective", slug);
    });

    // Push updated GeoJSONs to MapLibre whenever snapshot, filter, or selection changes
    Effect::new(move |_| {
        if let Some(snap) = active_snapshot.get() {
            let (rivers, stations, robs, reports) = build_map_geojsons(&snap, mech_filter.get());
            let sel = selected_id.get().unwrap_or_default();
            js_update_map_data(&rivers, &stations, &robs, &reports, &sel);
        }
    });

    // Replay interval playback
    let interval_handle = StoredValue::new_local(Option::<Interval>::None);
    Effect::new(move |_| {
        let is_play = playing.get() && replay_active.get();
        interval_handle.set_value(None);
        if is_play {
            let h = Interval::new(2200, move || {
                let sc_id = scenario_id.get_untracked();
                let count = flood_shared::replay::list_replay_scenarios()
                    .into_iter()
                    .find(|s| s.id == sc_id)
                    .map(|s| s.frames.len())
                    .unwrap_or(1);
                frame_idx.update(|i| *i = (*i + 1) % count.max(1));
            });
            interval_handle.set_value(Some(h));
        }
    });

    let toggle_replay = move || {
        replay_active.update(|active| {
            *active = !*active;
            if *active {
                playing.set(false);
                frame_idx.set(0);
                let sc = flood_shared::replay::list_replay_scenarios()
                    .into_iter()
                    .find(|s| s.id == scenario_id.get_untracked());
                if let Some(meta) = sc {
                    js_fly_to_point(meta.center_lon, meta.center_lat, meta.zoom);
                    update_url_param("replay", Some(meta.id));
                }
            } else {
                playing.set(false);
                update_url_param("replay", None);
                js_reset_indonesia_view();
            }
        });
    };

    let trigger_gps = move || {
        let Some(win) = web_sys::window() else { return };
        let Ok(geo) = win.navigator().geolocation() else {
            user_card_msg.set(Some("Peramban tidak mendukung geolokasi.".to_string()));
            return;
        };
        let snap_now = active_snapshot.get_untracked();
        let ok_cb = Closure::once(move |pos: web_sys::Position| {
            let coords = pos.coords();
            let lat = coords.latitude();
            let lon = coords.longitude();
            let acc = coords.accuracy();
            user_coords.set(Some((lon, lat)));
            js_set_user_location(lon, lat);
            js_fly_to_point(lon, lat, 9.5);

            // Find nearest monitored river station
            let mut best_name = "Tidak ada stasiun terdekat";
            let mut best_km = f64::MAX;
            let mut best_alert = "";
            if let Some(s) = &snap_now {
                for st in &s.stations {
                    let d = haversine_km(lat, lon, st.lat, st.lon);
                    if d < best_km {
                        best_km = d;
                        best_name = RIVER_STATIONS
                            .iter()
                            .find(|x| x.id == st.id)
                            .map(|x| x.name)
                            .unwrap_or("DAS");
                        best_alert = st.alert_level.label();
                    }
                }
            }
            user_card_msg.set(Some(format!(
                "Lokasi Anda (±{:.0} m) · Stasiun terdekat: {} (~{:.1} km · {})",
                acc, best_name, best_km, best_alert
            )));
        });
        let err_cb = Closure::once(move |_err: wasm_bindgen::JsValue| {
            user_card_msg.set(Some("Akses lokasi ditolak atau tidak tersedia.".to_string()));
        });
        let opts = web_sys::PositionOptions::new();
        opts.set_enable_high_accuracy(false);
        opts.set_timeout(8000);
        let _ = geo.get_current_position_with_error_callback_and_options(
            ok_cb.as_ref().unchecked_ref(),
            Some(err_cb.as_ref().unchecked_ref()),
            &opts,
        );
        ok_cb.forget();
        err_cb.forget();
    };

    view! {
        <div class="app-shell">
            <TopBar
                mech_filter=mech_filter
                audience=audience
                replay_active=replay_active
                terrain_3d=terrain_3d
                theme=theme
                sync_label=sync_label
                on_toggle_replay=toggle_replay
                on_gps=trigger_gps
                on_reset_view=move || {
                    selected_id.set(None);
                    js_reset_indonesia_view();
                }
                on_open_ai=move || ai_focus_tick.update(|n| *n += 1)
            />

            <main class="stage">
                <div id="map-canvas" class="map-canvas" aria-label="Peta Banjir dan Debit Sungai 3D Indonesia"></div>

                <Show when=move || user_card_msg.get().is_some()>
                    <div class="gps-toast" role="status">
                        <button
                            type="button"
                            class="gps-fly"
                            on:click=move |_| {
                                if let Some((lon, lat)) = user_coords.get() {
                                    js_fly_to_point(lon, lat, 9.8);
                                }
                            }
                        >{move || user_card_msg.get().unwrap_or_default()}</button>
                        <button
                            type="button"
                            class="gps-close"
                            aria-label="Tutup informasi lokasi"
                            on:click=move |_| {
                                user_card_msg.set(None);
                                user_coords.set(None);
                                js_clear_user_location();
                            }
                        >"×"</button>
                    </div>
                </Show>

                <ReplayBar
                    replay_active=replay_active
                    scenario_id=scenario_id
                    frame_idx=frame_idx
                    playing=playing
                    on_exit_replay=Callback::new(move |_| {
                        playing.set(false);
                        replay_active.set(false);
                        update_url_param("replay", None);
                        js_reset_indonesia_view();
                    })
                />

                <InspectorDrawer
                    snapshot=active_snapshot
                    loading=loading
                    error_msg=error_msg
                    selected_id=selected_id
                    mech_filter=mech_filter
                    audience=audience
                    active_tab=active_tab
                    drawer_open=drawer_open
                    mobile_snap=mobile_snap
                    ai_focus_tick=ai_focus_tick
                />
            </main>
        </div>
    }
}
