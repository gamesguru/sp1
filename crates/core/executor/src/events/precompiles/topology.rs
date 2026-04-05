use serde::{Deserialize, Serialize};
use deepsize2::DeepSizeOf;

/// Topological route event.
#[derive(Debug, Clone, Serialize, Deserialize, DeepSizeOf)]
pub struct TopologicalRouteEvent {
    pub current_node: u32,
    pub next_node: u32,
}
