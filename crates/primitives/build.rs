use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=SP1_TOPOLOGY_DIM");

    // Default to 40 if the environment variable is not provided.
    let dim_str = env::var("SP1_TOPOLOGY_DIM").unwrap_or_else(|_| "40".to_string());
    let dim_val: usize = dim_str.parse().expect("SP1_TOPOLOGY_DIM must be a valid usize");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("topology_dim.rs");

    let content = format!("pub const TOPOLOGY_DIM: usize = {};\n", dim_val);
    fs::write(&dest_path, content).unwrap();
}
