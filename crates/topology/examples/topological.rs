use core::borrow::Borrow;
use p3_matrix::dense::RowMajorMatrix;
use p3_matrix::Matrix;
use serde_json::Value;
use sha2::{Digest, Sha256};
use slop_algebra::{AbstractField, PrimeField32};
use sp1_primitives::SP1Field;
use sp1_topology::{num_cols, TopologyCols};

use std::fs;
use std::time::Instant;
use tracing::{info, warn};

fn event_to_coordinate<const DIM: usize>(event_id: &str) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(event_id.as_bytes());
    let hash_bytes = hasher.finalize();
    // Take first 8 bytes, convert to u64, mask to DIM
    let val = u64::from_be_bytes([
        hash_bytes[0],
        hash_bytes[1],
        hash_bytes[2],
        hash_bytes[3],
        hash_bytes[4],
        hash_bytes[5],
        hash_bytes[6],
        hash_bytes[7],
    ]);

    if DIM >= 64 {
        val
    } else {
        val & ((1u64 << DIM) - 1)
    }
}

fn find_json() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let root_dir = std::path::Path::new(manifest_dir).join("../../..");

    let paths = [
        root_dir.join("res/massive_matrix_state.json"),
        root_dir.join("res/real_matrix_state.json"),
        root_dir.join("res/real_5k.json"),
    ];

    for p in &paths {
        if p.exists() {
            return p.to_string_lossy().into_owned();
        }
    }

    panic!(
        "Could not find Matrix state JSON fixtures in the res/ directory. Expected them at {:?}",
        root_dir.join("res/")
    );
}

fn generate_trace<F: PrimeField32, const DIM: usize>(
    json_path: &str,
    max_events: usize,
) -> RowMajorMatrix<F> {
    let file_data = fs::read_to_string(json_path).expect("Failed to read JSON path");
    let mut events: Vec<Value> = serde_json::from_str(&file_data).expect("Invalid JSON");

    if events.len() > max_events {
        events.truncate(max_events);
    }

    info!("Ingested {} synthetic Matrix events. Routing over {}-D Hypercube...", events.len(), DIM);

    let mut trace = Vec::new();
    let mut current_node: u64 = 0;

    if !events.is_empty() {
        let first_id = events[0]["event_id"].as_str().unwrap_or("");
        current_node = event_to_coordinate::<DIM>(first_id);
    }

    let mut clock = 0;
    let n_cols = num_cols::<DIM>();

    for event in events.iter().skip(1) {
        let event_id = event["event_id"].as_str().unwrap_or("");
        if event_id.is_empty() {
            continue;
        }
        let target_coord = event_to_coordinate::<DIM>(event_id);

        while current_node != target_coord {
            let diff = current_node ^ target_coord;
            let bit_to_flip = diff.trailing_zeros() as usize;
            let next_node = current_node ^ (1u64 << bit_to_flip);

            let mut row = vec![F::zero(); n_cols];
            {
                let cols: &mut TopologyCols<F, DIM> =
                    unsafe { &mut *(row.as_mut_ptr() as *mut TopologyCols<F, DIM>) };
                cols.is_routing = F::one();
                cols.clk_low = F::from_canonical_usize(clock);

                for j in 0..DIM {
                    cols.current_bits[j] = F::from_canonical_u32(((current_node >> j) & 1) as u32);
                }

                cols.selectors[bit_to_flip] = F::one();

                for j in 0..DIM {
                    let bit = cols.current_bits[j];
                    let selector = cols.selectors[j];
                    cols.next_bits[j] = bit + selector - F::from_canonical_u32(2) * bit * selector;
                }
            }
            trace.extend(row);
            current_node = next_node;
            clock += 1;
        }
    }

    if trace.is_empty() {
        let mut row = vec![F::zero(); n_cols];
        {
            let cols: &mut TopologyCols<F, DIM> =
                unsafe { &mut *(row.as_mut_ptr() as *mut TopologyCols<F, DIM>) };
            cols.is_routing = F::zero();
            cols.clk_low = F::from_canonical_usize(clock);
        }
        trace.extend(row);
    }

    let num_rows = trace.len() / n_cols;
    let padded_rows = num_rows.next_power_of_two();
    if padded_rows > num_rows {
        for _ in num_rows..padded_rows {
            let mut row = vec![F::zero(); n_cols];
            {
                let cols: &mut TopologyCols<F, DIM> =
                    unsafe { &mut *(row.as_mut_ptr() as *mut TopologyCols<F, DIM>) };
                cols.is_routing = F::zero(); // padding
                cols.clk_low = F::from_canonical_usize(clock);

                for j in 0..DIM {
                    cols.current_bits[j] = F::from_canonical_u32(((current_node >> j) & 1) as u32);
                    cols.next_bits[j] = cols.current_bits[j];
                }
            }
            trace.extend(row);
            clock += 1;
        }
    }

    info!("Trace padded to {} rows (Power of 2).", padded_rows);
    RowMajorMatrix::new(trace, n_cols)
}

