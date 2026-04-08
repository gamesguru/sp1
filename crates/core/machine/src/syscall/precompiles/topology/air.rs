use core::borrow::Borrow;
use slop_air::{Air, BaseAir};
use slop_algebra::{AbstractField, PrimeField32};
use slop_matrix::Matrix;
use sp1_core_executor::{events::PrecompileEvent, ExecutionRecord, Program, SyscallCode};
use sp1_hypercube::air::{InteractionScope, MachineAir};
use std::{borrow::BorrowMut, mem::MaybeUninit};

use crate::{air::SP1CoreAirBuilder, utils::next_multiple_of_32};

use sp1_topology::{num_cols, TopologicalRouterAir, TopologyCols};

use sp1_primitives::TOPOLOGY_DIM as DIM;

#[derive(Default)]
pub struct TopologyChip;

impl TopologyChip {
    pub const fn new() -> Self {
        Self
    }
}

impl<F> BaseAir<F> for TopologyChip {
    fn width(&self) -> usize {
        num_cols::<DIM>()
    }
}

impl<F: PrimeField32> MachineAir<F> for TopologyChip {
    type Record = ExecutionRecord;
    type Program = Program;

    fn name(&self) -> &'static str {
        "TopologicalRoute"
    }

    fn num_rows(&self, input: &Self::Record) -> Option<usize> {
        let nb_rows = input.get_precompile_events(SyscallCode::TOPOLOGICAL_ROUTE).len();
        let size_log2 = input.fixed_log2_rows::<F, _>(self);
        let padded_nb_rows = next_multiple_of_32(nb_rows, size_log2);
        Some(padded_nb_rows)
    }

    fn generate_trace_into(
        &self,
        input: &ExecutionRecord,
        _output: &mut ExecutionRecord,
        buffer: &mut [MaybeUninit<F>],
    ) {
        let padded_nb_rows = <TopologyChip as MachineAir<F>>::num_rows(self, input).unwrap();

        let route_events = input.get_precompile_events(SyscallCode::TOPOLOGICAL_ROUTE);
        let num_event_rows = route_events.len();
        let n_cols = num_cols::<DIM>();

        unsafe {
            let padding_start = num_event_rows * n_cols;
            let padding_size = (padded_nb_rows - num_event_rows) * n_cols;
            if padding_size > 0 {
                core::ptr::write_bytes(buffer[padding_start..].as_mut_ptr(), 0, padding_size);
            }
        }

        let buffer_ptr = buffer.as_mut_ptr() as *mut F;
        let values =
            unsafe { core::slice::from_raw_parts_mut(buffer_ptr, num_event_rows * n_cols) };

        values.chunks_mut(n_cols).enumerate().for_each(|(idx, row)| {
            let syscall_event = &route_events[idx].0;
            let event = &route_events[idx].1;
            let event = if let PrecompileEvent::TopologicalRoute(event) = event {
                event
            } else {
                unreachable!()
            };

            let cols: &mut TopologyCols<F, DIM> = row.borrow_mut();

            cols.clk_high = F::from_canonical_u32((syscall_event.clk >> 24) as u32);
            cols.clk_low = F::from_canonical_u32((syscall_event.clk & 0xFFFFFF) as u32);
            cols.is_routing = F::one();

            let mut diff_bit_idx = 0;
            let mut diff_count = 0;
            // Unpack u32 into DIM bits and find the single differing bit for the hypercube hop
            for i in 0..DIM {
                let bit = ((event.current_node as u64) >> i) & 1;
                let next_bit = ((event.next_node as u64) >> i) & 1;
                cols.current_bits[i] = F::from_canonical_u64(bit);
                cols.next_bits[i] = F::from_canonical_u64(next_bit);

                cols.selectors[i] = F::zero();
                if bit != next_bit {
                    diff_bit_idx = i;
                    diff_count += 1;
                }
            }
            debug_assert_eq!(
                diff_count, 1,
                "Exactly one bit must differ for a valid hypercube hop"
            );
            // Mark the selector dimension
            cols.selectors[diff_bit_idx] = F::one();
        });
    }

    fn included(&self, shard: &Self::Record) -> bool {
        if let Some(shape) = shard.shape.as_ref() {
            shape.included::<F, _>(self)
        } else {
            !shard.get_precompile_events(SyscallCode::TOPOLOGICAL_ROUTE).is_empty()
        }
    }
}

impl<AB> Air<AB> for TopologyChip
where
    AB: SP1CoreAirBuilder,
{
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local = main.row_slice(0);
        let local: &TopologyCols<AB::Var, DIM> = (*local).borrow();

        // Use the library's AIR to evaluate the bit-flip and selector constraints
        TopologicalRouterAir::<DIM>.eval(builder);

        // Reconstruct composite node IDs for the syscall interaction
        let mut current_node = AB::Expr::zero();
        let mut next_node = AB::Expr::zero();
        for i in 0..DIM {
            let power = AB::Expr::from_canonical_u64(1u64 << i);
            current_node += local.current_bits[i].into() * power.clone();
            next_node += local.next_bits[i].into() * power;
        }

        // Receive syscall interaction, bridge EVM bounds
        builder.send_syscall(
            local.clk_high,
            local.clk_low,
            AB::F::from_canonical_u32(SyscallCode::TOPOLOGICAL_ROUTE.syscall_id()),
            [current_node, AB::Expr::zero(), AB::Expr::zero()],
            [next_node, AB::Expr::zero(), AB::Expr::zero()],
            local.is_routing,
            InteractionScope::Local,
        );
    }
}
