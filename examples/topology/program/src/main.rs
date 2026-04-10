//! A program that demonstrates the use of the `TOPOLOGICAL_ROUTE` precompile.

#![no_main]
sp1_zkvm::entrypoint!(main);

pub fn main() {
    // Read the current and next node IDs from the input.
    let curr = sp1_zkvm::io::read::<u32>();
    let next = sp1_zkvm::io::read::<u32>();

    // Execute the precompile to validate the hop.
    // This is much more efficient than doing the XOR and count_ones in Rust logic.
    sp1_zkvm::syscalls::syscall_topological_route(curr, next);

    // Commit the nodes to public input to verify the work was done.
    sp1_zkvm::io::commit(&curr);
    sp1_zkvm::io::commit(&next);
}
