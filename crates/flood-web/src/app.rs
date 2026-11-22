use crate::bridge::{
    build_map_geojsons, copy_text, current_url, js_clear_user_location, js_fly_to_point,
    js_init_map, js_reset_indonesia_view, js_set_map_theme, js_set_user_location,
    js_toggle_3d_terrain, js_update_map_data, prefers_dark, read_url_param, remove_splash,
    set_html_theme, update_url_param,
};
use crate::components::header::{AppMenu, Header};
use crate::components::inspector::{
    lens_from_slug, lens_slug, InspectorDrawer, InspectorTab, SheetSnap,
};
use crate::components::map_tools::{Legend, MapTools};
use crate::components::replay_bar::ReplayBar;
use crate::components::station_detail::{Gem15Cell, Gem15Ctx};
use crate::components::ui::Toast;
use crate::format::{fmt_num, now_ms};
use flood_shared::hydrology::haversine_km;
use flood_shared::model::{Audience, FloodMechanism, FloodSnapshot};
use flood_shared::replay::{build_replay_snapshot, list_replay_scenarios};
use gloo_net::http::Request;
use gloo_timers::callback::{Interval, Timeout};
use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

const REFRESH_MS: u32 = 5 * 60 * 1000;

fn mech_from_slug(s: &str) -> Option<FloodMechanism> {
    match s {
        "sungai" => Some(FloodMechanism::FluvialBasin),
        "hujan" => Some(FloodMechanism::UrbanPluvial),
        "rob" => Some(FloodMechanism::CoastalRob),
        _ => None,
    }
}

fn mech_slug(m: FloodMechanism) -> &'static str {
    match m {
        FloodMechanism::FluvialBasin => "sungai",
        FloodMechanism::UrbanPluvial => "hujan",
        FloodMechanism::CoastalRob => "rob",
    }
}

fn point_coords(snap: &FloodSnapshot, id: &str) -> Option<(f64, f64, f64)> {
    snap.stations
        .iter()
        .find(|s| s.id == id)
        .map(|s| (s.lon, s.lat, 9.6))
        .or_else(|| {
            snap.coastal_rob
                .iter()
                .find(|r| r.id == id)
                .map(|r| (r.lon, r.lat, 10.0))
        })
}

async fn fetch_snapshot() -> Result<FloodSnapshot, String> {
    let resp = Request::get("/api/flood")
        .send()
        .await
        .map_err(|_| "Server tidak dapat dihubungi. Periksa koneksi internet.".to_string())?;
    if !resp.ok() {
        return Err(format!("Server mengembalikan status {}.", resp.status()));
    }
    resp.json::<FloodSnapshot>()
        .await
        .map_err(|_| "Format data dari server tidak dikenali.".to_string())
}

