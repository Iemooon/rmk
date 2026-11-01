//! Build dependency for Nordic-only Gazell receivers. Does not initialize radio hardware.
use std::{env, path::PathBuf};

/// Link the existing SDK 17.1.1 archive for nRF52833/52840, or nrfxlib 2.9.0
/// soft-float archive for nRF52820. Call once from the firmware build.rs.
/// Application code still owns FFI, interrupt/glue handlers and radio configuration.
/// GZLL_DIR (relative to the application) and GZLL_LIB override the defaults.
pub fn link(chip: &str) {
    let (subdir, default_lib) = match chip {
        "nrf52833" | "nrf52840" => ("", "gzll_nrf52840_gcc.a"),
        "nrf52820" => ("nrfxlib-52820", "libgzll.a"),
        _ => panic!("Unsupported Gazell chip: {chip}"),
    };
    let lib = env::var("GZLL_LIB").unwrap_or_else(|_| default_lib.into());
    let directory = match env::var("GZLL_DIR") {
        Ok(value) => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo manifest directory")).join(path)
            }
        }
        Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/gzll")
            .join(subdir),
    };
    let archive = directory.join(&lib);
    assert!(archive.is_file(), "Gazell archive missing: {}", archive.display());
    println!("cargo:rustc-link-search=native={}", directory.display());
    println!("cargo:rustc-link-lib=static:+verbatim={lib}");
    println!("cargo:rerun-if-changed={}", archive.display());
    println!("cargo:rerun-if-env-changed=GZLL_DIR");
    println!("cargo:rerun-if-env-changed=GZLL_LIB");
}
