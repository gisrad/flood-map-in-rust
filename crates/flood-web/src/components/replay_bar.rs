use flood_shared::replay::list_replay_scenarios;
use leptos::prelude::*;

#[component]
pub fn ReplayBar(
    replay_active: RwSignal<bool>,
    scenario_id: RwSignal<String>,
    frame_idx: RwSignal<usize>,
    playing: RwSignal<bool>,
    on_exit_replay: Callback<()>,
) -> impl IntoView {
    let scenarios = list_replay_scenarios();

    let current_meta = Signal::derive(move || {
        let id = scenario_id.get();
        list_replay_scenarios()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| list_replay_scenarios().remove(0))
    });

    view! {
        <Show when=move || replay_active.get()>
            <section class="replay-dock" aria-label="Kontrol Mode Replay Historis">
                <div class="replay-banner">
                    <span class="replay-badge">"DATA REPLAY HISTORIS"</span>
                    <span class="replay-period">{move || current_meta.get().period}</span>
                    <button type="button" class="replay-close" on:click=move |_| on_exit_replay.run(())>
                        "Kembali Live"
                    </button>
                </div>

                <div class="replay-scenarios">
                    {scenarios.iter().map(|sc| {
                        let id = sc.id;
                        let title = sc.title;
                        view! {
                            <button
                                type="button"
                                class="sc-btn"
                                aria-pressed=move || (scenario_id.get() == id).to_string()
                                on:click=move |_| {
                                    playing.set(false);
                                    scenario_id.set(id.to_string());
                                    frame_idx.set(0);
                                }
                            >{title}</button>
                        }
                    }).collect_view()}
                </div>

                <div class="replay-controls">
                    <button
                        type="button"
                        class="play-btn"
                        on:click=move |_| playing.update(|p| *p = !*p)
                    >
                        {move || if playing.get() { "Jeda" } else { "Putar" }}
                    </button>

                    <div class="frame-steps">
                        {move || {
                            let sc = current_meta.get();
                            sc.frames.into_iter().enumerate().map(|(i, fr)| {
                                let lbl = fr.step_label.clone();
                                view! {
                                    <button
                                        type="button"
                                        class="step-btn"
                                        aria-pressed=move || (frame_idx.get() == i).to_string()
                                        on:click=move |_| {
                                            playing.set(false);
                                            frame_idx.set(i);
                                        }
                                    >{lbl}</button>
                                }
                            }).collect_view()
                        }}
                    </div>
                </div>

                <p class="replay-narrative">
                    {move || {
                        let sc = current_meta.get();
                        let idx = frame_idx.get().min(sc.frames.len().saturating_sub(1));
                        let fr = &sc.frames[idx];
                        format!("{} — {} ({})", fr.timestamp_wib, fr.narrative, sc.citation)
                    }}
                </p>
            </section>
        </Show>
    }
}
