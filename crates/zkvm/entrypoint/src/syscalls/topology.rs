#[cfg(target_os = "zkvm")]
use core::arch::asm;

/// Executes the TOPOLOGICAL_ROUTE syscall.
///
/// This syscall validates a single-bit hop in a hypercube graph. It ensures that
/// exactly one bit differs between the current and next node IDs, and that both IDs
/// are within the valid hypercube dimension range.
#[allow(unused_variables)]
#[no_mangle]
pub extern "C" fn syscall_topological_route(curr: u32, next: u32) {
    #[cfg(target_os = "zkvm")]
    unsafe {
        asm!(
            "ecall",
            in("t0") crate::syscalls::TOPOLOGICAL_ROUTE,
            in("a0") curr,
            in("a1") next
        );
    }

    #[cfg(not(target_os = "zkvm"))]
    {
        // For non-zkvm targets, we can provide a fallback implementation for testing
        let xor_diff = curr ^ next;
        if curr >= (1 << 10) || next >= (1 << 10) {
            panic!("TopologicalRoute: node IDs must be < 1024");
        }
        if xor_diff.count_ones() != 1 {
            panic!("TopologicalRoute: Exactly one bit must differ");
        }
    }
}
