//! Inline review comments not sent yet, per pull request, for the session.
//!
//! Memory only (no database): they outlive an item switch and a closed tab,
//! not the app. The Files tab writes them, the `R` / `A` review takes them,
//! queue rows mark PRs that have some, and quit or closing a PR tab warns.
//! New commits on the PR leave them where they are.

use std::collections::HashMap;

use fl_types::{DraftReviewComment, IssueId};
use gpui_kit::{App, AppContext as _, Context, Entity, Global};

#[derive(Default)]
pub struct Drafts {
    by_pr: HashMap<IssueId, Vec<DraftReviewComment>>,
}

struct GlobalDrafts(Entity<Drafts>);

impl Global for GlobalDrafts {}

impl Drafts {
    /// Call once at startup, before any view is built.
    pub fn init(cx: &mut App) {
        let drafts = cx.new(|_| Self::default());
        cx.set_global(GlobalDrafts(drafts));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalDrafts>().0.clone()
    }

    /// The drafts of `pr`, in the order written.
    pub fn of(&self, pr: IssueId) -> &[DraftReviewComment] {
        self.by_pr.get(&pr).map_or(&[], Vec::as_slice)
    }

    pub fn count(&self, pr: IssueId) -> usize {
        self.of(pr).len()
    }

    /// `(pull requests, drafts)` with drafts, for the quit warning.
    pub fn totals(&self) -> (usize, usize) {
        (self.by_pr.len(), self.by_pr.values().map(Vec::len).sum())
    }

    pub fn push(&mut self, pr: IssueId, draft: DraftReviewComment, cx: &mut Context<Self>) {
        self.by_pr.entry(pr).or_default().push(draft);
        cx.notify();
    }

    pub fn remove(&mut self, pr: IssueId, ix: usize, cx: &mut Context<Self>) {
        let Some(drafts) = self.by_pr.get_mut(&pr) else { return };
        if ix < drafts.len() {
            drafts.remove(ix);
            if drafts.is_empty() {
                self.by_pr.remove(&pr);
            }
            cx.notify();
        }
    }

    /// All drafts of `pr`, handed to a review that is being sent.
    pub fn take(&mut self, pr: IssueId, cx: &mut Context<Self>) -> Vec<DraftReviewComment> {
        let drafts = self.by_pr.remove(&pr).unwrap_or_default();
        if !drafts.is_empty() {
            cx.notify();
        }
        drafts
    }

    /// Put back drafts of a review the queue rejected, before any new ones.
    pub fn restore(&mut self, pr: IssueId, mut drafts: Vec<DraftReviewComment>, cx: &mut Context<Self>) {
        if drafts.is_empty() {
            return;
        }
        let newer = self.by_pr.remove(&pr).unwrap_or_default();
        drafts.extend(newer);
        self.by_pr.insert(pr, drafts);
        cx.notify();
    }
}

/// The warning text for `(prs, drafts)` unsent drafts.
pub fn warning(prs: usize, drafts: usize) -> String {
    let comments = if drafts == 1 { "1 inline comment".to_owned() } else { format!("{drafts} inline comments") };
    match prs {
        1 => format!("{comments} not sent yet"),
        n => format!("{comments} on {n} pull requests not sent yet"),
    }
}
