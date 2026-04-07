use p3_matrix::dense::RowMajorMatrix;
use p3_matrix::Matrix;
use rand::Rng;
use slop_algebra::{AbstractField, PrimeField32};
use slop_koala_bear::KoalaBear;
use sp1_topology::DIM;

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
    tracing_subscriber::fmt::init();

    let log_n = 17; // 131,072 rows
    let num_rows = 1 << log_n;

    println!("--- Pure Plonky3 Topological Router Benchmark ---");
    println!("Hypercube dimensions: {}", DIM);
    println!("Rows (Hops): {}", num_rows);

    // Generate Trace
    let now = std::time::Instant::now();
    let trace: RowMajorMatrix<KoalaBear> = generate_trace(num_rows);
    println!("Trace generation took: {:?}", now.elapsed());

    println!("--- Evaluating Constraints over Execution Trace ---");
    let eval_start = std::time::Instant::now();

    // We execute the explicit Topological constraints natively to benchmark their mathematical execution cost.
    let mut constraint_violations = 0;
    for i in 0..num_rows - 1 {
        let local = trace.row_slice(i);
        let next = trace.row_slice(i + 1);

        let mut selector_sum = KoalaBear::zero();
        for d in 0..DIM {
            let bit = local[d];
            let selector = local[DIM + d];

            // Boolean constraint on selectors
            let bool_val = selector * (selector - KoalaBear::one());
            if bool_val != KoalaBear::zero() {
                constraint_violations += 1;
            }

            selector_sum += selector;

            // XOR Transition constraint: s_next == s_local + selector - 2 * s_local * selector
            let bit_flip = bit + selector - KoalaBear::from_canonical_u32(2) * bit * selector;
            if next[d] != bit_flip {
                constraint_violations += 1;
            }
        }

        if selector_sum != KoalaBear::one() {
            constraint_violations += 1;
        }
    }

    // Use `trace` to prevent compiler warnings about unused variables and to prove memory pinning
    std::hint::black_box(&trace);

    println!("Constraint Evaluation Time ({} rows): {:?}", num_rows, eval_start.elapsed());
    assert_eq!(constraint_violations, 0, "Trace contains constraint violations!");

    println!("Execution trace generated and constraint-verified successfully.");
    println!("RAM usage: {:.1} MB", (num_rows as f64 * DIM as f64 * 2.0 * 4.0) / 1024.0 / 1024.0);

    // -- STARK Proof Generation --
    if std::env::args().any(|arg| arg == "--prove") {
        println!("--- Setting up Pure Plonky3 STARK Configuration ---");

        use p3_dft::Radix2Bowers;
        use p3_fri::{FriConfig, TwoAdicFriPcs};
        use p3_merkle_tree::FieldMerkleTreeMmcs;
        use slop_challenger::DuplexChallenger;
        use slop_uni_stark::StarkConfig;
        use sp1_primitives::poseidon2_init;
        use sp1_topology::TopologicalRouterAir;

        // Standard SP1 KoalaBear STARK parameters
        type Val = KoalaBear;
        let perm = poseidon2_init();
        let mmcs = FieldMerkleTreeMmcs::<Val, Val, _, 8>::new(perm.clone());
        let fri_config = FriConfig {
            log_blowup: 1,
            num_queries: 100,
            proof_of_work_bits: 16,
            mmcs: mmcs.clone(),
        };

        let pcs = TwoAdicFriPcs::new(1, Radix2Bowers::default(), mmcs, fri_config);
        let config = StarkConfig::new(pcs);
        let mut challenger = DuplexChallenger::new(perm);
        let air = TopologicalRouterAir;

        println!("--- Generating STARK Proof ---");
        let prove_start = std::time::Instant::now();

        // Pass trace into the actual STARK prover
        let proof = slop_uni_stark::prove(&config, &air, &mut challenger, trace, &mut vec![]);

        println!("STARK Proved length: {} bytes", bincode::serialize(&proof).unwrap().len());
        println!("STARK Proving Time: {:?}", prove_start.elapsed());
    }
}
