//! Search DTOs. Owner: agent 5.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::IssueRow;

/// One issue / PR that matched. `snippet` is plain text; matched terms are
/// wrapped in U+E000 / U+E001 so the UI can highlight without parsing HTML.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SearchHit {
    pub row: IssueRow,
    pub snippet: String,
    /// Set when the best match was in a comment.
    pub comment_id: Option<i64>,
}
