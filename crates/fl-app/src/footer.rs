//! Footer: sync status ("Synced 12 s ago", pending / failed actions, errors).

use std::time::Duration;

use fl_types::{DataChanged, SyncStatus};
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, IntoElement, ParentElement as _, Render, Styled as _, Subscription, Task, Window, div,
};

use crate::store::Store;

/// The relative time ticks once a second; the status itself is refetched on
/// a `sync` change and every `REFETCH_EVERY` ticks.
const TICK: Duration = Duration::from_secs(1);
const REFETCH_EVERY: u32 = 10;

pub struct Footer {
    status: Option<SyncStatus>,
    _tick: Task<()>,
    _subscription: Subscription,
}

impl Footer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store = Store::global(cx);
        let subscription = cx.subscribe(&store, |this, _, ev: &DataChanged, cx| {
            if ev.scopes.iter().any(|s| s == "sync") {
                this.refetch(cx);
            }
        });
        let tick = cx.spawn(async move |this, cx| {
            let mut n = 0u32;
            loop {
                cx.background_executor().timer(TICK).await;
                n += 1;
                let alive = this.update(cx, |this, cx| {
                    if n.is_multiple_of(REFETCH_EVERY) {
                        this.refetch(cx);
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        let mut this = Self { status: None, _tick: tick, _subscription: subscription };
        this.refetch(cx);
        this
    }

    fn refetch(&mut self, cx: &mut Context<Self>) {
        let store = Store::global(cx);
        let sync = store.read(cx).sync().clone();
        let task = store.read(cx).run(cx, async move { sync.status().await });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(status)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.status = Some(status);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

fn ago(secs: i64) -> String {
    match secs.max(0) {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86_400 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86_400),
    }
}

impl Render for Footer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (synced, pending, failed, error) = match &self.status {
            None => ("Loading…".to_owned(), 0, 0, None),
            Some(s) => (
                s.last_synced_at.map_or("Not synced yet".to_owned(), |t| {
                    format!("Synced {} ago", ago(fl_sync::now() - t))
                }),
                s.pending_actions,
                s.failed_actions,
                s.error.clone(),
            ),
        };
        StatusBar::new()
            .left(
                h_flex()
                    .gap_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(synced)
                    .when_some((pending > 0).then_some(pending), |el, n| el.child(format!("{n} pending")))
                    .when_some((failed > 0).then_some(failed), |el, n| {
                        el.child(div().text_color(theme.danger).child(format!("{n} failed")))
                    })
                    .when_some(error, |el, e| el.child(div().text_color(theme.danger).child(e))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::ago;

    #[test]
    fn formats_relative_times() {
        assert_eq!(ago(12), "12 s");
        assert_eq!(ago(125), "2 min");
        assert_eq!(ago(7200), "2 h");
        assert_eq!(ago(-3), "0 s");
    }
}
