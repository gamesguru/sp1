use crate::{
    events::{PrecompileEvent, TopologicalRouteEvent},
    vm::syscall::SyscallRuntime,
    SyscallCode,
};

pub(crate) fn topological_route<'a, RT: SyscallRuntime<'a>>(
    rt: &mut RT,
    syscall_code: SyscallCode,
    arg1: u64,
    arg2: u64,
) -> Option<u64> {
    let clk = rt.core().clk();

    if RT::TRACING {
        let current_node = u32::try_from(arg1).ok()?;
        let next_node = u32::try_from(arg2).ok()?;

        // Fail fast: Ensure exactly one bit flipped before letting the VM proceed
        let xor_diff = current_node ^ next_node;
        if xor_diff.count_ones() != 1 {
            panic!(
                "TopologicalRoute precompile error: Invalid hop from {} to {}. Exactly one bit must differ.",
                current_node, next_node
            );
        }

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
