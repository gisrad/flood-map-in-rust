use crate::components::ui::{Disclaimer, LevelBadge, OfficialRefs};
use crate::format::{fmt_datetime_short, fmt_relative, parse_utc_ms};
use flood_shared::model::{FloodSnapshot, IndicatorLevel};
use leptos::prelude::*;

/// Salinan `docs/METODOLOGI.md` yang disajikan statis (repo GitHub belum publik).
const METHOD_DOC: &str = "/metodologi.txt";

#[component]
pub fn AboutPane(snapshot: Signal<Option<FloodSnapshot>>, now: RwSignal<f64>) -> impl IntoView {
    let feeds = move || {
        let snap = snapshot.get()?;
        let now_v = now.get();
        let u = snap.upstream.clone();
        let rows = [
            (
                "Debit sungai GloFAS",
                "tiap 6 jam",
                u.glofas_ok,
                u.glofas_fetched_at_utc,
            ),
            (
                "Hujan & lengas tanah",
                "tiap jam",
                u.weather_ok,
                u.weather_fetched_at_utc,
            ),
            (
                "Muka laut & gelombang",
                "tiap jam",
                u.marine_ok,
                u.marine_fetched_at_utc,
            ),
            (
                "Laporan warga PetaBencana",
                "tiap 15 menit",
                u.petabencana_ok,
                u.petabencana_fetched_at_utc,
            ),
            (
                "Peringatan dini BMKG",
                "tiap 15 menit",
                u.bmkg_nowcast_ok,
                u.bmkg_fetched_at_utc,
            ),
            ("Berita", "tiap 15 menit", u.news_ok, u.news_fetched_at_utc),
        ];
        let notes = u.notes.clone();
        Some(view! {
            <section class="block">
                <h3 class="h-sub">"Pembaruan per sumber"</h3>
                <div class="table-wrap">
                    <table class="data-table">
                        <thead><tr><th scope="col">"Sumber"</th><th scope="col">"Jadwal"</th><th scope="col">"Terakhir diambil"</th></tr></thead>
                        <tbody>
                            {rows.into_iter().map(|(name, sched, ok, at)| {
                                let when = at
                                    .as_deref()
                                    .and_then(parse_utc_ms)
                                    .map(|ms| format!("{} ({})", fmt_datetime_short(ms, 7), fmt_relative(ms, now_v)))
                                    .unwrap_or_else(|| "–".into());
                                view! {
                                    <tr>
                                        <th scope="row">{name}</th>
                                        <td>{sched}</td>
                                        <td>
                                            <span class=if ok { "feed-ok" } else { "feed-bad" }>{if ok { "Berhasil" } else { "Gagal" }}</span>
                                            <small class="feed-time">{when}</small>
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
                {(!notes.is_empty()).then(|| view! {
                    <ul class="fine-list">{notes.into_iter().map(|n| view! { <li>{n}</li> }).collect_view()}</ul>
                })}
                <p class="fine">{format!("Versi metodologi {} · skema data v{}.", snap.methodology_version, snap.schema_version)}</p>
            </section>
        })
    };

    view! {
        <div class="pane about">
            <Disclaimer/>

            <section class="block">
                <h3 class="h-sub">"Arti level"</h3>
                <dl class="level-defs">
                    <div><dt><LevelBadge level=IndicatorLevel::High/></dt><dd>"Sungai: debit ≥ ambang periode ulang 2 tahun (Q2). Hujan lokal: hujan 24 jam ≥ 50 mm (lebat, kategori BMKG) atau ada laporan banjir warga ≤ 10 km dalam 24 jam."</dd></div>
                    <div><dt><LevelBadge level=IndicatorLevel::VeryHigh/></dt><dd>"Sungai: ≥ Q5. Hujan lokal: ≥ 100 mm (sangat lebat)."</dd></div>
                    <div><dt><LevelBadge level=IndicatorLevel::Extreme/></dt><dd>"Sungai: ≥ Q20. Hujan lokal: > 150 mm (ekstrem)."</dd></div>
                    <div><dt><LevelBadge level=IndicatorLevel::Unknown/></dt><dd>"Ambang belum tersedia untuk sel model ini; level sungai tidak dinilai."</dd></div>
                </dl>
                <p class="fine">"Ambang sungai berasal dari reanalisis GloFAS v4 1997–2024 pada sel model yang sama (maksimum tahunan, distribusi Gumbel). Rob dibandingkan dengan klimatologi puncak muka laut model 48 jam setempat. Level ini bukan status peringatan resmi BPBD, BMKG, atau BBWS."</p>
                <p><a href=METHOD_DOC target="_blank" rel="noopener">"Metodologi lengkap"</a></p>
            </section>

            {feeds}

            <section class="block">
                <h3 class="h-sub">"Sumber data dan lisensi"</h3>
                <ul class="source-list">
                    <li><b>"GloFAS v4"</b>" (Copernicus Emergency Management Service) melalui Open-Meteo Flood API — debit simulasi, prakiraan ensemble, dan reanalisis. CC BY 4.0."</li>
                    <li><b>"Open-Meteo"</b>" Weather, Marine, dan Historical (ERA5, Copernicus Climate Change Service) — hujan, lengas tanah, muka laut, gelombang. CC BY 4.0."</li>
                    <li><b>"PetaBencana.id"</b>" — laporan banjir warga (hanya jenis banjir)."</li>
                    <li><b>"BMKG"</b>" — peringatan dini cuaca (CAP). Sumber: BMKG."</li>
                    <li><b>"Berita"</b>" — judul dan tautan ke media asli; hak cipta milik penerbit."</li>
                    <li><b>"OpenStreetMap"</b>" — alur sungai dan peta dasar (via OpenFreeMap/OpenMapTiles). © kontributor OpenStreetMap, ODbL."</li>
                    <li><b>"Terrain Tiles"</b>" (AWS Open Data) — relief untuk tampilan 3D."</li>
                    <li><b>"GEM-15"</b>" — DEM 15 m untuk lensa Riset. GEM-15: This dataset is produced by Google. CC BY 4.0."</li>
                </ul>
            </section>

            <section class="block">
                <h3 class="h-sub">"Keterbatasan"</h3>
                <ul class="fine-list">
                    <li>"Debit adalah simulasi model global pada sel 0,05° (±5 km), bukan pengukuran pos duga air. Sel dapat jatuh pada anak sungai yang berbeda; jarak sel–pos ditampilkan di lensa Riset."</li>
                    <li>"Model tidak memuat operasi bendungan, pintu air, tanggul, maupun pompa kota."</li>
                    <li>"Hujan lokal diukur pada satu titik grid; hujan konvektif dapat terlewat."</li>
                    <li>"Laporan warga belum tentu terverifikasi dan bergantung pada partisipasi."</li>
                    <li>"Replay historis direkonstruksi dari reanalisis; nilainya dapat berbeda dari pengamatan lapangan."</li>
                </ul>
            </section>

            <OfficialRefs/>
        </div>
    }
}
