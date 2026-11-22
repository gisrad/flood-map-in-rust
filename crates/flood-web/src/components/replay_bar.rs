use flood_shared::model::{FloodSnapshot, ReplayScenarioMeta};
use leptos::prelude::*;

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

    view! {
        <Show when=move || replay_active.get()>
            <section class="replay" aria-label="Replay kejadian historis">
                <header class="replay-head">
                    <span class="replay-tag">"Replay historis"</span>
                    <span class="replay-period">{move || current.get().map(|s| s.period).unwrap_or_default()}</span>
                    <button type="button" class="btn btn-sm" on:click=move |_| on_exit.run(())>"Kembali ke data terkini"</button>
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
                    <div class="steps" role="group" aria-label="Hari">
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
                        <h3 class="h-sub">"Hasil model vs kejadian"</h3>
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