fn run_benchmark<const DIM: usize>(prove: bool, max_events: usize) {
    info!("┌────────────────────────────────────────────────────────┐");
    info!("│      TOPOLOGICAL ROUTER HYPERCUBE BENCHMARK            │");
    info!("└────────────────────────────────────────────────────────┘");
    info!("Hypercube dimensions: {}", DIM);

    let json_path = find_json();

    // Generate Trace
    let now = Instant::now();
    let trace: RowMajorMatrix<SP1Field> = generate_trace::<SP1Field, DIM>(&json_path, max_events);
    let num_rows = trace.height();
    let log_n = trace.height().trailing_zeros() as usize;

    info!("✓ Trace generation took: {:?}", now.elapsed());

    info!("--- [Constraints] Evaluating ---");
    let eval_start = Instant::now();

    let mut constraint_violations = 0;
    (0..num_rows - 1).for_each(|i| {
        let local_slice = trace.row_slice(i);
        let next_slice = trace.row_slice(i + 1);

        let local: &TopologyCols<SP1Field, DIM> = (*local_slice).borrow();
        let next: &TopologyCols<SP1Field, DIM> = (*next_slice).borrow();

        let is_routing = local.is_routing;

        let mut selector_sum = SP1Field::zero();
        (0..DIM).for_each(|d| {
            let bit = local.current_bits[d];
            let selector = local.selectors[d];

            let bool_val = selector * (selector - SP1Field::one());
            if bool_val != SP1Field::zero() {
                constraint_violations += 1;
            }

            selector_sum += selector;

            if is_routing == SP1Field::one() {
                let bit_flip = bit + selector - SP1Field::from_canonical_u32(2) * bit * selector;
                if next.current_bits[d] != bit_flip {
                    constraint_violations += 1;
                }
            }
        });

        if is_routing == SP1Field::one() && selector_sum != SP1Field::one() {
            constraint_violations += 1;
        }
    });

    std::hint::black_box(&trace);

    info!("✓ Constraints verified ({} rows) in: {:?}", num_rows, eval_start.elapsed());
    if constraint_violations > 0 {
        warn!("✗ FOUND {} CONSTRAINT VIOLATIONS", constraint_violations);
        std::process::exit(1);
    }

    let n_cols = num_cols::<DIM>();
    info!(
        "✓ Memory foot-print: {:.1} MB",
        (num_rows as f64 * n_cols as f64 * 4.0) / 1024.0 / 1024.0
    );

    if prove {
        info!("--- [Config] Setting up STARK ---");

        use p3_dft::Radix2Bowers;
        use p3_fri::{FriConfig, TwoAdicFriPcs};
        use p3_merkle_tree::FieldMerkleTreeMmcs;
        use slop_challenger::DuplexChallenger;
        use slop_symmetric::{PaddingFreeSponge, TruncatedPermutation};
        use slop_uni_stark::StarkConfig;
        use sp1_primitives::poseidon2_init;
        use sp1_topology::TopologicalRouterAir;

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

        let pcs = TwoAdicFriPcs::new(log_n, Radix2Bowers, mmcs, fri_config);
        let config = StarkConfig::new(pcs);
        let mut challenger = DuplexChallenger::<Val, _, 16, 8>::new(perm);
        let air = TopologicalRouterAir::<DIM>;

        info!("--- [Proving] Generating STARK Proof ---");
        let prove_start = Instant::now();
        let proof = slop_uni_stark::prove(&config, &air, &mut challenger, trace, &mut vec![]);
        let duration = prove_start.elapsed();

        info!("┌────────────────────────────────────────────────────────┐");
        info!("│                PERFORMANCE REPORT                      │");
        info!("├────────────────────────────────────────────────────────┤");
        let size_msg =
            format!(" STARK Proof Size: {:>10} bytes", bincode::serialize(&proof).unwrap().len());
        info!("│{:<56}│", size_msg);
        let time_msg = format!(" STARK Proving Time: {:>10.2?}", duration);
        info!("│{:<56}│", time_msg);
        info!("└────────────────────────────────────────────────────────┘");
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let mut dim = 10;
    let mut prove = false;
    let mut max_events = 100_000;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--dim" && i + 1 < args.len() {
            dim = args[i + 1].parse().expect("Invalid --dim value");
            i += 2;
        } else if args[i] == "--events" && i + 1 < args.len() {
            max_events = args[i + 1].parse().expect("Invalid --events value");
            i += 2;
        } else if args[i] == "--prove" {
            prove = true;
            i += 1;
        } else {
            i += 1;
        }
    }

    match dim {
        10 => run_benchmark::<10>(prove, max_events),
        11 => run_benchmark::<11>(prove, max_events),
        12 => run_benchmark::<12>(prove, max_events),
        20 => run_benchmark::<20>(prove, max_events),
        30 => run_benchmark::<30>(prove, max_events),
        40 => run_benchmark::<40>(prove, max_events),
        _ => {
            warn!(
                "Unsupported dimensionality --dim {}. Valid options are: 10, 11, 12, 20, 30, 40.",
                dim
            );
            std::process::exit(1);
        }
    }
}