#[component]
pub fn App() -> impl IntoView {
    let initial_theme = read_url_param("theme")
        .filter(|t| t == "dark" || t == "light")
        .unwrap_or_else(|| {
            if prefers_dark() {
                "dark".into()
            } else {
                "light".into()
            }
        });
    let initial_lens = read_url_param("lens")
        .map(|s| lens_from_slug(&s))
        .unwrap_or(Audience::Public);
    let initial_mech = read_url_param("m").and_then(|s| mech_from_slug(&s));
    let initial_sel = read_url_param("s").or_else(|| read_url_param("basin"));
    let scenarios = StoredValue::new(list_replay_scenarios());
    let initial_replay = read_url_param("replay")
        .filter(|id| scenarios.with_value(|all| all.iter().any(|s| &s.id == id)));

    let toast = Toast(RwSignal::new(None));
    provide_context(toast);
    let gem15 = Gem15Ctx(RwSignal::new(HashMap::new()));
    provide_context(gem15);

    let live_snapshot = RwSignal::new(Option::<FloodSnapshot>::None);
    let loading = RwSignal::new(true);
    let error_msg = RwSignal::new(Option::<String>::None);
    let now = RwSignal::new(now_ms());

    let selected_id = RwSignal::new(initial_sel);
    let mech_filter = RwSignal::new(initial_mech);
    let lens = RwSignal::new(initial_lens);
    let theme = RwSignal::new(initial_theme);
    let terrain_3d = RwSignal::new(false);

    let replay_active = RwSignal::new(initial_replay.is_some());
    let scenario_id = RwSignal::new(
        initial_replay
            .clone()
            .or_else(|| scenarios.with_value(|all| all.first().map(|s| s.id.clone())))
            .unwrap_or_default(),
    );
    let frame_idx = RwSignal::new(0usize);
    let playing = RwSignal::new(false);

    let active_tab = RwSignal::new(InspectorTab::Summary);
    let drawer_open = RwSignal::new(true);
    let mobile_snap = RwSignal::new(SheetSnap::Peek);
    let menu_open = RwSignal::new(false);
    let legend_open = RwSignal::new(false);

    let user_msg = RwSignal::new(Option::<String>::None);
    let user_coords = RwSignal::new(Option::<(f64, f64)>::None);

    // Data langsung + pembaruan berkala.
    let load = move || {
        spawn_local(async move {
            match fetch_snapshot().await {
                Ok(snap) => {
                    live_snapshot.set(Some(snap));
                    error_msg.set(None);
                }
                Err(e) => {
                    if live_snapshot.get_untracked().is_none() {
                        error_msg.set(Some(e));
                    }
                }
            }
            loading.set(false);
            remove_splash();
        });
    };
    load();
    Interval::new(REFRESH_MS, load).forget();
    Interval::new(30_000, move || now.set(now_ms())).forget();
    Timeout::new(8000, remove_splash).forget();

    // Sel DEM GEM-15 untuk lensa Riset (opsional, berkas milik worker aset).
    spawn_local(async move {
        if let Ok(resp) = Request::get("/data/gem15-cells.json").send().await {
            if resp.ok() {
                if let Ok(m) = resp.json::<HashMap<String, Option<Gem15Cell>>>().await {
                    gem15.0.set(m);
                }
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

    let select_point = Callback::new(move |(id, lon, lat): (String, f64, f64)| {
        selected_id.set(Some(id));
        drawer_open.set(true);
        if mobile_snap.get_untracked() == SheetSnap::Peek {
            mobile_snap.set(SheetSnap::Half);
        }
        js_fly_to_point(lon, lat, 9.6);
    });

    // Peta
    Effect::new(move |_| {
        set_html_theme(&theme.get_untracked());
        let cb = Closure::wrap(Box::new(move |id: String| {
            if let Some((lon, lat, _)) = active_snapshot
                .get_untracked()
                .and_then(|s| point_coords(&s, &id))
            {
                select_point.run((id, lon, lat));
            }
        }) as Box<dyn FnMut(String)>);
        js_init_map("map-canvas", &theme.get_untracked(), &cb);
        cb.forget();
    });

    // Terbang ke titik dari permalink sekali, setelah data pertama tiba.
    let flew_initial = StoredValue::new(false);
    Effect::new(move |_| {
        let Some(snap) = active_snapshot.get() else {
            return;
        };
        if flew_initial.get_value() {
            return;
        }
        flew_initial.set_value(true);
        if let Some(id) = selected_id.get_untracked() {
            if let Some((lon, lat, z)) = point_coords(&snap, &id) {
                mobile_snap.set(SheetSnap::Half);
                js_fly_to_point(lon, lat, z);
            }
        } else if replay_active.get_untracked() {
            let id = scenario_id.get_untracked();
            if let Some(sc) = scenarios.with_value(|all| all.iter().find(|s| s.id == id).cloned()) {
                js_fly_to_point(sc.center_lon, sc.center_lat, sc.zoom);
            }
        }
    });

    Effect::new(move |_| {
        let th = theme.get();
        set_html_theme(&th);
        js_set_map_theme(&th);
        update_url_param("theme", (th == "dark").then_some("dark"));
    });
    Effect::new(move |_| js_toggle_3d_terrain(terrain_3d.get()));
    Effect::new(move |_| update_url_param("s", selected_id.get().as_deref()));
    Effect::new(move |_| update_url_param("m", mech_filter.get().map(mech_slug)));
    Effect::new(move |_| {
        let l = lens.get();
        update_url_param("lens", (l != Audience::Public).then(|| lens_slug(l)));
    });
    Effect::new(move |_| {
        let id = scenario_id.get();
        update_url_param("replay", replay_active.get().then_some(id.as_str()));
    });

    Effect::new(move |_| {
        if let Some(snap) = active_snapshot.get() {
            let (stations, robs, reports) = build_map_geojsons(&snap, mech_filter.get());
            js_update_map_data(
                &stations,
                &robs,
                &reports,
                &selected_id.get().unwrap_or_default(),
            );
        }
    });

    // Putar replay
    let interval_handle = StoredValue::new_local(Option::<Interval>::None);
    Effect::new(move |_| {
        let is_play = playing.get() && replay_active.get();
        interval_handle.set_value(None);
        if is_play {
            let h = Interval::new(1800, move || {
                let id = scenario_id.get_untracked();
                let count = scenarios
                    .with_value(|all| all.iter().find(|s| s.id == id).map(|s| s.frames.len()))
                    .unwrap_or(1)
                    .max(1);
                frame_idx.update(|i| *i = (*i + 1) % count);
            });
            interval_handle.set_value(Some(h));
        }
    });

    let enter_scenario = move |id: String| {
        playing.set(false);
        frame_idx.set(0);
        selected_id.set(None);
        if let Some(sc) = scenarios.with_value(|all| all.iter().find(|s| s.id == id).cloned()) {
            js_fly_to_point(sc.center_lon, sc.center_lat, sc.zoom);
        }
        scenario_id.set(id);
        replay_active.set(true);
    };
    let exit_replay = Callback::new(move |_| {
        playing.set(false);
        replay_active.set(false);
        selected_id.set(None);
        js_reset_indonesia_view();
    });
    let replay_available = scenarios.with_value(|all| !all.is_empty());
    let toggle_replay = Callback::new(move |_| {
        if replay_active.get_untracked() {
            exit_replay.run(());
        } else if replay_available {
            enter_scenario(scenario_id.get_untracked());
        }
    });

    let go_home = Callback::new(move |_| {
        selected_id.set(None);
        menu_open.set(false);
        js_reset_indonesia_view();
    });

    let trigger_gps = Callback::new(move |_| {
        let Some(win) = web_sys::window() else { return };
        let Ok(geo) = win.navigator().geolocation() else {
            user_msg.set(Some("Peramban tidak mendukung lokasi.".into()));
            return;
        };
        let ok_cb = Closure::once(move |pos: web_sys::Position| {
            let c = pos.coords();
            let (lat, lon) = (c.latitude(), c.longitude());
            user_coords.set(Some((lon, lat)));
            js_set_user_location(lon, lat);
            js_fly_to_point(lon, lat, 9.5);
            let nearest = active_snapshot.get_untracked().and_then(|s| {
                s.stations
                    .iter()
                    .map(|st| {
                        (
                            haversine_km(lat, lon, st.lat, st.lon),
                            st.name.clone(),
                            st.level.label_id(),
                        )
                    })
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            });
            user_msg.set(Some(match nearest {
                Some((km, name, lvl)) => format!(
                    "Titik pantau terdekat: {name}, ±{} km · {lvl}",
                    fmt_num(km, 0)
                ),
                None => "Lokasi ditemukan.".into(),
            }));
        });
        let err_cb = Closure::once(move |_e: wasm_bindgen::JsValue| {
            user_msg.set(Some("Akses lokasi ditolak atau tidak tersedia.".into()));
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
    });

    // Escape: tutup menu, lalu batalkan pilihan titik.
    let esc =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            if ev.key() != "Escape" {
                return;
            }
            if menu_open.get_untracked() {
                menu_open.set(false);
            } else if selected_id.get_untracked().is_some() {
                selected_id.set(None);
            }
        });
    if let Some(win) = web_sys::window() {
        let _ = win.add_event_listener_with_callback("keydown", esc.as_ref().unchecked_ref());
    }
    esc.forget();

    view! {
        <div class="app-shell" class:is-replay=move || replay_active.get()>
            <a class="skip-link" href="#panel">"Lewati ke panel informasi"</a>
            <Header snapshot=active_snapshot loading=loading now=now menu_open=menu_open on_home=go_home/>
            <AppMenu
                menu_open=menu_open
                replay_active=replay_active
                replay_available=replay_available
                on_about=Callback::new(move |_| {
                    selected_id.set(None);
                    active_tab.set(InspectorTab::About);
                    drawer_open.set(true);
                    mobile_snap.set(SheetSnap::Full);
                })
                on_replay=toggle_replay
                on_copy_link=Callback::new(move |_| {
                    copy_text(&current_url());
                    toast.show("Tautan disalin");
                })
                on_open_data=Callback::new(move |_| {
                    selected_id.set(None);
                    lens.set(Audience::AiAgent);
                    active_tab.set(InspectorTab::Summary);
                    drawer_open.set(true);
                    mobile_snap.set(SheetSnap::Full);
                })
            />

            <main class="stage">
                <div id="map-canvas" class="map-canvas" role="region" aria-label="Peta indikasi banjir Indonesia"></div>
                <MapTools mech_filter=mech_filter terrain_3d=terrain_3d theme=theme on_gps=trigger_gps/>
                <Legend open=legend_open/>

                {move || user_msg.get().map(|m| view! {
                    <div class="gps-toast" role="status">
                        <button type="button" class="gps-msg" on:click=move |_| {
                            if let Some((lon, lat)) = user_coords.get_untracked() { js_fly_to_point(lon, lat, 10.0); }
                        }>{m}</button>
                        <button type="button" class="icon-btn" aria-label="Tutup" on:click=move |_| {
                            user_msg.set(None);
                            user_coords.set(None);
                            js_clear_user_location();
                        }>"×"</button>
                    </div>
                })}

                <ReplayBar
                    scenarios=scenarios
                    snapshot=active_snapshot
                    replay_active=replay_active
                    scenario_id=scenario_id
                    frame_idx=frame_idx
                    playing=playing
                    on_pick=Callback::new(enter_scenario)
                    on_exit=exit_replay
                />

                <InspectorDrawer
                    snapshot=active_snapshot
                    loading=loading
                    error_msg=error_msg
                    selected_id=selected_id
                    mech_filter=mech_filter
                    lens=lens
                    active_tab=active_tab
                    drawer_open=drawer_open
                    mobile_snap=mobile_snap
                    now=now
                    on_select=select_point
                />

                {move || toast.0.get().map(|m| view! { <div class="toast" role="status">{m}</div> })}
            </main>
        </div>
    }
}
