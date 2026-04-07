use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::AbstractField;
use p3_field::PrimeField32;
use p3_matrix::dense::RowMajorMatrix;
use p3_matrix::Matrix;
use rand::Rng;
use slop_baby_bear::BabyBear;

/// The number of dimensions in our hypercube.
const DIMS: usize = 10;

/// TopologicalRouterAir defines the constraints for a valid sequence of hops in a hypercube.
///
/// Columns:
/// - node_bits[DIMS]: The binary representation of the current node ID.
/// - selectors[DIMS]: Boolean flags indicating which bit is being flipped in this hop.
pub struct TopologicalRouterAir;

impl<F> BaseAir<F> for TopologicalRouterAir {
    fn width(&self) -> usize {
        DIMS * 2
    }
}

impl<AB: AirBuilder> Air<AB> for TopologicalRouterAir {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local = main.row_slice(0);
        let next = main.row_slice(1);

        let local_bits = &local[0..DIMS];
        let selectors = &local[DIMS..2 * DIMS];
        let next_bits = &next[0..DIMS];

        // 1. Selector Constraints: Each selector must be boolean (0 or 1)
        for selector in selectors.iter().take(DIMS) {
            builder.assert_bool((*selector).into());
        }

        // 2. Routing Constraint: Exactly one bit must be flipped per hop.
        let mut selector_sum = AB::Expr::zero();
        for selector in selectors.iter().take(DIMS) {
            selector_sum += (*selector).into();
        }
        builder.when_transition().assert_eq(selector_sum, AB::Expr::one());

        // 3. Transition Constraint: next_bit[i] = local_bit[i] XOR selector[i]
        // Algebraic XOR for boolean values: A + B - 2AB
        for i in 0..DIMS {
            let a: AB::Expr = local_bits[i].into();
            let b: AB::Expr = selectors[i].into();
            let xor_val = a.clone() + b.clone() - a * b * AB::F::from_canonical_u32(2);
            builder.when_transition().assert_eq(next_bits[i].into(), xor_val);
        }
    }
}

fn generate_trace<F: PrimeField32>(num_hops: usize) -> RowMajorMatrix<F> {
    let mut rng = rand::thread_rng();
    let mut trace = Vec::with_capacity(num_hops * DIMS * 2);

    let mut current_node: u32 = 0;

    for _ in 0..num_hops {
        let mut row = vec![F::zero(); DIMS * 2];

        // Fill node bits
        for (i, val) in row.iter_mut().enumerate().take(DIMS) {
            *val = F::from_canonical_u32((current_node >> i) & 1);
        }

        // Randomly pick exactly one bit to flip for the next hop
        let flip_idx = rng.gen_range(0..DIMS);
        row[DIMS + flip_idx] = F::one();

        trace.extend(row);
        current_node ^= 1 << flip_idx;
    }

    RowMajorMatrix::new(trace, DIMS * 2)
}

fn main() {
    // Setup logging
    tracing_subscriber::fmt::init();

    // Test parameters
    let log_n = 17; // 131,072 rows
    let num_rows = 1 << log_n;

    println!("--- Pure Plonky3 Topological Router Benchmark ---");
    println!("Dimensions: {}", DIMS);
    println!("Rows (Hops): {}", num_rows);

    // Generate Trace
    let now = std::time::Instant::now();
    let _trace = generate_trace::<BabyBear>(num_rows);
    println!("Trace generation took: {:?}", now.elapsed());

    // -- Setup Prover Config --
    if std::env::args().any(|arg| arg == "--prove") {
        println!("--- Setting up Pure Plonky3 STARK Configuration ---");

        use p3_blake3::Blake3;
        use p3_dft::Radix2Bowers;
        use p3_fri::{FriConfig, TwoAdicFriPcs};
        use p3_symmetric::CompressionFunctionFromHasher;
        use slop_challenger::DuplexChallenger;
        use slop_uni_stark::StarkConfig;

        type Hasher = Blake3;
        type Compress = CompressionFunctionFromHasher<Hasher, 2, 2>;
        type Dft = Radix2Bowers;

        let fri_config = FriConfig {
            log_blowup: 1,
            num_queries: 100,
            proof_of_work_bits: 16,
            mmcs: p3_commit::ExtensionMmcs::new(Hasher::new()),
        };

        let pcs = TwoAdicFriPcs::new(fri_config, Dft::default());
        let config = StarkConfig::new(pcs, DuplexChallenger::new(), Hasher::new());

        let mut challenger = config.challenger();
        let air = TopologicalRouterAir;

        println!("--- Generating STARK Proof ---");
        let prove_start = std::time::Instant::now();

        let _proof = slop_uni_stark::prove(&config, &air, &mut challenger, trace, &vec![]);

        println!("STARK Proving Time: {:?}", prove_start.elapsed());
    } else {
        println!("(Skipping full STARK proof for now. Run with `cargo run --release -p hypercube-pure-plonky3 -- --prove` to execute it.)");
        println!("Execution trace of 131,072 hops generated successfully in memory.");
        println!("This trace is Degree-2 and uses only {} columns.", DIMS * 2);
        println!("RAM usage for this trace: ~{} MB", (num_rows * DIMS * 2 * 4) / 1024 / 1024);
    }
}
