use deepsize2::DeepSizeOf;
use serde::{Deserialize, Serialize};

/// Topological route event.
#[derive(Debug, Clone, Serialize, Deserialize, DeepSizeOf, Default)]
pub struct TopologicalRouteEvent {
    /// The current node ID in the graph.
    pub current_node: u64,
    /// The next valid node ID in the graph.
    pub next_node: u64,
}
