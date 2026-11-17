//! A queue beside a reading pane, in one dock panel.

use fl_types::{Action, QueueKind, QueueQuery};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, div, px,
};

use super::Built;
use super::queue::{Queue, QueueEvent};
use super::reading::{Approve, Done, ReadingPane, Reply, Snooze};
use crate::store::Store;

pub struct ListView {
    pub(super) queue: Entity<Queue>,
    pub(super) reading: Entity<ReadingPane>,
    _subscription: Subscription,
}

impl ListView {
    pub fn build(query: QueueQuery, title: SharedString, window: &mut Window, cx: &mut App) -> Built {
        let inbox = query.kind == QueueKind::Inbox;
        let queue = cx.new(|cx| Queue::new(query, title, window, cx));
        let focus = queue.read(cx).focus.clone();
        let reading = cx.new(|cx| ReadingPane::new(inbox, cx));
        let view = cx.new(|cx| {
            let _subscription = cx.subscribe(&queue, move |this: &mut Self, _, ev: &QueueEvent, cx| {
                let QueueEvent::Selected(row) = ev;
                // Reading a notification marks it read, on GitHub too (through
                // the action queue). A bench run leaves its database alone.
                let store = Store::global(cx).read(cx);
                if let Some(row) = row
                    && inbox
                    && row.unread
                    && !store.read_only()
                {
                    let engine = store.sync().clone();
                    let id = row.key.clone();
                    store.run(cx, async move { engine.enqueue(Action::NotificationRead { id }).await }).detach();
                }
                this.reading.update(cx, |reading, cx| reading.set_item(row.clone(), cx));
            });
            Self { queue, reading, _subscription }
        });
        Built { view: view.into(), focus }
    }
}

impl ListView {
    // The queue holds the focus; the action keys act on the item on show.
    fn approve(&mut self, a: &Approve, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.approve(a, window, cx));
    }

    fn reply(&mut self, a: &Reply, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.reply(a, window, cx));
    }

    fn done(&mut self, a: &Done, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.done(a, window, cx));
    }

    fn snooze(&mut self, a: &Snooze, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.snooze(a, window, cx));
    }
}

impl Render for ListView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_action(cx.listener(Self::approve))
            .on_action(cx.listener(Self::reply))
            .on_action(cx.listener(Self::done))
            .on_action(cx.listener(Self::snooze))
            .child(
                h_resizable("list-view")
                    .child(
                        resizable_panel()
                            .size(px(380.))
                            .size_range(px(280.)..px(640.))
                            .child(self.queue.clone()),
                    )
                    .child(resizable_panel().child(self.reading.clone())),
            )
    }
}
