use core::borrow::Borrow;
use p3_matrix::Matrix;
use slop_air::{Air, AirBuilder, BaseAir};
use slop_algebra::AbstractField;
use sp1_derive::AlignedBorrow;

#[derive(Debug, Clone, AlignedBorrow)]
#[repr(C)]
pub struct TopologyCols<T, const DIM: usize> {
    /// Clock cycle of the syscall (split into high and low parts)
    pub clk_high: T,
    pub clk_low: T,

    /// Is this a real routing operation or padding?
    pub is_routing: T,

    /// Dimensional Hypercube architecture
    pub current_bits: [T; DIM],
    pub selectors: [T; DIM],
    pub next_bits: [T; DIM],
}

pub const fn num_cols<const DIM: usize>() -> usize {
    core::mem::size_of::<TopologyCols<u8, DIM>>()
}

#[derive(Default)]
pub struct TopologicalRouterAir<const DIM: usize>;

impl<F, const DIM: usize> BaseAir<F> for TopologicalRouterAir<DIM> {
    fn width(&self) -> usize {
        num_cols::<DIM>()
    }
}

impl<AB: AirBuilder, const DIM: usize> Air<AB> for TopologicalRouterAir<DIM> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local = main.row_slice(0);
        let local: &TopologyCols<AB::Var, DIM> = (*local).borrow();

        builder.assert_bool(local.is_routing);

        // Boolean limits on bits and selectors
        for i in 0..DIM {
            builder.assert_bool(local.current_bits[i]);
            builder.assert_bool(local.next_bits[i]);
            builder.assert_bool(local.selectors[i]);
        }

        // Only ONE dimension can flip per hop
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
    }
}
