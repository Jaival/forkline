//! A queue beside a reading pane, in one dock panel.

use fl_types::{Action, QueueKind, QueueQuery};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Subscription, Window, px,
};

use super::Built;
use super::queue::{Queue, QueueEvent};
use super::reading::ReadingPane;
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
        let reading = cx.new(ReadingPane::new);
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

impl Render for ListView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_resizable("list-view")
            .child(
                resizable_panel().size(px(380.)).size_range(px(280.)..px(640.)).child(self.queue.clone()),
            )
            .child(resizable_panel().child(self.reading.clone()))
    }
}
