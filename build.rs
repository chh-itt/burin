fn main() {
    // Explicit rerun-if-changed prevents stray files (debug logs, temp
    // files, etc.) from triggering spurious rebuilds.
    // Without these, Cargo watches the *entire* package directory.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=auralis/");
    println!("cargo:rerun-if-changed=burin-platform/");
    println!("cargo:rerun-if-changed=examples/");

    #[cfg(feature = "built-info")]
    {
        built::write_built_file().expect("Failed to acquire build-time information");
    }
}
