//! Wish Cell Runtime — manifest, capability gate, signature verification.
//!
//! v0.5.0 ships the **manifest type + capability gate + signature
//! schema**. The wasm sandbox itself lands in v0.8.0
//! (`wish-design/wish-plan-20260514/03-crates/10-wish-cells.md`).
//!
//! The point of shipping this crate now: any other crate (especially
//! `wish-cell-forge`) can already code against the manifest format.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Trust tier of a cell.
///
/// See `wish-design/wish-plan-20260514/07-cells-and-governance/00-cell-architecture.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    /// Hermon-verified, security-reviewed.
    HermonVerified,
    /// Signed by your team key.
    TeamVerified,
    /// Local developer cell, no distribution.
    UserSigned,
    /// Third-party signed; user approval required per install.
    ThirdParty,
    /// Anonymous / unsigned — not loaded unless explicit.
    Untrusted,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub fs_read: Vec<String>,
    #[serde(default)]
    pub fs_write: Vec<String>,
    #[serde(default)]
    pub net_fetch: bool,
    #[serde(default)]
    pub model_invoke: bool,
    #[serde(default)]
    pub world_read: bool,
    #[serde(default)]
    pub world_patch: bool,
    #[serde(default)]
    pub finance_read: bool,
    /// Even if true, finance writes must still pass the OpeniBank 5-gate.
    #[serde(default)]
    pub finance_transact: bool,
}

impl Capabilities {
    /// Cell A is a subset of Cell B's capabilities? Used to enforce
    /// "child cannot exceed parent" rules.
    pub fn is_subset_of(&self, other: &Capabilities) -> bool {
        let scope_subset =
            |a: &[String], b: &[String]| a.iter().all(|p| b.iter().any(|q| p.starts_with(q)));
        scope_subset(&self.fs_read, &other.fs_read)
            && scope_subset(&self.fs_write, &other.fs_write)
            && (!self.net_fetch || other.net_fetch)
            && (!self.model_invoke || other.model_invoke)
            && (!self.world_read || other.world_read)
            && (!self.world_patch || other.world_patch)
            && (!self.finance_read || other.finance_read)
            && (!self.finance_transact || other.finance_transact)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub created_at: DateTime<Utc>,
    pub trust_tier: TrustTier,
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Logical wasm entry. `none` for stub cells that only declare a
    /// capability surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_entry: Option<String>,
    #[serde(default)]
    pub events_in: Vec<String>,
    #[serde(default)]
    pub events_out: Vec<String>,
    /// Ed25519-style signature; `None` means unsigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl CellManifest {
    /// SHA-256 over the manifest content (signature field excluded).
    /// This is what a signer signs.
    pub fn content_hash_hex(&self) -> String {
        let mut copy = self.clone();
        copy.signature = None;
        let json = serde_json::to_vec(&copy).expect("serialize manifest");
        let mut h = Sha256::new();
        h.update(&json);
        hex_lower(&h.finalize())
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xF) as usize] as char);
    }
    s
}

#[derive(Debug, Error)]
pub enum CellError {
    #[error("capability denied: {0}")]
    CapabilityDenied(String),
    #[error("untrusted cell: {0}")]
    Untrusted(String),
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
}

/// Decide whether a cell is loadable under a per-user policy.
pub struct CellGate {
    pub minimum_tier: TrustTier,
    /// If true, refuse to load any cell whose signature is None.
    pub require_signature: bool,
}

impl Default for CellGate {
    fn default() -> Self {
        Self {
            minimum_tier: TrustTier::HermonVerified,
            require_signature: true,
        }
    }
}

impl CellGate {
    pub fn check(&self, manifest: &CellManifest) -> Result<(), CellError> {
        if self.require_signature && manifest.signature.is_none() {
            return Err(CellError::Untrusted("missing signature".into()));
        }
        let allowed = match (self.minimum_tier, manifest.trust_tier) {
            (TrustTier::Untrusted, _) => true,
            (_, TrustTier::Untrusted) => false,
            (TrustTier::ThirdParty, _) => true,
            (TrustTier::UserSigned, TrustTier::ThirdParty) => false,
            (TrustTier::UserSigned, _) => true,
            (TrustTier::TeamVerified, TrustTier::ThirdParty) => false,
            (TrustTier::TeamVerified, TrustTier::UserSigned) => false,
            (TrustTier::TeamVerified, _) => true,
            (TrustTier::HermonVerified, TrustTier::HermonVerified) => true,
            (TrustTier::HermonVerified, _) => false,
        };
        if !allowed {
            return Err(CellError::Untrusted(format!(
                "tier {:?} below required {:?}",
                manifest.trust_tier, self.minimum_tier
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> CellManifest {
        // Fixed timestamp so content_hash tests are deterministic.
        let created_at = chrono::DateTime::parse_from_rfc3339("2026-05-14T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        CellManifest {
            id: "cell.wish.shader.glsl_v1".into(),
            name: "GLSL Shader Generator".into(),
            version: "0.1.0".into(),
            author: "Hermon".into(),
            created_at,
            trust_tier: TrustTier::HermonVerified,
            capabilities: Capabilities {
                fs_read: vec!["assets/textures/".into()],
                fs_write: vec!["assets/shaders/".into()],
                model_invoke: true,
                world_read: true,
                world_patch: true,
                ..Default::default()
            },
            wasm_entry: Some("cell.wasm".into()),
            events_in: vec!["scene.entity.selected".into()],
            events_out: vec!["world.patch".into()],
            signature: Some("ed25519:deadbeef".into()),
        }
    }

    #[test]
    fn content_hash_is_stable_excluding_signature() {
        let mut a = manifest();
        let mut b = manifest();
        a.signature = Some("sig-A".into());
        b.signature = Some("sig-B".into());
        assert_eq!(a.content_hash_hex(), b.content_hash_hex());
    }

    #[test]
    fn content_hash_changes_with_capability_change() {
        let a = manifest();
        let mut b = manifest();
        b.capabilities.net_fetch = true;
        assert_ne!(a.content_hash_hex(), b.content_hash_hex());
    }

    #[test]
    fn gate_accepts_hermon_verified() {
        let m = manifest();
        let gate = CellGate::default();
        assert!(gate.check(&m).is_ok());
    }

    #[test]
    fn gate_rejects_unsigned_when_required() {
        let mut m = manifest();
        m.signature = None;
        let gate = CellGate::default();
        assert!(matches!(gate.check(&m), Err(CellError::Untrusted(_))));
    }

    #[test]
    fn capability_subset_check() {
        let parent = Capabilities {
            fs_read: vec!["assets/".into()],
            fs_write: vec!["assets/shaders/".into()],
            model_invoke: true,
            world_read: true,
            world_patch: true,
            ..Default::default()
        };
        let child = Capabilities {
            fs_read: vec!["assets/textures/".into()],
            fs_write: vec!["assets/shaders/glsl/".into()],
            world_read: true,
            ..Default::default()
        };
        assert!(child.is_subset_of(&parent));
        let mut overreach = child.clone();
        overreach.finance_transact = true;
        assert!(!overreach.is_subset_of(&parent));
    }
}
