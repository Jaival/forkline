//! `--bench <out.json>`: run a scripted scenario on local data, write timings,
//! quit. `fl-bench gpui` starts the app, then checks the report against
//! `bench/budgets-gpui.json`.
//!
//! Views call [`ready`] once their rows are on screen; the first call for a
//! key records when that frame was painted. G1 areas add their scenario
//! steps to [`run`].
//!
//! Steps: launch (`inbox_painted`), tab switch (`switch_tab_ms`), the other
//! queues (`*_painted`), open a cached item in the reading pane
//! (`open_cached_item_ms`: `j` in the Inbox until the item is painted), filter
//! the Issues queue with 20 `/` queries (`search_p95_ms`: p95 of change ->
//! first filtered page painted, 10 rounds after one warm-up round), scroll
//! the 10k-line diff (`diff_scroll_fps`: the Files tab of the fake PR
//! `tune-generated`, `diff::ScrollDown` every frame for two seconds), scroll
//! a Conversation tab (`conversation_scroll_fps`: the fake PR with the most
//! comments, 40 px a frame down and back up for two seconds), open each
//! step 7 view once (`open_<key>_ms`: open -> `<key>_painted`; no budget
//! yet).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::component::diff::DiffState;
use fl_types::QueueKind;
use gpui_kit::{AnyView, App, AsyncWindowContext, Entity, Focusable as _, Global, ListState, Window, px};
use rusqlite::OptionalExtension as _;
use serde::Serialize;

use crate::view_ref::ViewRef;
use crate::workspace::Workspace;

const TAB_SWITCH_SAMPLES: usize = 20;
const OPEN_ITEM_SAMPLES: usize = 20;
/// Frames to wait for an item; a row without a cached item never paints one.
const OPEN_ITEM_MAX_FRAMES: usize = 60;
/// The `/` queries of the search step: the set of `fl-search`'s
/// `queue_filter_bench` example.
const SEARCH_QUERIES: &[&str] = &[
    "fixture", "fixture item", "project 42", "gener", "fix*", "PR #7", "is:pr fixture",
    "is:issue is:open fixture", "-is:pr fixture", "author:octocat fixture", "repo:forkline-demo/project-007",
    "repo:forkline-demo/project-007 is:pr item", "is:pr", "author:octocat is:open", "label:bug fixture",
    "assignee:octocat", "fixture -generated", "\"local data\" project", "NEAR( AND \" * -", "zzzznomatch",
];
/// Round 0 warms the caches and is dropped.
const SEARCH_ROUNDS: usize = 11;
/// The fake-data PR whose diff changes every tenth line of a 10k-line file
/// (`fl-fake/src/git.rs`).
const BIG_DIFF_HEAD_REF: &str = "tune-generated";
const DIFF_LOAD_MAX_MS: f64 = 15_000.;
const DIFF_SCROLL_MS: f64 = 2_000.;
/// `diff::ScrollDown` moves 1.5 rem (24 px); two per frame, like the Tauri
/// bench's 40 px per frame.
const SCROLLS_PER_FRAME: usize = 2;
/// Frames to wait for a Conversation tab to show its entries.
const CONVERSATION_LOAD_MAX_FRAMES: usize = 120;
const CONVERSATION_SCROLL_MS: f64 = 2_000.;
const CONVERSATION_SCROLL_PX: f32 = 40.;

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
    /// The last Issues list view built, for the search step.
    issues: Option<AnyView>,
    /// The list of the last Conversation tab filled, for its scroll step.
    conversation: Option<ListState>,
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
    cx.set_global(Bench { out, report, diff: None, issues: None, conversation: None });
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

/// A queue list view was built: keep the Issues one for the search step.
/// No-op outside bench mode.
pub fn list_built(kind: QueueKind, view: AnyView, cx: &mut App) {
    if kind == QueueKind::Issues && cx.has_global::<Bench>() {
        cx.global_mut::<Bench>().issues = Some(view);
    }
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

/// A Conversation tab was filled: keep its list for the scroll step. No-op
/// outside bench mode.
pub fn conversation_shown(state: &ListState, cx: &mut App) {
    if cx.has_global::<Bench>() {
        cx.global_mut::<Bench>().conversation = Some(state.clone());
    }
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

    // The Inbox reading pane, beside its queue, shows the last item opened.
    let inbox_state = cx.update(|_, cx| cx.global::<Bench>().conversation.clone()).ok().flatten();
    let inbox_conversation_fps = match inbox_state {
        Some(state) if state.item_count() > 0 => Some(scroll_list(&state, &mut cx).await),
        _ => None,
    };

    let views = open_views(&ws, &mut cx).await;

    let search = search_p95(&ws, &mut cx).await;

    let conversation_fps = scroll_conversation(&ws, &mut cx).await;

    let fps = scroll_big_diff(&ws, &mut cx).await;

    let _ = cx.update(|_, cx| {
        let bench = cx.global_mut::<Bench>();
        if let Some(fps) = fps {
            bench.report.durations.insert("diff_scroll_fps".into(), vec![fps]);
        }
        if let Some(fps) = conversation_fps {
            bench.report.durations.insert("conversation_scroll_fps".into(), vec![fps]);
        }
        if let Some(fps) = inbox_conversation_fps {
            bench.report.durations.insert("inbox_conversation_scroll_fps".into(), vec![fps]);
        }
        bench.report.durations.insert("switch_tab_ms".into(), vec![median(samples)]);
        if let Some(ms) = search {
            bench.report.durations.insert("search_p95_ms".into(), vec![ms]);
        }
        if !opens.is_empty() {
            bench.report.durations.insert("open_cached_item_ms".into(), vec![median(opens)]);
        }
        for (key, ms) in views {
            bench.report.durations.insert(format!("open_{key}_ms"), vec![ms]);
        }
        let json = serde_json::to_vec_pretty(&bench.report).expect("report serializes");
        if let Err(e) = std::fs::write(&bench.out, json) {
            eprintln!("forkline: cannot write {}: {e}", bench.out.display());
        }
        cx.quit();
    });
}

/// Open each step 7 view in a new tab; time open -> its rows painted.
async fn open_views(ws: &Entity<Workspace>, cx: &mut AsyncWindowContext) -> Vec<(&'static str, f64)> {
    let repo_id = cx
        .update(|_, cx| {
            crate::store::Store::global(cx)
                .read(cx)
                .db()
                .read(|c| c.query_row("SELECT id FROM repos ORDER BY pinned DESC, id LIMIT 1", [], |r| r.get(0)).optional())
        })
        .ok()
        .and_then(Result::ok)
        .flatten()
        .unwrap_or(1);
    let views = [
        ("home", ViewRef::Home),
        ("repos", ViewRef::Repos),
        ("starred", ViewRef::Starred),
        ("explore", ViewRef::Explore),
        ("repo", ViewRef::Repo { repo_id }),
        ("search", ViewRef::Search { q: "fixture".into() }),
    ];
    let mut out = Vec::new();
    for (key, view) in views {
        let start = epoch_ms();
        let _ = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open(view, window, cx)));
        let mark = format!("{key}_painted");
        for _ in 0..OPEN_ITEM_MAX_FRAMES {
            painted(cx).await;
            if let Ok(Some(at)) = cx.update(|_, cx| cx.global::<Bench>().report.marks.get(&mark).copied()) {
                out.push((key, at - start));
                break;
            }
        }
    }
    out
}

