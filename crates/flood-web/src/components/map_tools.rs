use crate::components::ui::{Shape, ShapeGlyph};
use flood_shared::model::{FloodMechanism, IndicatorLevel};
use leptos::prelude::*;

#[component]
pub fn MapTools(
    mech_filter: RwSignal<Option<FloodMechanism>>,
    terrain_3d: RwSignal<bool>,
    theme: RwSignal<String>,
    on_gps: Callback<()>,
) -> impl IntoView {
    let options: [(Option<FloodMechanism>, &'static str); 4] = [
        (None, "Semua"),
        (Some(FloodMechanism::FluvialBasin), "Sungai"),
        (Some(FloodMechanism::UrbanPluvial), "Hujan lokal"),
        (Some(FloodMechanism::CoastalRob), "Rob"),
    ];
    view! {
        <div class="map-tools">
            <div class="seg" role="group" aria-label="Tampilkan mekanisme">
                {options.into_iter().map(|(m, lbl)| view! {
                    <button type="button" class="seg-btn" aria-pressed=move || (mech_filter.get() == m).to_string()
                        on:click=move |_| mech_filter.set(m)>{lbl}</button>
                }).collect_view()}
            </div>
            <div class="tool-rail" role="group" aria-label="Kontrol peta">
                <button type="button" class="icon-btn" aria-pressed=move || terrain_3d.get().to_string()
                    title="Relief 3D" aria-label="Relief 3D"
                    on:click=move |_| terrain_3d.update(|v| *v = !*v)>
                    <span class="txt-icon">"3D"</span>
                </button>
                <button type="button" class="icon-btn"
                    title=move || if theme.get() == "dark" { "Mode terang" } else { "Mode gelap" }
                    aria-label=move || if theme.get() == "dark" { "Mode terang" } else { "Mode gelap" }
                    on:click=move |_| theme.update(|t| *t = if t == "dark" { "light".into() } else { "dark".into() })>
                    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                        <circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.5"/>
                        <path d="M8 2.5 A5.5 5.5 0 0 1 8 13.5 Z" fill="currentColor"/>
                    </svg>
                </button>
                <button type="button" class="icon-btn" title="Lokasi saya" aria-label="Lokasi saya" on:click=move |_| on_gps.run(())>
                    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                        <circle cx="8" cy="8" r="4" fill="none" stroke="currentColor" stroke-width="1.5"/>
                        <circle cx="8" cy="8" r="1.4" fill="currentColor"/>
                        <path d="M8 1v2.5M8 12.5V15M1 8h2.5M12.5 8H15" stroke="currentColor" stroke-width="1.5"/>
                    </svg>
                </button>
            </div>
        </div>
    }
}

#[component]
pub fn Legend(open: RwSignal<bool>) -> impl IntoView {
    let levels = [
        IndicatorLevel::Normal,
        IndicatorLevel::High,
        IndicatorLevel::VeryHigh,
        IndicatorLevel::Extreme,
        IndicatorLevel::Unknown,
    ];
    view! {
        <section class="legend" class:is-open=move || open.get() aria-label="Legenda">
            <button type="button" class="legend-toggle" aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|v| *v = !*v)>"Legenda"</button>
            <div class="legend-body">
                <ul class="lg-shapes">
                    <li><ShapeGlyph shape=Shape::Circle level=IndicatorLevel::Normal/>"Sungai"</li>
                    <li><ShapeGlyph shape=Shape::Square level=IndicatorLevel::Normal/>"Hujan lokal"</li>
                    <li><ShapeGlyph shape=Shape::Diamond level=IndicatorLevel::Normal/>"Rob"</li>
                    <li><ShapeGlyph shape=Shape::Drop level=IndicatorLevel::VeryHigh/>"Laporan warga"</li>
                </ul>
                <ul class="lg-levels">
                    {levels.into_iter().map(|l| view! {
                        <li><span class=format!("sw lvl-bg-{}", l.slug())></span>{l.label_id()}</li>
                    }).collect_view()}
                </ul>
                <p class="lg-note">"Indikasi model, bukan status resmi."</p>
            </div>
        </section>
    }
}
