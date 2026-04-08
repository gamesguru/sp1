use crate::{
    events::{PrecompileEvent, TopologicalRouteEvent},
    vm::syscall::SyscallRuntime,
    SyscallCode,
};

/// The dimensionality of the hypercube graph used for oblivious routing constraints.
///
/// Design Philosophy:
/// - **10 Dimensions (1024 nodes):** Chosen as a strictly power-of-two graph size. This aligns
///   perfectly with the fundamental paddings required by STARK-based execution traces, eliminating
///   the overhead of uneven constraint tables.
/// - **Oblivious & Predetermined Routing:** Hypercubes provide highly symmetric, highly
///   connected paths between any two nodes. By operating on a hypercube, data-flow algorithms
///   can determine exact, collision-free paths obliviously (independent of the actual data payload).
///   This prevents side-channel leakage and ensures a strictly deterministic execution trace
///   length regardless of the routing choices.
/// - **Hardware Efficiency (CPU & GPU):** Because the graph nodes are represented merely by bits,
///   validating a "hop" between two nodes across the network is reduced to a simple integer `XOR`
///   and a hardware-native `count_ones()` instruction. This maps perfectly to the parallelized
///   architectures of modern GPUs and the ALUs of CPUs, achieving maximum performance with
///   negligible arithmetic overhead compared to arbitrary graph structures.
/// - **Diameter vs Optimality:** While a 10-D hypercube has a slightly wider diameter than heavily
///   optimized arbitrary-degree topologies (e.g., odd dimensional models or specific butterfly
///   networks), its uniform node degree and bitwise traversal rule make it definitively ideal for
///   ZK circuits where mathematical constraint simplicity drastically outweighs traditional
///   shortest-path routing.
pub const TOPOLOGY_DIM: u32 = 10;

pub(crate) fn validate_hop(arg1: u64, arg2: u64) -> Result<(u32, u32), String> {
    let current_node =
        u32::try_from(arg1).map_err(|_| format!("current_node {arg1} exceeds u32"))?;
    let next_node = u32::try_from(arg2).map_err(|_| format!("next_node {arg2} exceeds u32"))?;

    let xor_diff = current_node ^ next_node;
    if current_node >= (1 << TOPOLOGY_DIM) || next_node >= (1 << TOPOLOGY_DIM) {
        return Err(format!(
            "TopologicalRoute precompile error: node IDs must be < {} ({TOPOLOGY_DIM}-bit hypercube). Got {current_node} -> {next_node}.",
            1 << TOPOLOGY_DIM
        ));
    }

    if xor_diff.count_ones() != 1 {
        return Err(format!(
            "TopologicalRoute precompile error: Invalid hop from {current_node} to {next_node}. Exactly one bit must differ."
        ));
    }
    Ok((current_node, next_node))
}

pub(crate) fn topological_route<'a, RT: SyscallRuntime<'a>>(
    rt: &mut RT,
    syscall_code: SyscallCode,
    arg1: u64,
    arg2: u64,
) -> Option<u64> {
    let clk = rt.core().clk();

    if RT::TRACING {
        // Fail fast: Ensure exactly one bit flipped before letting the VM proceed
        let (current_node, next_node) = match validate_hop(arg1, arg2) {
            Ok(nodes) => nodes,
            Err(e) => panic!("{}", e),
        };

        let event =
            PrecompileEvent::TopologicalRoute(TopologicalRouteEvent { current_node, next_node });

        let syscall_event = rt.syscall_event(
            clk,
            syscall_code,
            arg1,
            arg2,
            false,
            rt.core().next_pc(),
            rt.core().exit_code(),
        );

        rt.add_precompile_event(syscall_code, syscall_event, event);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_hop() {
        // Valid 1-bit hops
        assert_eq!(validate_hop(0, 1), Ok((0, 1)));
        assert_eq!(validate_hop(1, 0), Ok((1, 0)));
        assert_eq!(validate_hop(0b1010, 0b1011), Ok((10, 11)));
        assert_eq!(validate_hop(0b1111, 0b0111), Ok((15, 7)));
    }

    #[test]
    fn test_invalid_zero_hop() {
        // Same node
        assert!(validate_hop(5, 5).is_err());
    }

    #[test]
    fn test_invalid_multi_hop() {
        // Two bits flipped
        assert!(validate_hop(0b00, 0b11).is_err());
        assert!(validate_hop(1, 4).is_err()); // 001 to 100 (2 bits diff)
    }

    #[test]
    fn test_invalid_9_bit_hop() {
        // 9 bits flipped (e.g., 0000000000 to 0111111111)
        assert!(validate_hop(0b0000000000, 0b0111111111).is_err());
    }

    #[test]
    fn test_invalid_out_of_bounds() {
        assert!(validate_hop(0, u64::MAX).is_err());
        assert!(validate_hop(u64::MAX, 1).is_err());
    }
}
