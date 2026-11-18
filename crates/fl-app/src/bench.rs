//! `--bench <out.json>`: run a scripted scenario on local data, write timings,
//! quit. `fl-bench gpui` starts the app, then checks the report against
//! `bench/budgets-gpui.json`.
//!
//! Views call [`ready`] once their rows are on screen; the first call for a
//! key records when that frame was painted. G1 areas add their scenario
//! steps to [`run`] (search still to come).
//!
//! Steps: launch (`inbox_painted`), tab switch (`switch_tab_ms`), the other
//! queues (`*_painted`), open a cached item in the reading pane
//! (`open_cached_item_ms`: `j` in the Inbox until the item is painted), scroll
//! the 10k-line diff (`diff_scroll_fps`: the Files tab of the fake PR
//! `tune-generated`, `diff::ScrollDown` every frame for two seconds).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::component::diff::DiffState;
use gpui_kit::{App, AsyncWindowContext, Entity, Focusable as _, Global, Window};
use rusqlite::OptionalExtension as _;
use serde::Serialize;

use crate::view_ref::ViewRef;
use crate::workspace::Workspace;

const TAB_SWITCH_SAMPLES: usize = 20;
const OPEN_ITEM_SAMPLES: usize = 20;
/// Frames to wait for an item; a row without a cached item never paints one.
const OPEN_ITEM_MAX_FRAMES: usize = 60;
/// The fake-data PR whose diff changes every tenth line of a 10k-line file
/// (`fl-fake/src/git.rs`).
const BIG_DIFF_HEAD_REF: &str = "tune-generated";
const DIFF_LOAD_MAX_MS: f64 = 15_000.;
const DIFF_SCROLL_MS: f64 = 2_000.;
/// `diff::ScrollDown` moves 1.5 rem (24 px); two per frame, like the Tauri
/// bench's 40 px per frame.
const SCROLLS_PER_FRAME: usize = 2;

#[derive(Default, Serialize)]
struct Report {
    /// Wall clock (unix ms) of named moments, e.g. `inbox_painted`.
    marks: BTreeMap<String, f64>,
    /// One aggregate value per budget key, e.g. `switch_tab_ms` (median).
    durations: BTreeMap<String, Vec<f64>>,
}

struct Bench {
    out: PathBuf,
    report: Report,
    /// The last diff on screen, for the scroll step.
    diff: Option<Entity<DiffState>>,
}

impl Global for Bench {}

pub fn epoch_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// `early`: startup moments before the app existed, so the report shows where
/// launch time goes.
pub fn init(out: PathBuf, early: &[(&str, f64)], cx: &mut App) {
    let mut report = Report::default();
    for (name, ms) in early {
        report.marks.insert((*name).into(), *ms);
    }
    report.marks.insert("app_init".into(), epoch_ms());
    cx.set_global(Bench { out, report, diff: None });
}

/// Record a startup phase (e.g. `store_ready`). No-op outside bench mode.
pub fn mark(name: &str, cx: &mut App) {
    if cx.has_global::<Bench>() {
        cx.global_mut::<Bench>().report.marks.insert(name.into(), epoch_ms());
    }
}

/// A view's rows for `key` are rendered: mark `<key>_painted` after this frame.
/// No-op outside bench mode and after the first call per key.
pub fn ready(key: &'static str, window: &Window, cx: &App) {
    let Some(bench) = cx.try_global::<Bench>() else { return };
    let mark = format!("{key}_painted");
    if bench.report.marks.contains_key(&mark) {
        return;
    }
    window.on_next_frame(move |_, cx| {
        cx.global_mut::<Bench>().report.marks.entry(mark).or_insert_with(epoch_ms);
    });
}

/// A Files tab shows `diff`: mark `diff_painted` and keep it for the scroll
/// step. No-op outside bench mode.
pub fn diff_shown(diff: &Entity<DiffState>, window: &Window, cx: &mut App) {
    let Some(bench) = cx.try_global::<Bench>() else { return };
    if bench.diff.as_ref() != Some(diff) {
        cx.global_mut::<Bench>().diff = Some(diff.clone());
    }
    ready("diff", window, cx);
}

/// Wait until a frame with the current changes has been painted.
async fn painted(cx: &mut AsyncWindowContext) {
    let (tx, rx) = async_channel::bounded::<()>(1);
    let _ = cx.update(|window, _| {
        window.refresh();
        window.on_next_frame(move |_, _| {
            let _ = tx.try_send(());
        });
    });
    let _ = rx.recv().await;
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs[xs.len() / 2]
}

