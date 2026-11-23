use flood_shared::model::{FloodSnapshot, ReplayScenarioMeta};
use leptos::html::Div;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

#[component]
pub fn ReplayBar(
    scenarios: StoredValue<Vec<ReplayScenarioMeta>>,
    snapshot: Signal<Option<FloodSnapshot>>,
    replay_active: RwSignal<bool>,
    scenario_id: RwSignal<String>,
    frame_idx: RwSignal<usize>,
    playing: RwSignal<bool>,
    on_pick: Callback<String>,
    on_exit: Callback<()>,
) -> impl IntoView {
    let current = Signal::derive(move || {
        let id = scenario_id.get();
        scenarios.with_value(|all| {
            all.iter()
                .find(|s| s.id == id)
                .cloned()
                .or_else(|| all.first().cloned())
        })
    });
    let has_outcome = Signal::derive(move || {
        current.with(|c| c.as_ref().is_some_and(|sc| !sc.outcome.is_empty()))
    });
    // Layar sempit: narasi penuh, rincian hasil, dan sumber dibuka lewat tombol agar peta tetap
    // terlihat. Layar lebar selalu menampilkan semuanya (tombol disembunyikan CSS).
    let details_open = RwSignal::new(false);

    // Hari aktif tetap terlihat di deret hari yang digulir mendatar, juga saat replay diputar.
    let steps_ref = NodeRef::<Div>::new();
    Effect::new(move |_| {
        let idx = frame_idx.get();
        scenario_id.track();
        let Some(row) = steps_ref.get() else {
            return;
        };
        let Ok(Some(btn)) = row.query_selector(&format!(".step:nth-child({})", idx + 1)) else {
            return;
        };
        let Ok(btn) = btn.dyn_into::<web_sys::HtmlElement>() else {
            return;
        };
        let target = btn.offset_left() - (row.client_width() - btn.offset_width()) / 2;
        row.set_scroll_left(target.max(0));
    });

    view! {
        <Show when=move || replay_active.get()>
            <section class="replay" class:is-open=move || details_open.get() class:has-outcome=move || has_outcome.get()
                aria-label="Replay kejadian historis">
                <header class="replay-head">
                    <span class="replay-tag">"Replay historis"</span>
                    <span class="replay-period">{move || current.get().map(|s| s.period).unwrap_or_default()}</span>
                    <select class="replay-pick" aria-label="Pilih kejadian"
                        on:change=move |ev| on_pick.run(event_target_value(&ev))>
                        {scenarios.get_value().into_iter().map(|sc| {
                            let id = sc.id.clone();
                            view! {
                                <option value=sc.id.clone() prop:selected=move || scenario_id.get() == id>{sc.title.clone()}</option>
                            }
                        }).collect_view()}
                    </select>
                    <button type="button" class="btn btn-sm replay-exit" aria-label="Kembali ke data terkini"
                        on:click=move |_| on_exit.run(())>
                        <span class="lbl-wide">"Kembali ke data terkini"</span>
                        <span class="lbl-narrow">"← Data terkini"</span>
                    </button>
                </header>

                <div class="replay-cards" role="group" aria-label="Pilih kejadian">
                    {scenarios.get_value().into_iter().map(|sc| {
                        let id = sc.id.clone();
                        let id_p = sc.id.clone();
                        let src = format!("/art/replay-{}.webp", sc.id);
                        let img_hidden = RwSignal::new(false);
                        view! {
                            <button type="button" class="replay-card"
                                aria-pressed=move || (scenario_id.get() == id_p).to_string()
                                on:click=move |_| on_pick.run(id.clone())>
                                <Show when=move || !img_hidden.get()>
                                    <img src=src.clone() alt="" loading="lazy" width="120" height="68"
                                        on:error=move |_| img_hidden.set(true)/>
                                </Show>
                                <span class="rc-title">{sc.title.clone()}</span>
                            </button>
                        }
                    }).collect_view()}
                </div>

                <div class="replay-steps">
                    <button type="button" class="btn btn-solid btn-sm play" on:click=move |_| playing.update(|p| *p = !*p)
                        aria-label=move || if playing.get() { "Jeda" } else { "Putar" }>
                        {move || if playing.get() { "Jeda" } else { "Putar" }}
                    </button>
                    <div class="steps" node_ref=steps_ref role="group" aria-label="Hari">
                        {move || current.get().map(|sc| sc.frames.into_iter().enumerate().map(|(i, fr)| view! {
                            <button type="button" class="step" aria-pressed=move || (frame_idx.get() == i).to_string()
                                on:click=move |_| { playing.set(false); frame_idx.set(i); }>{fr.step_label}</button>
                        }).collect_view())}
                    </div>
                </div>

                <p class="replay-narr">
                    {move || current.get().map(|sc| {
                        let idx = frame_idx.get().min(sc.frames.len().saturating_sub(1));
                        sc.frames.get(idx).map(|fr| format!("{} — {}", fr.timestamp_wib, fr.narrative)).unwrap_or_default()
                    })}
                </p>
                {move || current.get().filter(|sc| !sc.outcome.is_empty()).map(|sc| view! {
                    <section class="replay-outcome" aria-label="Hasil model dibanding kejadian">
                        <div class="ro-head">
                            <h3 class="h-sub">"Hasil model vs kejadian"</h3>
                            <button type="button" class="replay-more" aria-expanded=move || details_open.get().to_string()
                                on:click=move |_| details_open.update(|v| *v = !*v)>
                                "Rincian"<span class="chev" aria-hidden="true">"▾"</span>
                            </button>
                        </div>
                        <ul>
                            {sc.outcome.into_iter().map(|line| view! { <li>{line}</li> }).collect_view()}
                        </ul>
                    </section>
                })}
                <p class="replay-prov">
                    {move || current.get().map(|sc| sc.provenance)}
                    {move || snapshot.get().and_then(|s| s.replay_note).map(|n| format!(" {n}"))}
                </p>
                <p class="replay-cite">{move || current.get().map(|sc| format!("Rujukan kejadian: {}", sc.citation))}</p>
            </section>
        </Show>
    }
}
