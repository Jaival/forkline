//! A queue beside a reading pane, in one dock panel.

use fl_types::{Action, QueueKind, QueueQuery};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, div, px,
};

use super::Built;
use super::queue::{FocusFilter, Queue, QueueEvent, SaveView};
use super::reading::{
    Approve, Done, EditAssignees, EditLabels, NextFile, PrevFile, ReadingPane, Reply, ShowFiles, Snooze, ToggleState,
    ToggleViewed,
};
use crate::store::Store;

pub struct ListView {
    pub(super) queue: Entity<Queue>,
    pub(super) reading: Entity<ReadingPane>,
    _subscription: Subscription,
}

impl ListView {
    pub fn build(query: QueueQuery, title: SharedString, window: &mut Window, cx: &mut App) -> Built {
        let inbox = query.kind == QueueKind::Inbox;
        let kind = query.kind;
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
        crate::bench::list_built(kind, view.clone().into(), cx);
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

    fn toggle_state(&mut self, a: &ToggleState, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.toggle_state(a, window, cx));
    }

    fn edit_labels(&mut self, a: &EditLabels, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.edit_labels(a, window, cx));
    }

    fn edit_assignees(&mut self, a: &EditAssignees, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.edit_assignees(a, window, cx));
    }

    fn done(&mut self, a: &Done, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.done(a, window, cx));
    }

    fn snooze(&mut self, a: &Snooze, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.snooze(a, window, cx));
    }

    fn next_file(&mut self, a: &NextFile, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.next_file(a, window, cx));
    }

    fn prev_file(&mut self, a: &PrevFile, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.prev_file(a, window, cx));
    }

    fn toggle_viewed(&mut self, a: &ToggleViewed, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.toggle_viewed(a, window, cx));
    }

    fn show_files(&mut self, a: &ShowFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.update(cx, |pane, cx| pane.show_files(a, window, cx));
    }

    // `/` and Save view work from the reading pane too; the queue owns the bar.
    fn focus_filter(&mut self, a: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.queue.update(cx, |queue, cx| queue.focus_filter(a, window, cx));
    }

    fn save_view(&mut self, a: &SaveView, window: &mut Window, cx: &mut Context<Self>) {
        self.queue.update(cx, |queue, cx| queue.save_view(a, window, cx));
    }
}

impl Render for ListView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_action(cx.listener(Self::approve))
            .on_action(cx.listener(Self::reply))
            .on_action(cx.listener(Self::toggle_state))
            .on_action(cx.listener(Self::edit_labels))
            .on_action(cx.listener(Self::edit_assignees))
            .on_action(cx.listener(Self::done))
            .on_action(cx.listener(Self::snooze))
            .on_action(cx.listener(Self::next_file))
            .on_action(cx.listener(Self::prev_file))
            .on_action(cx.listener(Self::toggle_viewed))
            .on_action(cx.listener(Self::show_files))
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::save_view))
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