/// The scripted scenario. Runs once the first frame is up, then quits.
pub async fn run(ws: Entity<Workspace>, mut cx: AsyncWindowContext) {
    // The dock shows the first tab's content a few frames after the window
    // opens. Wait for the launch view itself, or the next step hides it
    // before it ever rendered.
    loop {
        painted(&mut cx).await;
        let done = cx.update(|_, cx| cx.global::<Bench>().report.marks.contains_key("inbox_painted"));
        if !matches!(done, Ok(false)) {
            break;
        }
    }

    // Tab switch: two open tabs, alternate, time change -> painted frame.
    let _ = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open(ViewRef::Pulls { repo_id: None }, window, cx)));
    painted(&mut cx).await;
    let mut samples = Vec::with_capacity(TAB_SWITCH_SAMPLES);
    for i in 0..TAB_SWITCH_SAMPLES {
        let target = if i % 2 == 0 { ViewRef::Inbox } else { ViewRef::Pulls { repo_id: None } };
        let start = epoch_ms();
        let _ = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open(target, window, cx)));
        painted(&mut cx).await;
        samples.push(epoch_ms() - start);
    }

    // Views not in the switch loop: record their `*_painted` marks.
    for view in [ViewRef::Issues { repo_id: None }, ViewRef::Reviews] {
        let _ = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open(view, window, cx)));
        painted(&mut cx).await;
    }

    // Open a cached item: select the next Inbox row, time until the reading
    // pane painted it. The store is read-only, so the run is repeatable.
    let _ = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open(ViewRef::Inbox, window, cx)));
    painted(&mut cx).await;
    let mut opens = Vec::with_capacity(OPEN_ITEM_SAMPLES);
    for _ in 0..OPEN_ITEM_SAMPLES {
        let start = epoch_ms();
        let _ = cx.update(|window, cx| {
            cx.global_mut::<Bench>().report.marks.remove("item_painted");
            window.dispatch_action(Box::new(crate::views::SelectNext), cx);
        });
        for _ in 0..OPEN_ITEM_MAX_FRAMES {
            painted(&mut cx).await;
            let mark = cx.update(|_, cx| cx.global::<Bench>().report.marks.get("item_painted").copied());
            if let Ok(Some(at)) = mark {
                opens.push(at - start);
                break;
            }
        }
    }

    let fps = scroll_big_diff(&ws, &mut cx).await;

    let _ = cx.update(|_, cx| {
        let bench = cx.global_mut::<Bench>();
        if let Some(fps) = fps {
            bench.report.durations.insert("diff_scroll_fps".into(), vec![fps]);
        }
        bench.report.durations.insert("switch_tab_ms".into(), vec![median(samples)]);
        if !opens.is_empty() {
            bench.report.durations.insert("open_cached_item_ms".into(), vec![median(opens)]);
        }
        let json = serde_json::to_vec_pretty(&bench.report).expect("report serializes");
        if let Err(e) = std::fs::write(&bench.out, json) {
            eprintln!("forkline: cannot write {}: {e}", bench.out.display());
        }
        cx.quit();
    });
}

/// Open the big fake diff in a PR tab, then scroll it for two seconds and
/// count painted frames. `None` when the data has no such PR or no clone.
async fn scroll_big_diff(ws: &Entity<Workspace>, cx: &mut AsyncWindowContext) -> Option<f64> {
    let pr = cx
        .update(|_, cx| {
            crate::store::Store::global(cx).read(cx).db().read(|c| {
                c.query_row("SELECT issue_id FROM pr_details WHERE head_ref = ?1", [BIG_DIFF_HEAD_REF], |r| r.get(0)).optional()
            })
        })
        .ok()?
        .ok()??;
    let _ = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open(ViewRef::Pr { id: pr }, window, cx)));
    let start = epoch_ms();
    loop {
        painted(cx).await;
        // The tab bar (and so the Files tab) shows once the details are read.
        let _ = cx.update(|window, cx| window.dispatch_action(Box::new(crate::views::ShowFiles), cx));
        let done = cx.update(|_, cx| cx.global::<Bench>().report.marks.contains_key("diff_painted")).ok()?;
        if done {
            break;
        }
        if epoch_ms() - start > DIFF_LOAD_MAX_MS {
            eprintln!("forkline bench: the {BIG_DIFF_HEAD_REF} diff did not load");
            return None;
        }
    }
    let scroll = cx
        .update(|window, cx| {
            let diff = cx.global::<Bench>().diff.clone()?;
            window.focus(&diff.focus_handle(cx), cx);
            cx.build_action("diff::ScrollDown", None).ok()
        })
        .ok()??;
    // Let syntax emphasis land before measuring.
    for _ in 0..10 {
        painted(cx).await;
    }
    let mut frames = 0u32;
    let start = epoch_ms();
    while epoch_ms() - start < DIFF_SCROLL_MS {
        let _ = cx.update(|window, cx| {
            for _ in 0..SCROLLS_PER_FRAME {
                window.dispatch_action(scroll.boxed_clone(), cx);
            }
        });
        painted(cx).await;
        frames += 1;
    }
    Some(f64::from(frames) * 1000. / (epoch_ms() - start))
}
