use core::borrow::Borrow;
use slop_air::{Air, AirBuilder, BaseAir};
use slop_algebra::{AbstractField, PrimeField32};
use slop_matrix::Matrix;
use sp1_core_executor::{events::PrecompileEvent, ExecutionRecord, Program, SyscallCode};
use sp1_derive::AlignedBorrow;
use sp1_hypercube::air::{InteractionScope, MachineAir};
use std::{borrow::BorrowMut, mem::MaybeUninit};

use crate::{air::SP1CoreAirBuilder, utils::next_multiple_of_32};

pub const DIM: usize = 10;

#[derive(Debug, Clone, AlignedBorrow)]
#[repr(C)]
pub struct TopologyCols<T> {
    /// Clock cycle of the syscall (split into high and low parts)
    pub clk_high: T,
    pub clk_low: T,

    /// Is this a real routing operation or padding?
    pub is_routing: T,

    /// 10-dimensional Hypercube architecture
    pub current_bits: [T; DIM],
    pub selectors: [T; DIM],
    pub next_bits: [T; DIM],
}

pub const NUM_COLS: usize = core::mem::size_of::<TopologyCols<u8>>();

#[derive(Default)]
pub struct TopologyChip;

impl TopologyChip {
    pub const fn new() -> Self {
        Self
    }
}

impl<F> BaseAir<F> for TopologyChip {
    fn width(&self) -> usize {
        NUM_COLS
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

        unsafe {
            let padding_start = num_event_rows * NUM_COLS;
            let padding_size = (padded_nb_rows - num_event_rows) * NUM_COLS;
            if padding_size > 0 {
                core::ptr::write_bytes(buffer[padding_start..].as_mut_ptr(), 0, padding_size);
            }
        }

        let buffer_ptr = buffer.as_mut_ptr() as *mut F;
        let values =
            unsafe { core::slice::from_raw_parts_mut(buffer_ptr, num_event_rows * NUM_COLS) };

        values.chunks_mut(NUM_COLS).enumerate().for_each(|(idx, row)| {
            let syscall_event = &route_events[idx].0;
            let event = &route_events[idx].1;
            let event = if let PrecompileEvent::TopologicalRoute(event) = event {
                event
            } else {
                unreachable!()
            };

            let cols: &mut TopologyCols<F> = row.borrow_mut();

            cols.clk_high = F::from_canonical_u32((syscall_event.clk >> 24) as u32);
            cols.clk_low = F::from_canonical_u32((syscall_event.clk & 0xFFFFFF) as u32);
            cols.is_routing = F::one();

            let mut diff_bit_idx = 0;
            let mut diff_count = 0;
            // Unpack u32 into 10 bits and find the single differing bit for the hypercube hop
            for i in 0..DIM {
                let bit = (event.current_node >> i) & 1;
                let next_bit = (event.next_node >> i) & 1;
                cols.current_bits[i] = F::from_canonical_u32(bit);
                cols.next_bits[i] = F::from_canonical_u32(next_bit);

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
        let local: &TopologyCols<AB::Var> = (*local).borrow();

        builder.assert_bool(local.is_routing);

        // Boolean limits on bits and selectors
        for i in 0..DIM {
            builder.assert_bool(local.current_bits[i]);
            builder.assert_bool(local.next_bits[i]);
            builder.assert_bool(local.selectors[i]);
        }

        // Only ONE dimension can flip (Topological Graph restriction)
        let mut sum_selectors = AB::Expr::zero();
        for i in 0..DIM {
            sum_selectors += local.selectors[i].into();
        }
        builder.when(local.is_routing).assert_one(sum_selectors);

        // Bit-Flip Equation: next = current + selector - 2 * current * selector
        let two = AB::Expr::from_canonical_usize(2);
        for i in 0..DIM {
            let bit = local.current_bits[i];
            let selector = local.selectors[i];
            let bit_flip: AB::Expr =
                bit.into() + selector.into() - two.clone() * bit.into() * selector.into();

            builder.when(local.is_routing).assert_eq(local.next_bits[i], bit_flip);
        }

        // Reconstruct composite node IDs
        let mut current_node = AB::Expr::zero();
        let mut next_node = AB::Expr::zero();
        for i in 0..DIM {
            let power = AB::Expr::from_canonical_u32(1 << i);
            current_node += local.current_bits[i].into() * power.clone();
            next_node += local.next_bits[i].into() * power;
        }

        // Receive syscall interaction, bridge EVM bounds
        builder.receive_syscall(
            local.clk_high,
            local.clk_low,
            AB::F::from_canonical_u32(SyscallCode::TOPOLOGICAL_ROUTE.syscall_id()),
            [current_node, next_node, AB::Expr::zero()],
            [AB::Expr::zero(), AB::Expr::zero(), AB::Expr::zero()],
            local.is_routing,
            InteractionScope::Local,
        );
    }
}
