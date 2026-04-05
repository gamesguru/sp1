use sp1_jit::SyscallContext;
use crate::SyscallCode;

pub(crate) unsafe fn topological_route(
    _ctx: &mut impl SyscallContext,
    _arg1: u64,
    _arg2: u64,
) -> Option<u64> {
    // Minimal fast-execution mode ignores trace logic
    None
}
