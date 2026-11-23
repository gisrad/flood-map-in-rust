use flood_shared::model::{FloodMechanism, IndicatorLevel};
use leptos::prelude::*;

/// Pesan singkat (mis. "Tautan disalin") yang tampil beberapa detik.
#[derive(Clone, Copy)]
pub struct Toast(pub RwSignal<Option<String>>);

impl Toast {
    pub fn show(self, msg: &str) {
        let sig = self.0;
        sig.set(Some(msg.to_string()));
        gloo_timers::callback::Timeout::new(2600, move || sig.set(None)).forget();
    }
}

pub fn use_toast() -> Toast {
    use_context::<Toast>().expect("Toast context")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Circle,
    Square,
    Diamond,
    Drop,
}

impl Shape {
    pub fn for_mechanism(m: FloodMechanism) -> Self {
        match m {
            FloodMechanism::FluvialBasin => Self::Circle,
            FloodMechanism::UrbanPluvial => Self::Square,
            FloodMechanism::CoastalRob => Self::Diamond,
        }
    }
}

/// Glif bentuk mekanisme berwarna level (sama dengan ikon peta).
#[component]
pub fn ShapeGlyph(shape: Shape, level: IndicatorLevel) -> impl IntoView {
    let cls = format!("glyph lvl-fill-{}", level.slug());
    let path = match shape {
        Shape::Circle => view! { <circle cx="8" cy="8" r="6"/> }.into_any(),
        Shape::Square => view! { <rect x="2.5" y="2.5" width="11" height="11" rx="3.2"/> }.into_any(),
        Shape::Diamond => view! { <path d="M8 1.2 L14.8 8 L8 14.8 L1.2 8 Z"/> }.into_any(),
        Shape::Drop => {
            view! { <path d="M8 1.5 C9.5 4.5 12.5 7 12.5 10 A4.5 4.5 0 0 1 3.5 10 C3.5 7 6.5 4.5 8 1.5 Z"/> }
                .into_any()
        }
    };
    view! {
        <svg class=cls viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">{path}</svg>
    }
}

#[component]
pub fn LevelBadge(level: IndicatorLevel) -> impl IntoView {
    view! { <span class=format!("lvl-badge lvl-{}", level.slug())>{level.label_id()}</span> }
}

/// Pernyataan bahwa level adalah indikasi model.
#[component]
pub fn Disclaimer() -> impl IntoView {
    view! {
        <p class="disclaimer">
            <strong>"Indikasi model, bukan peringatan resmi."</strong>
            " Keputusan evakuasi dan status siaga ditetapkan BPBD setempat. Ikuti informasi BMKG, BNPB/BPBD, dan BBWS."
        </p>
    }
}

/// Rujukan resmi dan nomor darurat (terverifikasi 22 Nov 2026 pada situs resmi masing-masing).
#[component]
pub fn OfficialRefs() -> impl IntoView {
    view! {
        <div class="refs">
            <h3 class="h-sub">"Rujukan resmi"</h3>
            <ul class="ref-list">
                <li><a href="https://www.bnpb.go.id/bpbd-propinsi" target="_blank" rel="noopener">"Daftar BPBD provinsi"</a></li>
                <li><a href="https://www.bmkg.go.id/peringatan-dini" target="_blank" rel="noopener">"Peringatan dini cuaca BMKG"</a></li>
                <li><a href="https://sda.pu.go.id/" target="_blank" rel="noopener">"Ditjen Sumber Daya Air (BBWS/BWS)"</a></li>
                <li><a href="https://petabencana.id/" target="_blank" rel="noopener">"Laporkan banjir: PetaBencana.id"</a></li>
            </ul>
            <ul class="tel-list">
                <li><a href="tel:112"><b>"112"</b></a><span>"Panggilan darurat"</span></li>
                <li><a href="tel:117"><b>"117"</b></a><span>"Pusdalops BNPB"</span></li>
                <li><a href="tel:115"><b>"115"</b></a><span>"Basarnas (SAR)"</span></li>
                <li><a href="tel:196"><b>"196"</b></a><span>"Contact Center BMKG"</span></li>
            </ul>
        </div>
    }
}
