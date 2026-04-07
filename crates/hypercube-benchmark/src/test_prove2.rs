pub fn get_prover_config() {
    use slop_baby_bear::BabyBear;
    use p3_dft::Radix2Bowers;
    use p3_fri::{FriConfig, TwoAdicFriPcs};
    use slop_challenger::DuplexChallenger;
    use slop_uni_stark::StarkConfig;
    
    use slop_baby_bear::baby_bear_poseidon2::{my_bb_16_perm, Perm};
    use slop_symmetric::{PaddingFreeSponge, TruncatedPermutation};
    
    type Val = BabyBear;
    type Challenge = slop_algebra::extension::BinomialExtensionField<Val, 4>;
    type Hasher = PaddingFreeSponge<Perm, 16, 8, 8>;
    type Compress = TruncatedPermutation<Perm, 2, 8, 16>;
    type Dft = Radix2Bowers;

    let perm = my_bb_16_perm();
    let hasher = Hasher::new(perm.clone());
    let compress = Compress::new(perm.clone());
    
    let mmcs = p3_commit::ExtensionMmcs::<Val, Val, Challenge, Hasher, Compress>::new(
        hasher, compress,
    );

    let fri_config = FriConfig {
        log_blowup: 1,
        num_queries: 100,
        proof_of_work_bits: 16,
        mmcs: mmcs.clone(),
    };

    let pcs = TwoAdicFriPcs::new(1, Dft::default(), mmcs, fri_config);
    let config = StarkConfig::new(pcs);
    let mut challenger = DuplexChallenger::new(perm.clone());
}
