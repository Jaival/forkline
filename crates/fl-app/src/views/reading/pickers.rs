//! Label and assignee pickers of the reading pane: `L` or the action-bar
//! buttons open a list under the header with a search box. Each click sends
//! the whole set (`SetLabels` / `SetAssignees`) through the optimistic action
//! queue, as the Tauri issue view does; a newer set supersedes one not sent
//! yet. The header marks the field `pending` until GitHub confirms it.

use fl_db::queries::issue;
use fl_types::{Action, EditOptions};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, actions,
    div, px,
};

use super::{Detail, ReadingPane};
use crate::store::Store;

actions!(reading, [EditLabels, EditAssignees, ClosePicker]);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PickerKind {
    Labels,
    Assignees,
}

impl PickerKind {
    fn field(self) -> &'static str {
        match self {
            Self::Labels => "labels",
            Self::Assignees => "assignees",
        }
    }
}

/// One label or user the picker offers.
pub(super) struct Choice {
    pub(super) id: i64,
    name: SharedString,
    /// Label colour, six hex digits.
    color: Option<String>,
}

pub(super) struct Picker {
    pub(super) kind: PickerKind,
    /// The repo's labels or the known users; `None` until read.
    pub(super) choices: Option<Vec<Choice>>,
    /// The set the picker sends: the item's set plus the clicks made here.
    pub(super) selected: Vec<i64>,
    search: Entity<InputState>,
    _subscription: Subscription,
}

impl ReadingPane {
    pub(crate) fn edit_labels(&mut self, _: &EditLabels, window: &mut Window, cx: &mut Context<Self>) {
        self.open_picker(PickerKind::Labels, window, cx);
    }

    pub(crate) fn edit_assignees(&mut self, _: &EditAssignees, window: &mut Window, cx: &mut Context<Self>) {
        self.open_picker(PickerKind::Assignees, window, cx);
    }

