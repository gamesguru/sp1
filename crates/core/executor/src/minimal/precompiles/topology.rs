use sp1_jit::SyscallContext;

pub(crate) unsafe fn topological_route(
    _ctx: &mut impl SyscallContext,
    _arg1: u64,
    _arg2: u64,
) -> Option<u64> {
    // This is a pure STARK constraint precompile without return values.
    // The JIT executor runs outside the ZK environment, so it can safely
    // ignore constraint logic and return `None` (no side-effects on registers)
    // to execute as fast as possible.
    None
}
