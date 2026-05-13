//! Build script: generate Rust gRPC client code from the vendored `.proto`
//! files in `proto/`.
//!
//! Outputs land in `OUT_DIR` and are included via `tonic::include_proto!` in
//! `src/lib.rs`. We deliberately generate *only* the client side here — the
//! server lives in the separate `wishd` workspace.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("proto");

    // Re-run if any proto changes.
    println!("cargo:rerun-if-changed={}", proto_root.display());
    for entry in std::fs::read_dir(&proto_root)? {
        let entry = entry?;
        if entry.path().extension().and_then(|e| e.to_str()) == Some("proto") {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }

    // tonic 0.14 moved prost integration to the separate
    // `tonic-prost-build` crate. The `Config::new()`/`configure()` API
    // now lives there; `tonic_build` itself only ships the service
    // codegen trait.
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&[proto_root.join("health.proto")], &[proto_root])?;
    Ok(())
}