    /// Open the picker of `kind` and focus its search box; open again closes it.
    pub(crate) fn open_picker(&mut self, kind: PickerKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(detail) = &self.detail else { return };
        if self.picker.as_ref().is_some_and(|p| p.kind == kind) {
            self.close_picker(&ClosePicker, window, cx);
            return;
        }
        let selected = match kind {
            PickerKind::Labels => labels_of(detail).iter().map(|l| l.id).collect(),
            PickerKind::Assignees => self.assignees.iter().map(|u| u.id).collect(),
        };
        let repo_id = detail.row().repo_id;
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(match kind {
            PickerKind::Labels => "Filter labels",
            PickerKind::Assignees => "Filter people",
        }));
        let subscription = cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::Change => cx.notify(),
            // Enter toggles the first match.
            InputEvent::PressEnter { .. } => {
                if let Some(id) = this.visible_choices(cx).first().map(|c| c.id) {
                    this.toggle_choice(id, cx);
                }
            }
            _ => {}
        });
        self.picker = Some(Picker { kind, choices: None, selected, search: search.clone(), _subscription: subscription });
        if self.return_focus.is_none() {
            self.return_focus = window.focused(cx);
        }
        search.update(cx, |search, cx| search.focus(window, cx));

        let task = Store::global(cx).read(cx).read(cx, move |c| issue::edit_options(c, repo_id));
        cx.spawn(async move |this, cx| {
            let Ok(EditOptions { labels, users }) = task.await else { return };
            let _ = this.update(cx, |this, cx| {
                let Some(picker) = this.picker.as_mut().filter(|p| p.kind == kind) else { return };
                picker.choices = Some(match kind {
                    PickerKind::Labels => labels
                        .into_iter()
                        .map(|l| Choice { id: l.id, name: l.name.into(), color: Some(l.color) })
                        .collect(),
                    PickerKind::Assignees => {
                        users.into_iter().map(|u| Choice { id: u.id, name: u.login.into(), color: None }).collect()
                    }
                });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn close_picker(&mut self, _: &ClosePicker, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_none() {
            return;
        }
        if self.reply.is_none() {
            let focus = self.return_focus.take().unwrap_or_else(|| self.focus.clone());
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Add or remove `id` and send the new set.
    pub(super) fn toggle_choice(&mut self, id: i64, cx: &mut Context<Self>) {
        let (Some(picker), Some(detail)) = (self.picker.as_mut(), &self.detail) else { return };
        match picker.selected.iter().position(|x| *x == id) {
            Some(ix) => {
                picker.selected.remove(ix);
            }
            None => picker.selected.push(id),
        }
        let (issue_id, set) = (detail.row().id, picker.selected.clone());
        let action = match picker.kind {
            PickerKind::Labels => Action::SetLabels { issue_id, label_ids: set },
            PickerKind::Assignees => Action::SetAssignees { issue_id, user_ids: set },
        };
        cx.notify();
        self.enqueue(action, cx);
    }

    /// Choices whose name holds the search text (any case).
    pub(super) fn visible_choices(&self, cx: &gpui_kit::App) -> Vec<&Choice> {
        let Some(picker) = &self.picker else { return Vec::new() };
        let needle = picker.search.read(cx).value().trim().to_lowercase();
        picker
            .choices
            .iter()
            .flatten()
            .filter(|c| needle.is_empty() || c.name.to_lowercase().contains(&needle))
            .collect()
    }

    /// The open picker under the header.
    pub(super) fn picker_box(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let picker = self.picker.as_ref()?;
        let theme = cx.theme();
        let choices = self.visible_choices(cx);
        let list = match &picker.choices {
            None => div().text_xs().text_color(theme.muted_foreground).child("Loading…").into_any_element(),
            Some(_) if choices.is_empty() => {
                div().text_xs().text_color(theme.muted_foreground).child("No match").into_any_element()
            }
            Some(_) => div()
                .id("picker-list")
                .max_h(px(220.))
                .overflow_y_scroll()
                .children(choices.into_iter().map(|choice| {
                    let on = picker.selected.contains(&choice.id);
                    let id = choice.id;
                    let swatch = choice.color.as_deref().and_then(super::header::parse_color);
                    h_flex()
                        .id(("pick", id as usize))
                        .test_support()
                        .gap(px(6.))
                        .px(px(6.))
                        .h(px(24.))
                        .items_center()
                        .rounded(px(4.))
                        .cursor_pointer()
                        .text_sm()
                        .hover(|s| s.bg(theme.accent))
                        .child(
                            div().w(px(14.)).when(on, |d| d.child(Icon::new(IconName::Check).xsmall())),
                        )
                        .when_some(swatch, |d, color| d.child(div().size(px(10.)).rounded_full().bg(color)))
                        .child(choice.name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_choice(id, cx)))
                }))
                .into_any_element(),
        };
        Some(
            v_flex()
                .flex_none()
                .key_context("Picker")
                .on_action(cx.listener(Self::close_picker))
                .p(px(8.))
                .gap(px(6.))
                .border_b_1()
                .border_color(theme.border)
                .child(
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(div().text_xs().text_color(theme.muted_foreground).child(match picker.kind {
                            PickerKind::Labels => "Labels",
                            PickerKind::Assignees => "Assignees",
                        }))
                        .child(div().flex_1().child(Input::new(&picker.search).small()))
                        .child(div().text_xs().text_color(theme.muted_foreground).child("Esc to close")),
                )
                .child(list)
                .into_any_element(),
        )
    }

    /// The item's field has a change GitHub has not confirmed yet.
    pub(super) fn field_pending(&self, kind: PickerKind) -> bool {
        let pending = match &self.detail {
            Some(Detail::Issue { thread, .. }) => &thread.pending_fields,
            Some(Detail::Pr(pr)) => &pr.pending_fields,
            None => return false,
        };
        pending.iter().any(|f| f == kind.field())
    }
}

fn labels_of(detail: &Detail) -> &[fl_types::Label] {
    match detail {
        Detail::Issue { detail, .. } => &detail.labels,
        Detail::Pr(pr) => &pr.labels,
    }
}