/// Filter the Issues queue by each query, time change -> first filtered page
/// painted, and return the p95. `None` when the queue never showed.
async fn search_p95(ws: &Entity<Workspace>, cx: &mut AsyncWindowContext) -> Option<f64> {
    let _ = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open(ViewRef::Issues { repo_id: None }, window, cx)));
    painted(cx).await;
    let view = cx.update(|_, cx| cx.global::<Bench>().issues.clone()).ok()??;
    let mut samples = Vec::with_capacity((SEARCH_ROUNDS - 1) * SEARCH_QUERIES.len());
    for round in 0..SEARCH_ROUNDS {
        for query in SEARCH_QUERIES {
            let start = epoch_ms();
            let _ = cx.update(|window, cx| {
                cx.global_mut::<Bench>().report.marks.remove("filter_painted");
                crate::views::set_filter(&view, query, window, cx);
            });
            for _ in 0..OPEN_ITEM_MAX_FRAMES {
                painted(cx).await;
                let mark = cx.update(|_, cx| cx.global::<Bench>().report.marks.get("filter_painted").copied());
                if let Ok(Some(at)) = mark {
                    if round > 0 {
                        samples.push(at - start);
                    }
                    break;
                }
            }
        }
    }
    let _ = cx.update(|window, cx| crate::views::set_filter(&view, "", window, cx));
    painted(cx).await;
    if samples.is_empty() {
        return None;
    }
    samples.sort_by(f64::total_cmp);
    Some(samples[samples.len() * 95 / 100])
}

/// Open the fake PR with the most comments in a PR tab, then scroll its
/// Conversation tab down and back up for two seconds and count painted
/// frames. `None` when the data has no PR with comments.
async fn scroll_conversation(ws: &Entity<Workspace>, cx: &mut AsyncWindowContext) -> Option<f64> {
    let pr: i64 = cx
        .update(|_, cx| {
            crate::store::Store::global(cx).read(cx).db().read(|c| {
                c.query_row(
                    "SELECT c.issue_id FROM comments c JOIN issues i ON i.id = c.issue_id WHERE i.is_pr = 1 \
                     GROUP BY c.issue_id ORDER BY count(*) DESC, c.issue_id LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
            })
        })
        .ok()?
        .ok()??;
    let _ = cx.update(|_, cx| cx.global_mut::<Bench>().conversation = None);
    let _ = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open(ViewRef::Pr { id: pr }, window, cx)));
    let mut state = None;
    for _ in 0..CONVERSATION_LOAD_MAX_FRAMES {
        painted(cx).await;
        state = cx.update(|_, cx| cx.global::<Bench>().conversation.clone()).ok()?.filter(|s| s.item_count() > 0);
        if state.is_some() {
            break;
        }
    }
    Some(scroll_list(&state?, cx).await)
}

/// Scroll `state` 40 px a frame, down and back up, for two seconds; the
/// painted frames per second.
async fn scroll_list(state: &ListState, cx: &mut AsyncWindowContext) -> f64 {
    // Let the bodies lay out (the list measures them as they show).
    for _ in 0..10 {
        painted(cx).await;
    }
    let mut frames = 0u32;
    let mut down = true;
    let start = epoch_ms();
    while epoch_ms() - start < CONVERSATION_SCROLL_MS {
        let top = -state.scroll_px_offset_for_scrollbar().y;
        let max = state.max_offset_for_scrollbar().y;
        if down && top >= max {
            down = false;
        } else if !down && top <= px(0.) {
            down = true;
        }
        let step = px(CONVERSATION_SCROLL_PX).min(if down { max - top } else { top });
        state.scroll_by(if down { step } else { -step });
        painted(cx).await;
        frames += 1;
    }
    f64::from(frames) * 1000. / (epoch_ms() - start)
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
