use crate::components::ui::{Shape, ShapeGlyph};
use flood_shared::model::{FloodMechanism, IndicatorLevel};
use leptos::prelude::*;

#[component]
pub fn MapTools(
    mech_filter: RwSignal<Option<FloodMechanism>>,
    terrain_3d: RwSignal<bool>,
    inundation: RwSignal<bool>,
    theme: RwSignal<String>,
    /// `true` setelah pengunjung memilih tema sendiri; sebelum itu tema mengikuti sistem.
    theme_explicit: RwSignal<bool>,
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
                <button type="button" class="icon-btn" aria-pressed=move || inundation.get().to_string()
                    title="Riwayat genangan 1999–2020 (Google)" aria-label="Riwayat genangan 1999–2020 (Google)"
                    on:click=move |_| inundation.update(|v| *v = !*v)>
                    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                        <path d="M1 5.5c2-2 3-2 5 0s3 2 5 0 3-2 5 0M1 9.5c2-2 3-2 5 0s3 2 5 0 3-2 5 0M1 13.5c2-2 3-2 5 0s3 2 5 0 3-2 5 0"
                            fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                    </svg>
                </button>
                <button type="button" class="icon-btn"
                    title=move || if theme.get() == "dark" { "Mode terang" } else { "Mode gelap" }
                    aria-label=move || if theme.get() == "dark" { "Mode terang" } else { "Mode gelap" }
                    on:click=move |_| {
                        theme_explicit.set(true);
                        theme.update(|t| *t = if t == "dark" { "light".into() } else { "dark".into() });
                    }>
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
pub fn Legend(open: RwSignal<bool>, inundation: RwSignal<bool>) -> impl IntoView {
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
                {move || inundation.get().then(|| view! {
                    <div class="lg-inund">
                        <p class="lg-title">"Riwayat genangan 1999–2020"</p>
                        <ul class="lg-levels">
                            <li><span class="sw inund-sw inund-high"></span>"Sering basah (≥ 5% waktu)"</li>
                            <li><span class="sw inund-sw inund-medium"></span>"Kadang basah (≥ 1%)"</li>
                            <li><span class="sw inund-sw inund-low"></span>"Jarang basah (≥ 0,5%)"</li>
                        </ul>
                        <p class="lg-note">"Citra satelit, bukan prakiraan dan bukan peta rawan banjir resmi. © Google Research (CC BY 4.0), GLAD."</p>
                    </div>
                })}
                <p class="lg-note">"Indikasi model, bukan status resmi."</p>
            </div>
        </section>
    }
}
