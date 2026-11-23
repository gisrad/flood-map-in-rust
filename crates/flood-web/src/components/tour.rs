//! Tur singkat fitur inti untuk pengunjung pertama: 6 langkah, bisa dilewati kapan saja, dan
//! dapat dibuka lagi dari Menu. Tiap langkah menyorot satu bagian antarmuka.

use leptos::html::Button;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

struct Step {
    /// Selektor CSS bagian yang disorot; `None` = kartu di tengah layar.
    target: Option<&'static str>,
    title: &'static str,
    body: &'static str,
}

const STEPS: &[Step] = &[
    Step {
        target: None,
        title: "Selamat datang di Hidrosentinel Nusantara",
        body: "Peta indikasi banjir sungai, hujan lebat, dan rob untuk seluruh Indonesia, diperbarui tiap 15 menit. Tur ini 6 langkah, sekitar 30 detik.",
    },
    Step {
        target: Some(".legend"),
        title: "Warna menunjukkan level, bentuk menunjukkan jenis",
        body: "Abu-abu berarti normal; kuning, merah, dan ungu makin tinggi. Lingkaran untuk sungai, kotak untuk hujan lokal, belah ketupat untuk rob. Ketuk titik untuk rinciannya; Legenda selalu ada di sini.",
    },
    Step {
        target: Some(".seg"),
        title: "Fokus pada satu jenis banjir",
        body: "Sungai untuk banjir kiriman dari hulu, Hujan lokal untuk genangan, Rob untuk pesisir.",
    },
    Step {
        target: Some(".inspector"),
        title: "Ringkasan sesuai kebutuhan Anda",
        body: "Panel ini memuat titik yang perlu diperhatikan. Pilih lensa Publik, BPBD/Pemda, Riset, atau AI; tab Notifikasi berisi langganan gratis lewat ntfy atau RSS.",
    },
    Step {
        target: Some(".tool-rail"),
        title: "Alat peta",
        body: "Relief 3D, riwayat genangan 1999–2020, mode gelap atau terang, dan titik pantau terdekat dari lokasi Anda.",
    },
    Step {
        target: Some(".menu-btn"),
        title: "Replay, data terbuka, dan tur ini",
        body: "Di Menu: putar ulang banjir besar masa lalu, unduh data dan API, atau buka tur ini lagi. Ingat, ini indikasi model, bukan peringatan resmi; ikuti BPBD dan BMKG setempat.",
    },
];

const STORE_KEY: &str = "hidrosentinel-tour";
const STORE_DONE: &str = "v1";
const GAP: f64 = 12.0;
const SPOT_PAD: f64 = 6.0;
/// Perkiraan tinggi kartu untuk memilih sisi penempatan.
const CARD_H: f64 = 230.0;

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// `true` bila pengunjung sudah menyelesaikan atau melewati tur di peramban ini.
pub fn tour_seen() -> bool {
    storage()
        .and_then(|s| s.get_item(STORE_KEY).ok().flatten())
        .is_some_and(|v| v == STORE_DONE)
}

