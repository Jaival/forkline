//! A queue beside a reading pane, in one dock panel.

use std::time::Duration;

use fl_types::{Action, QueueKind, QueueQuery, QueueRow};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Task, TestSupportExt as _, Window, div, px,
};

use super::Built;
use super::home;
use super::queue::{FocusFilter, Queue, QueueEvent, SaveView, ToggleGrouped};
use super::reading::{
    Approve, Done, EditAssignees, EditLabels, NextFile, PrevFile, ReadingPane, Reply, ShowFiles, Snooze, ToggleState,
    ToggleViewed,
};
use crate::store::Store;
use crate::theme::{ActivePalette as _, TextScale as _};
use crate::view_ref::ViewRef;
use crate::workspace::OpenView;

/// An Inbox row counts as read once it stays selected this long, so j / k
/// skimming does not mark rows read on GitHub.
pub(crate) const READ_AFTER: Duration = Duration::from_millis(1500);

pub struct ListView {
    pub(super) queue: Entity<Queue>,
    pub(super) reading: Entity<ReadingPane>,
    /// Home: how many repos the first sync pinned, until the note is closed.
    pub(super) pin_note: Option<u32>,
    /// Inbox: marks the selected row read after `READ_AFTER`; a new
    /// selection drops it.
    _read_timer: Option<Task<()>>,
    _subscription: Subscription,
    /// Home: auto-pin runs during the first sync, after Home opened.
    _store: Option<Subscription>,
    /// Width of the list beside the reading pane.
    list_width: f32,
}

impl ListView {
    pub fn build(query: QueueQuery, title: SharedString, window: &mut Window, cx: &mut App) -> Built {
        let inbox = query.kind == QueueKind::Inbox;
        let kind = query.kind;
        let queue = cx.new(|cx| Queue::new(query, title, window, cx));
        let focus = queue.read(cx).focus.clone();
        let reading = cx.new(|cx| ReadingPane::new(inbox, cx));
        let pin_note = if kind == QueueKind::Home {
            Store::global(cx).read(cx).db().read(home::read_auto_pin_note).ok().flatten()
        } else {
            None
        };
        let view = cx.new(|cx| {
            let _subscription = cx.subscribe(&queue, move |this: &mut Self, _, ev: &QueueEvent, cx| {
                let QueueEvent::Selected(row) = ev;
                this._read_timer = None;
                if let Some(row) = row
                    && inbox
                    && row.unread
                {
                    let row = row.clone();
                    this._read_timer = Some(cx.spawn(async move |_, cx| {
                        cx.background_executor().timer(READ_AFTER).await;
                        cx.update(|cx| mark_read(&row, cx));
                    }));
                }
                this.reading.update(cx, |reading, cx| reading.set_item(row.clone(), cx));
            });
            let _store = (kind == QueueKind::Home).then(|| {
                cx.subscribe(&Store::global(cx), |this: &mut Self, _, ev: &fl_types::DataChanged, cx| {
                    if ev.scopes.iter().any(|s| s == "repos") {
                        this.pin_note = Store::global(cx).read(cx).db().read(home::read_auto_pin_note).ok().flatten();
                        cx.notify();
                    }
                })
            });
            // Home is the main view: its rows carry reasons, so give the
            // list more room.
            let list_width = if kind == QueueKind::Home { 520. } else { 380. };
            Self { queue, reading, pin_note, _read_timer: None, _subscription, _store, list_width }
        });
        crate::bench::list_built(kind, view.clone().into(), cx);
        Built { view: view.into(), focus }
    }
}

/// Reading a notification marks it read, on GitHub too (through the action
/// queue). A bench run leaves its database alone.
fn mark_read(row: &QueueRow, cx: &mut App) {
    let store = Store::global(cx).read(cx);
    if store.read_only() {
        return;
    }
    let engine = store.sync().clone();
    let id = row.key.clone();
    store.run(cx, async move { engine.enqueue(Action::NotificationRead { id }).await }).detach();
}

impl ListView {
    /// Close the auto-pin note for good.
    fn close_pin_note(&mut self, cx: &mut Context<Self>) {
        self.pin_note = None;
        let db = Store::global(cx).read(cx).db().clone();
        cx.background_spawn(async move { db.write(|c| fl_db::queries::layout::delete_setting(c, home::AUTO_PIN_NOTE)) })
            .detach();
        cx.notify();
    }

    fn pin_note_element(&self, count: u32, cx: &Context<Self>) -> impl IntoElement + use<> {
        let p = cx.palette();
        h_flex()
            .id("home-pin-note")
            .test_support()
            .flex_none()
            .w_full()
            .px(px(20.))
            .py(px(8.))
            .gap(px(8.))
            .items_center()
            .bg(p.raised)
            .border_b_1()
            .border_color(p.border)
            .text_secondary()
            .child(div().flex_1().min_w_0().child(home::auto_pin_text(count)))
            .child(
                Button::new("home-pin-change")
                    .ghost()
                    .small()
                    .label("Change\u{2026}")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenView(ViewRef::Repos)), cx)),
            )
            .child(
                Button::new("home-pin-close")
                    .ghost()
                    .small()
                    .label("\u{d7}")
                    .tooltip("Close")
                    .on_click(cx.listener(|this, _, _, cx| this.close_pin_note(cx))),
            )
    }

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

    fn toggle_grouped(&mut self, a: &ToggleGrouped, window: &mut Window, cx: &mut Context<Self>) {
        self.queue.update(cx, |queue, cx| queue.toggle_grouped(a, window, cx));
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
            .on_action(cx.listener(Self::toggle_grouped))
            .child(
                v_flex()
                    .size_full()
                    .when_some(self.pin_note, |d, count| d.child(self.pin_note_element(count, cx)))
                    .child(
                        div().flex_1().min_h_0().child(
                            h_resizable("list-view")
                                .child(
                                    resizable_panel()
                                        .size(px(self.list_width))
                                        .size_range(px(280.)..px(640.))
                                        .child(self.queue.clone()),
                                )
                                .child(resizable_panel().child(self.reading.clone())),
                        ),
                    ),
            )
    }
}
