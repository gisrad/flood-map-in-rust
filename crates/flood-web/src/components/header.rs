use crate::format::{fmt_relative, parse_utc_ms};
use crate::status::{status_body, status_stamp};
use flood_shared::model::FloodSnapshot;
use leptos::prelude::*;

#[component]
pub fn Header(
    snapshot: Signal<Option<FloodSnapshot>>,
    loading: RwSignal<bool>,
    now: RwSignal<f64>,
    menu_open: RwSignal<bool>,
    on_home: Callback<()>,
) -> impl IntoView {
    let sentence = move || match snapshot.get() {
        Some(s) if s.is_historical_replay => view! {
            <span class="status-stamp is-replay">"Replay historis"</span>
            <span class="status-body">{s.replay_label.clone().unwrap_or_default()}</span>
        }
        .into_any(),
        Some(s) => view! {
            <span class="status-stamp">{status_stamp(&s)}</span>
            <span class="status-body">{status_body(&s)}</span>
        }
        .into_any(),
        None if loading.get() => {
            view! { <span class="status-body muted">"Memuat data…"</span> }.into_any()
        }
        None => view! { <span class="status-body muted">"Data belum tersedia"</span> }.into_any(),
    };
    let updated = move || {
        let s = snapshot.get()?;
        if s.is_historical_replay {
            return None;
        }
        let ms = parse_utc_ms(&s.generated_at_utc)?;
        Some(fmt_relative(ms, now.get()))
    };

    view! {
        <header class="topbar">
            <button type="button" class="wordmark" on:click=move |_| on_home.run(()) title="Tampilkan seluruh Indonesia">
                <span class="wm-mark" aria-hidden="true"></span>
                <span class="wm-text">"Hidrosentinel"<span class="wm-sub">" Nusantara"</span></span>
            </button>
            <p class="status" aria-live="polite">{sentence}</p>
            {move || updated().map(|u| view! { <span class="updated" title="Waktu pembaruan data">{u}</span> })}
            <button
                type="button"
                class="menu-btn"
                aria-haspopup="true"
                aria-expanded=move || menu_open.get().to_string()
                aria-controls="app-menu"
                on:click=move |_| menu_open.update(|v| *v = !*v)
            >
                <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                    <path d="M2 4h12M2 8h12M2 12h12" stroke="currentColor" stroke-width="1.6" fill="none"/>
                </svg>
                <span class="menu-lbl">"Menu"</span>
            </button>
        </header>
    }
}

#[component]
pub fn AppMenu(
    menu_open: RwSignal<bool>,
    replay_active: RwSignal<bool>,
    replay_available: bool,
    on_about: Callback<()>,
    on_replay: Callback<()>,
    on_copy_link: Callback<()>,
    on_open_data: Callback<()>,
) -> impl IntoView {
    let close = move || menu_open.set(false);
    view! {
        <Show when=move || menu_open.get()>
            <div class="menu-scrim" on:click=move |_| close()></div>
            <nav id="app-menu" class="app-menu" aria-label="Menu"
                on:keydown=move |ev: web_sys::KeyboardEvent| if ev.key() == "Escape" { close() }>
                <button type="button" class="menu-item" autofocus on:click=move |_| { close(); on_about.run(()); }>
                    <b>"Tentang data & metodologi"</b><small>"Sumber, lisensi, arti level, keterbatasan"</small>
                </button>
                <button type="button" class="menu-item" disabled=!replay_available on:click=move |_| { close(); on_replay.run(()); }>
                    <b>{move || if replay_active.get() { "Kembali ke data terkini" } else { "Replay kejadian historis" }}</b>
                    <small>{if replay_available { "Rekonstruksi dari reanalisis" } else { "Belum tersedia" }}</small>
                </button>
                <button type="button" class="menu-item" on:click=move |_| { close(); on_copy_link.run(()); }>
                    <b>"Salin tautan tampilan ini"</b><small>"Termasuk titik, lensa, dan tema"</small>
                </button>
                <button type="button" class="menu-item" on:click=move |_| { close(); on_open_data.run(()); }>
                    <b>"Data terbuka & API"</b><small>"JSON, GeoJSON, OpenAPI, llms.txt"</small>
                </button>
                <a class="menu-item" href="https://github.com/gisrad/flood-map-in-rust" target="_blank" rel="noopener">
                    <b>"Kode sumber"</b><small>"GitHub"</small>
                </a>
            </nav>
        </Show>
    }
}
