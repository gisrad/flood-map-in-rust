use flood_shared::model::{Audience, FloodMechanism};
use leptos::prelude::*;

#[component]
pub fn TopBar(
    mech_filter: RwSignal<Option<FloodMechanism>>,
    audience: RwSignal<Audience>,
    replay_active: RwSignal<bool>,
    terrain_3d: RwSignal<bool>,
    theme: RwSignal<String>,
    sync_label: Signal<String>,
    on_toggle_replay: impl Fn() + 'static + Clone,
    on_gps: impl Fn() + 'static + Clone,
    on_reset_view: impl Fn() + 'static + Clone,
    on_open_ai: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let toggle_theme = move |_| {
        theme.update(|t| {
            *t = if t == "light" {
                "dark".to_string()
            } else {
                "light".to_string()
            };
        });
    };

    view! {
        <header class="top-bar" role="banner">
            <div class="brand-group">
                <button type="button" class="brand-btn" on:click=move |_| on_reset_view() title="Reset tampilan Nusantara">
                    <span class="brand-mark" aria-hidden="true"></span>
                    <span class="brand-text">
                        <strong>"Hidrosentinel"</strong>
                        <small>"Nusantara · Rust/WASM"</small>
                    </span>
                </button>
                <span class="sync-pill" class:is-replay=move || replay_active.get()>
                    {move || sync_label.get()}
                </span>
            </div>

            <nav class="mech-scroll" aria-label="Filter mekanisme banjir">
                <button
                    type="button"
                    class="chip"
                    aria-pressed=move || mech_filter.get().is_none().to_string()
                    on:click=move |_| mech_filter.set(None)
                >"Semua"</button>
                <button
                    type="button"
                    class="chip"
                    aria-pressed=move || (mech_filter.get() == Some(FloodMechanism::FluvialBasin)).to_string()
                    on:click=move |_| mech_filter.set(Some(FloodMechanism::FluvialBasin))
                >
                    <span class="dot-ind dot-fluvial"></span>"Hulu DAS"
                </button>
                <button
                    type="button"
                    class="chip"
                    aria-pressed=move || (mech_filter.get() == Some(FloodMechanism::UrbanPluvial)).to_string()
                    on:click=move |_| mech_filter.set(Some(FloodMechanism::UrbanPluvial))
                >
                    <span class="dot-ind dot-pluvial"></span>"Drainase"
                </button>
                <button
                    type="button"
                    class="chip"
                    aria-pressed=move || (mech_filter.get() == Some(FloodMechanism::CoastalRob)).to_string()
                    on:click=move |_| mech_filter.set(Some(FloodMechanism::CoastalRob))
                >
                    <span class="dot-ind dot-rob"></span>"Rob Pesisir"
                </button>
            </nav>

            <div class="tool-group">
                <div class="aud-switch" role="group" aria-label="Perspektif audiens">
                    <button
                        type="button"
                        class="aud-btn"
                        aria-pressed=move || (audience.get() == Audience::Public).to_string()
                        on:click=move |_| audience.set(Audience::Public)
                    >"Publik"</button>
                    <button
                        type="button"
                        class="aud-btn"
                        aria-pressed=move || (audience.get() == Audience::Gov).to_string()
                        on:click=move |_| audience.set(Audience::Gov)
                    >"BPBD"</button>
                    <button
                        type="button"
                        class="aud-btn"
                        aria-pressed=move || (audience.get() == Audience::Academia).to_string()
                        on:click=move |_| audience.set(Audience::Academia)
                    >"Riset"</button>
                    <button
                        type="button"
                        class="aud-btn"
                        aria-pressed=move || (audience.get() == Audience::AiAgent).to_string()
                        on:click=move |_| audience.set(Audience::AiAgent)
                    >"AI"</button>
                </div>

                <button
                    type="button"
                    class="tool-btn ai-trigger"
                    on:click=move |_| on_open_ai()
                    title="Tanya Data / AI Command (Ctrl+K)"
                >"AI Tanya"</button>

                <button
                    type="button"
                    class="tool-btn"
                    aria-pressed=move || replay_active.get().to_string()
                    on:click=move |_| on_toggle_replay()
                    title="Simulasi Kejadian Banjir Historis"
                >"Replay"</button>

                <button
                    type="button"
                    class="tool-btn"
                    on:click=move |_| on_gps()
                    title="Deteksi lokasi saya (GPS / Wi-Fi)"
                >"Lokasi"</button>

                <button
                    type="button"
                    class="tool-btn"
                    aria-pressed=move || terrain_3d.get().to_string()
                    on:click=move |_| terrain_3d.update(|v| *v = !*v)
                    title="Toggle medan 3D AWS Terrarium"
                >"3D"</button>

                <button
                    type="button"
                    class="tool-btn"
                    on:click=toggle_theme
                    title="Ganti Mode Siang / Malam Pusdalops"
                >
                    {move || if theme.get() == "light" { "Malam" } else { "Siang" }}
                </button>
            </div>
        </header>
    }
}