fn mark_seen() {
    if let Some(s) = storage() {
        let _ = s.set_item(STORE_KEY, STORE_DONE);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Below,
    Above,
    Side,
    Center,
}

#[derive(Clone, Copy, PartialEq)]
struct Layout {
    /// Kotak sorotan (x, y, w, h) dalam piksel viewport.
    spot: Option<(f64, f64, f64, f64)>,
    top: f64,
    left: f64,
    width: f64,
    place: Place,
}

fn layout_for(step: &Step) -> Layout {
    let win = web_sys::window();
    let vw = win
        .as_ref()
        .and_then(|w| w.inner_width().ok()?.as_f64())
        .unwrap_or(1024.0);
    let vh = win
        .as_ref()
        .and_then(|w| w.inner_height().ok()?.as_f64())
        .unwrap_or(768.0);
    let width = (vw - 2.0 * GAP).min(360.0);
    let center = Layout {
        spot: None,
        top: vh / 2.0,
        left: (vw - width) / 2.0,
        width,
        place: Place::Center,
    };
    let rect = step.target.and_then(|sel| {
        let el = web_sys::window()?.document()?.query_selector(sel).ok()??;
        let r = el.get_bounding_client_rect();
        (r.width() > 0.0 && r.height() > 0.0).then_some(r)
    });
    let Some(r) = rect else {
        return center;
    };
    let (x, y) = (r.left() - SPOT_PAD, r.top() - SPOT_PAD);
    let (w, h) = (r.width() + 2.0 * SPOT_PAD, r.height() + 2.0 * SPOT_PAD);
    let clamp_x = |left: f64| left.clamp(GAP, (vw - width - GAP).max(GAP));
    let below = vh - (y + h) - GAP;
    let (top, left, place) = if below >= CARD_H {
        (
            y + h + GAP,
            clamp_x(x + w / 2.0 - width / 2.0),
            Place::Below,
        )
    } else if y - GAP >= CARD_H {
        (y - GAP, clamp_x(x + w / 2.0 - width / 2.0), Place::Above)
    } else if x - GAP >= width + GAP {
        // Sasaran tinggi (panel samping desktop): kartu di kirinya.
        (
            (y + h / 2.0).clamp(CARD_H / 2.0, vh - CARD_H / 2.0),
            x - GAP - width,
            Place::Side,
        )
    } else if vw - (x + w) - GAP >= width + GAP {
        (
            (y + h / 2.0).clamp(CARD_H / 2.0, vh - CARD_H / 2.0),
            x + w + GAP,
            Place::Side,
        )
    } else {
        (vh / 2.0, (vw - width) / 2.0, Place::Center)
    };
    Layout {
        spot: Some((x, y, w, h)),
        top,
        left,
        width,
        place,
    }
}

#[component]
pub fn Tour(open: RwSignal<bool>) -> impl IntoView {
    let step = RwSignal::new(0usize);
    // Dinaikkan saat ukuran jendela berubah agar sorotan dihitung ulang.
    let tick = RwSignal::new(0u32);
    let resize = Closure::<dyn FnMut()>::new(move || tick.update(|t| *t = t.wrapping_add(1)));
    if let Some(w) = web_sys::window() {
        let _ = w.add_event_listener_with_callback("resize", resize.as_ref().unchecked_ref());
    }
    resize.forget();

    let layout = Memo::new(move |_| {
        tick.track();
        open.get()
            .then(|| layout_for(&STEPS[step.get().min(STEPS.len() - 1)]))
    });

    let next_ref = NodeRef::<Button>::new();
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        step.track();
        // Fokus ke tombol utama tiap langkah; hitung ulang setelah panel selesai beranimasi.
        if let Some(b) = next_ref.get() {
            let _ = b.focus();
        }
        gloo_timers::callback::Timeout::new(260, move || tick.update(|t| *t = t.wrapping_add(1)))
            .forget();
    });

    let finish = move || {
        mark_seen();
        open.set(false);
        step.set(0);
    };
    let next = move || {
        if step.get_untracked() + 1 >= STEPS.len() {
            finish();
        } else {
            step.update(|s| *s += 1);
        }
    };
    let back = move || step.update(|s| *s = s.saturating_sub(1));
    let not_first = Memo::new(move |_| step.get() > 0);
    let not_last = Memo::new(move |_| step.get() + 1 < STEPS.len());

    view! {
        <Show when=move || open.get()>
            <div class="tour-scrim" class:is-dim=move || layout.get().is_some_and(|l| l.spot.is_none())></div>
            {move || layout.get().and_then(|l| l.spot).map(|(x, y, w, h)| view! {
                <div class="tour-spot" aria-hidden="true"
                    style=format!("left:{x:.0}px;top:{y:.0}px;width:{w:.0}px;height:{h:.0}px")></div>
            })}
            <div class="tour-card" role="dialog" aria-modal="true"
                aria-labelledby="tour-title" aria-describedby="tour-body"
                class:is-above=move || layout.get().is_some_and(|l| l.place == Place::Above)
                class:is-mid=move || layout.get().is_some_and(|l| matches!(l.place, Place::Side | Place::Center))
                style=move || layout.get().map(|l| format!("top:{:.0}px;left:{:.0}px;width:{:.0}px", l.top, l.left, l.width)).unwrap_or_default()
                on:keydown=move |ev: web_sys::KeyboardEvent| match ev.key().as_str() {
                    "Escape" => finish(),
                    "ArrowRight" => next(),
                    "ArrowLeft" => back(),
                    _ => {}
                }>
                <p class="tour-count">{move || format!("{} dari {}", step.get() + 1, STEPS.len())}</p>
                <h2 id="tour-title" class="tour-title">{move || STEPS[step.get()].title}</h2>
                <p id="tour-body" class="tour-body">{move || STEPS[step.get()].body}</p>
                <div class="tour-dots" aria-hidden="true">
                    {(0..STEPS.len()).map(|i| view! { <span class:is-on=move || step.get() == i></span> }).collect_view()}
                </div>
                <div class="tour-actions">
                    <Show when=move || not_last.get()>
                        <button type="button" class="tour-skip" on:click=move |_| finish()>"Lewati"</button>
                    </Show>
                    <span class="tour-gap"></span>
                    <Show when=move || not_first.get()>
                        <button type="button" class="btn btn-sm" on:click=move |_| back()>"Kembali"</button>
                    </Show>
                    <button type="button" class="btn btn-sm btn-solid" node_ref=next_ref on:click=move |_| next()>
                        {move || match step.get() {
                            0 => "Mulai tur",
                            s if s + 1 == STEPS.len() => "Selesai",
                            _ => "Lanjut",
                        }}
                    </button>
                </div>
            </div>
        </Show>
    }
}
