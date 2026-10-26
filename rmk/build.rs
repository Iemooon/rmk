#[path = "./build_common.rs"]
mod common;

fn main() {
    let mut cfgs = common::CfgSet::new();
    common::set_target_cfgs(&mut cfgs);

    // `RMK_COMMIT` and `RMK_FEATURES` go into the storage schema hash.
    // When either changes, the storage is erased.
    let commit = include_str!("../STORAGE-BASE-COMMIT").trim();
    println!("cargo:rustc-env=RMK_COMMIT={commit}");

    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_lowercase))
        .collect();
    features.sort();
    println!("cargo:rustc-env=RMK_FEATURES={}", features.join(","));
}
