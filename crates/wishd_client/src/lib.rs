//! Rust client for the `wishd` local daemon.
//!
//! `wishd` is a sibling product to `wish` — a trusted Rust daemon that
//! listens on a Unix-domain socket and exposes privileged operations
//! (fs, git, process, terminal, index, capability, cell-verify) over
//! gRPC. See `/Users/wenyan/ClaudeProjects/wishd` for the daemon source.
//!
//! This crate is a thin client wrapper: tonic-generated proto types
//! plus a `WishdConnection` helper that owns the Unix-socket transport
//! and exposes typed RPC clients to the rest of the wish app.
//!
//! ## What's wired today (slice 1)
//!
//! - `health` service: probe whether wishd is up and which components
//!   are healthy. This is the bootstrap dependency for everything else —
//!   on wish startup, ping wishd's health endpoint; if it's down, the
//!   user gets a one-line "wishd is not running" toast with a
//!   "launch / install" affordance.
//!
//! ## What's coming next
//!
//! Subsequent slices vendor and codegen the rest of wishd's protos:
//! `auth`, `capability`, `cell_verify`, `fs`, `git`, `index`, `process`,
//! `terminal`. Each slice typically: vendor the .proto file, add it to
//! `build.rs`, expose the generated client as a re-export in this lib,
//! migrate one in-process wish singleton to call the new client behind a
//! `FeatureFlag::WishdBacked{Foo}` flag.
//!
//! ## Socket location
//!
//! The conventional path is `${WISH_RUNTIME_DIR}/wishd.sock`, defaulting
//! to `$HOME/.wish/wishd.sock` on macOS/Linux. `WishdConnection` knows
//! how to resolve this; callers should never hard-code paths.

use std::path::PathBuf;

/// Health-service generated client + types. Re-exported so callers say
/// `wishd_client::health::HealthServiceClient` instead of digging into
/// the `wishd.health.v1` proto package path.
pub mod health {
    tonic::include_proto!("wishd.health.v1");
}

/// Default Unix-domain socket path for talking to wishd, honoring the
/// `WISH_RUNTIME_DIR` env var when set so tests / sandboxed builds can
/// point at a fixture socket without touching the real one.
///
/// Returns `None` only when neither `WISH_RUNTIME_DIR` nor `HOME` is
/// resolvable — extremely unusual on a real desktop, so callers can
/// treat `None` as "the environment is broken; refuse to connect."
pub fn default_socket_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("WISH_RUNTIME_DIR") {
        return Some(PathBuf::from(dir).join("wishd.sock"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".wish").join("wishd.sock"))
}

/// Convenience constants so callers don't sprinkle string literals.
pub mod paths {
    /// Subdirectory of `$HOME` (or `WISH_RUNTIME_DIR`'s parent) used as
    /// wishd's runtime root.
    pub const RUNTIME_DIR_NAME: &str = ".wish";

    /// Unix-domain socket filename within the runtime dir.
    pub const SOCKET_FILE_NAME: &str = "wishd.sock";

    /// Environment variable overriding `RUNTIME_DIR_NAME` lookup. Set by
    /// tests / dev tooling.
    pub const RUNTIME_DIR_ENV: &str = "WISH_RUNTIME_DIR";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_env_var<T>(key: &str, value: Option<&str>, f: impl FnOnce() -> T) -> T {
        // Tests in this module are marked `serial` because they mutate
        // process-wide env vars. Snapshot and restore around the closure.
        let previous = std::env::var_os(key);
        match value {
            // Safety: tests in this module run serially so no concurrent reader.
            Some(v) => unsafe { std::env::set_var(key, v) },
            // Safety: same as above.
            None => unsafe { std::env::remove_var(key) },
        }
        let result = f();
        // Safety: restoring previous state.
        match previous {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
        result
    }

    #[test]
    #[serial_test::serial]
    fn default_socket_path_honors_wish_runtime_dir() {
        let path = with_env_var(paths::RUNTIME_DIR_ENV, Some("/tmp/wish-fixture"), || {
            default_socket_path()
        })
        .expect("socket path");
        assert_eq!(path, PathBuf::from("/tmp/wish-fixture/wishd.sock"));
    }

    #[test]
    #[serial_test::serial]
    fn default_socket_path_falls_back_to_home() {
        // Drop WISH_RUNTIME_DIR, set HOME to a known fixture.
        let path = with_env_var(paths::RUNTIME_DIR_ENV, None, || {
            with_env_var("HOME", Some("/tmp/fakehome"), default_socket_path)
        })
        .expect("socket path");
        assert_eq!(path, PathBuf::from("/tmp/fakehome/.wish/wishd.sock"));
    }

    #[test]
    fn health_check_request_round_trips_component() {
        // Quick smoke test: tonic-build emitted the message types, and they
        // round-trip via prost serialization.
        let req = health::HealthCheckRequest {
            component: "git".to_string(),
        };
        let mut buf = Vec::new();
        prost::Message::encode(&req, &mut buf).expect("encode");
        let decoded: health::HealthCheckRequest =
            prost::Message::decode(buf.as_slice()).expect("decode");
        assert_eq!(decoded.component, "git");
    }

    #[test]
    fn health_check_response_serving_status_round_trips() {
        let res = health::HealthCheckResponse {
            status: health::health_check_response::ServingStatus::Serving as i32,
            detail: "ok".to_string(),
        };
        let mut buf = Vec::new();
        prost::Message::encode(&res, &mut buf).expect("encode");
        let decoded: health::HealthCheckResponse =
            prost::Message::decode(buf.as_slice()).expect("decode");
        assert_eq!(decoded.status, 1);
        assert_eq!(decoded.detail, "ok");
    }

    #[test]
    fn paths_constants_are_stable() {
        // Tripwire: changing these constants is a breaking change for any
        // user who has shipped a wishd.sock at the current path. Failing
        // this test on a future refactor forces a deliberate decision +
        // migration plan.
        assert_eq!(paths::RUNTIME_DIR_NAME, ".wish");
        assert_eq!(paths::SOCKET_FILE_NAME, "wishd.sock");
        assert_eq!(paths::RUNTIME_DIR_ENV, "WISH_RUNTIME_DIR");
    }
}
