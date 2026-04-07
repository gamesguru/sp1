use sp1_sdk::prelude::*;
use sp1_sdk::ProverClient;

/// The ELF we want to execute inside the zkVM.
const ELF: Elf = include_elf!("topology-program");

#[tokio::main]
async fn main() {
    // Setup logging.
    sp1_sdk::utils::setup_logger();

    // Define a valid 1-bit hop in a 10-D hypercube.
    // Node 0 (0000000000) to Node 1 (0000000001).
    let curr = 0u32;
    let next = 1u32;

    let mut stdin = SP1Stdin::new();
    stdin.write(&curr);
    stdin.write(&next);

    // Create a `ProverClient`.
    let client = ProverClient::from_env().await;

    // Execute the program.
    let (mut public_values, report) = client.execute(ELF, stdin.clone()).await.unwrap();
    println!("executed program with {} cycles", report.total_instruction_count());

    // Verify the output matches our inputs.
    let got_curr = public_values.read::<u32>();
    let got_next = public_values.read::<u32>();

    assert_eq!(curr, got_curr);
    assert_eq!(next, got_next);

    println!("Public values verified: {} -> {}", got_curr, got_next);

    // Generate and verify a core proof.
    println!("Setting up PK/VK and generating proof...");
    let pk = client.setup(ELF).await.unwrap();
    let proof = client.prove(&pk, stdin).core().await.unwrap();

    client.verify(&proof, pk.verifying_key(), None).expect("Verification failed");

    println!("successfully generated and verified proof for the topology program!")
}
