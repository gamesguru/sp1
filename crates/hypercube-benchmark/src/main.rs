use p3_matrix::dense::RowMajorMatrix;
use p3_matrix::Matrix;
use rand::Rng;
use slop_algebra::{AbstractField, PrimeField32};
use sp1_primitives::SP1Field;
use sp1_topology::DIM;

use std::time::Instant;
use tracing::{info, warn};

fn generate_trace<F: PrimeField32>(num_hops: usize) -> RowMajorMatrix<F> {
    let mut rng = rand::thread_rng();
    let mut trace = Vec::with_capacity(num_hops * DIM * 2);

    let mut current_node: u32 = 0;

    for _ in 0..num_hops {
        let mut row = vec![F::zero(); DIM * 2];

        // Fill node bits
        for (i, item) in row.iter_mut().enumerate().take(DIM) {
            *item = F::from_canonical_u32((current_node >> i) & 1);
        }

        // Randomly pick exactly one bit to flip for the next hop
        let flip_idx = rng.gen_range(0..DIM);
        row[DIM + flip_idx] = F::one();

        trace.extend(row);
        current_node ^= 1 << flip_idx;
    }

    RowMajorMatrix::new(trace, DIM * 2)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let log_n = 17; // 131,072 rows
    let num_rows = 1 << log_n;

    info!("┌────────────────────────────────────────────────────────┐");
    info!("│      TOPOLOGICAL ROUTER HYPERCUBE BENCHMARK          │");
    info!("└────────────────────────────────────────────────────────┘");
    info!("Hypercube dimensions: {}", DIM);
    info!("Rows (Hops): {}", num_rows);

    // Generate Trace
    let now = Instant::now();
    let trace: RowMajorMatrix<SP1Field> = generate_trace(num_rows);
    info!("✓ Trace generation took: {:?}", now.elapsed());

    info!("--- [Constraints] Evaluating ---");
    let eval_start = Instant::now();

    let mut constraint_violations = 0;
    (0..num_rows - 1).for_each(|i| {
        let local = trace.row_slice(i);
        let next = trace.row_slice(i + 1);

        let mut selector_sum = SP1Field::zero();
        (0..DIM).for_each(|d| {
            let bit = local[d];
            let selector = local[DIM + d];

            // Boolean constraint on selectors: s * (s - 1) == 0
            let bool_val = selector * (selector - SP1Field::one());
            if bool_val != SP1Field::zero() {
                constraint_violations += 1;
            }

            selector_sum += selector;

            // XOR Transition constraint: s_next == s_local + selector - 2 * s_local * selector
            let bit_flip = bit + selector - SP1Field::from_canonical_u32(2) * bit * selector;
            if next[d] != bit_flip {
                constraint_violations += 1;
            }
        });

        if selector_sum != SP1Field::one() {
            constraint_violations += 1;
        }
    });

    // Use `trace` to prevent compiler warnings about unused variables and to prove memory pinning
    std::hint::black_box(&trace);

    info!("✓ Constraints verified ({} rows) in: {:?}", num_rows, eval_start.elapsed());
    if constraint_violations > 0 {
        warn!("✗ FOUND {} CONSTRAINT VIOLATIONS", constraint_violations);
        std::process::exit(1);
    }

    info!(
        "✓ Memory foot-print: {:.1} MB",
        (num_rows as f64 * DIM as f64 * 2.0 * 4.0) / 1024.0 / 1024.0
    );

    // -- STARK Proof Generation --
    if std::env::args().any(|arg| arg == "--prove") {
        info!("--- [Config] Setting up STARK ---");

        use p3_dft::Radix2Bowers;
        use p3_fri::{FriConfig, TwoAdicFriPcs};
        use p3_merkle_tree::FieldMerkleTreeMmcs;
        use slop_challenger::DuplexChallenger;
        use slop_symmetric::{PaddingFreeSponge, TruncatedPermutation};
        use slop_uni_stark::StarkConfig;
        use sp1_primitives::poseidon2_init;
        use sp1_topology::TopologicalRouterAir;

        // Standard SP1 STARK parameters
        type Val = SP1Field;
        let perm = poseidon2_init();
        let hasher = PaddingFreeSponge::<_, 16, 8, 8>::new(perm.clone());
        let compressor = TruncatedPermutation::<_, 2, 8, 16>::new(perm.clone());
        let mmcs = FieldMerkleTreeMmcs::<Val, Val, _, _, 8>::new(hasher, compressor);
        let fri_config = FriConfig {
            log_blowup: 1,
            num_queries: 100,
            proof_of_work_bits: 16,
            mmcs: mmcs.clone(),
        };

        let pcs = TwoAdicFriPcs::new(1, Radix2Bowers, mmcs, fri_config);
        let config = StarkConfig::new(pcs);
        let mut challenger = DuplexChallenger::<Val, _, 16, 8>::new(perm);
        let air = TopologicalRouterAir;

        info!("--- [Proving] Generating STARK Proof ---");
        let prove_start = Instant::now();
        let proof = slop_uni_stark::prove(&config, &air, &mut challenger, trace, &mut vec![]);
        let duration = prove_start.elapsed();

        info!("┌────────────────────────────────────────────────────────┐");
        info!("│                PERFORMANCE REPORT                      │");
        info!("├────────────────────────────────────────────────────────┤");
        info!(
            "│ STARK Proof Size: {:>10} bytes               │",
            bincode::serialize(&proof).unwrap().len()
        );
        info!("│ STARK Proving Time: {:>10.2?}                     │", duration);
        info!("└────────────────────────────────────────────────────────┘");
    }
}
