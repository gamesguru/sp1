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

        let event = PrecompileEvent::TopologicalRoute(TopologicalRouteEvent {
            current_node,
            next_node,
        });

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
