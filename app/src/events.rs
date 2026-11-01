//! Tauri events. The UI maps `DataChangedEvent` scopes to TanStack Query
//! invalidations (see ui/src/lib/events.ts).

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct DataChangedEvent(pub fl_types::DataChanged);
