//! Missions and VerifiableArtifacts.
//!
//! See `wish-design/wish-plan-20260514/04-data-model/05-verifiable-artifacts.md`
//! and `wish-design/wish-plan-20260514/09-product-surfaces/07-mission-cockpit.md`.
//!
//! A `Mission` is the unit of agent work, world-aware from the start.
//! A `VerifiableArtifact` is the tamper-evident proof of a world
//! transition produced by the mission — Wish's stronger answer to
//! Google Antigravity's "Verifiable Artifacts."

use crate::patch::{Actor, PatchId};
use crate::semantic_id::SemanticId;
use crate::world::WorldEventId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type MissionId = String;
pub type VerifiableArtifactId = String;
pub type BranchId = String;
pub type SignatureId = String;

pub const DEFAULT_BRANCH: &str = "main";

/// The unit of agent work, world-aware from the start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: MissionId,
    pub world_id: String,
    pub intent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<MissionId>,
    pub branch: BranchId,
    #[serde(default)]
    pub plan: Vec<MissionStep>,
    #[serde(default)]
    pub status: MissionStatus,
    #[serde(default)]
    pub artifacts: Vec<VerifiableArtifactId>,
    #[serde(default)]
    pub approvals: Vec<ApprovalRecord>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}

impl Mission {
    pub fn new(world_id: impl Into<String>, intent: impl Into<String>) -> Self {
        let now = Utc::now();
        let id = format!("mission_{}", now.timestamp_nanos_opt().unwrap_or(0));
        Self {
            id,
            world_id: world_id.into(),
            intent: intent.into(),
            parent: None,
            branch: DEFAULT_BRANCH.to_string(),
            plan: Vec::new(),
            status: MissionStatus::default(),
            artifacts: Vec::new(),
            approvals: Vec::new(),
            capabilities: Vec::new(),
            started_at: now,
            finished_at: None,
        }
    }

    pub fn with_parent(mut self, parent: MissionId) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = branch.into();
        self
    }

    pub fn add_step(&mut self, step: MissionStep) {
        self.plan.push(step);
    }

    pub fn attach_artifact(&mut self, id: VerifiableArtifactId) {
        self.artifacts.push(id);
    }

    pub fn close(&mut self, terminal: MissionStatus) {
        self.status = terminal;
        self.finished_at = Some(Utc::now());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionStep {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub status: MissionStatus,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    #[default]
    Planned,
    Running,
    WaitingHuman,
    WaitingSimulation,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub at: DateTime<Utc>,
    pub by: Actor,
    pub decision: ApprovalDecision,
    pub gate: ApprovalGate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    Branched,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalGate {
    Auto,
    Human,
    Simulation,
    Capability,
    OnChain,
}

/// Tamper-evident proof of a world transition produced by a mission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableArtifact {
    pub id: VerifiableArtifactId,
    pub mission_id: MissionId,
    pub kind: ArtifactKind,
    pub world_event: WorldEventId,
    pub patch_id: PatchId,
    #[serde(default)]
    pub affected: Vec<SemanticId>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub validation: ArtifactValidation,
    #[serde(default)]
    pub signatures: Vec<Signature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merkle_proof: Option<MerkleProof>,
    pub created_at: DateTime<Utc>,
}

impl VerifiableArtifact {
    pub fn new(
        mission_id: impl Into<String>,
        kind: ArtifactKind,
        world_event: impl Into<String>,
        patch_id: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        let id = format!("artifact_{}", now.timestamp_nanos_opt().unwrap_or(0));
        Self {
            id,
            mission_id: mission_id.into(),
            kind,
            world_event: world_event.into(),
            patch_id: patch_id.into(),
            affected: Vec::new(),
            evidence: Vec::new(),
            validation: ArtifactValidation::default(),
            signatures: Vec::new(),
            merkle_proof: None,
            created_at: now,
        }
    }

    pub fn with_affected(mut self, affected: Vec<SemanticId>) -> Self {
        self.affected = affected;
        self
    }

    pub fn add_evidence(&mut self, ev: Evidence) {
        self.evidence.push(ev);
    }

    pub fn sign(&mut self, sig: Signature) {
        self.signatures.push(sig);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    CodeChange,
    CanvasChange,
    SceneChange,
    AssetGeneration,
    Deployment,
    SimulationRun,
    FinancialTxn,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Evidence {
    Diff { unified: String },
    Screenshot { png_path: String, alt: String },
    Recording { uri: String, duration_ms: u32 },
    TestOutput { passed: u32, failed: u32, log: String },
    LogTrace { entries: Vec<String> },
    RuntimeObservation { source: String, payload: serde_json::Value },
    External { source: String, payload: serde_json::Value },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactValidation {
    pub tests_passed: u32,
    pub tests_failed: u32,
    pub lint_errors: u32,
    pub lint_warnings: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_review: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulation_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub id: SignatureId,
    pub signer: Signer,
    /// Hex-encoded signature bytes.
    pub bytes: String,
    pub algorithm: String,
    pub signed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Signer {
    Agent { agent_id: String },
    Human { user_id: String },
    IWallet { address: String },
    Hermon { key_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub root_hex: String,
    pub path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creditchain_tx: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_id::SemanticId;

    #[test]
    fn smoke_mission_roundtrip() {
        let mut m = Mission::new("world_abc", "do a thing");
        m.add_step(MissionStep {
            id: "1".into(),
            label: "plan".into(),
            status: MissionStatus::Planned,
            depends_on: vec![],
        });
        let json = serde_json::to_string(&m).unwrap();
        let parsed: Mission = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.world_id, "world_abc");
        assert_eq!(parsed.plan.len(), 1);
    }

    #[test]
    fn smoke_artifact_with_evidence_and_signature() {
        let mut a = VerifiableArtifact::new(
            "mission_x",
            ArtifactKind::CodeChange,
            "event_1",
            "patch_1",
        )
        .with_affected(vec![SemanticId::code_function("a::b")]);
        a.add_evidence(Evidence::Diff {
            unified: "@@ -1 +1 @@\n-a\n+b".into(),
        });
        a.add_evidence(Evidence::TestOutput {
            passed: 41,
            failed: 0,
            log: "ok".into(),
        });
        a.sign(Signature {
            id: "s1".into(),
            signer: Signer::Agent {
                agent_id: "wish-agent-coder".into(),
            },
            bytes: "deadbeef".into(),
            algorithm: "ed25519".into(),
            signed_at: Utc::now(),
        });
        let json = serde_json::to_string(&a).unwrap();
        let parsed: VerifiableArtifact = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kind, ArtifactKind::CodeChange);
        assert_eq!(parsed.affected.len(), 1);
        assert_eq!(parsed.evidence.len(), 2);
        assert_eq!(parsed.signatures.len(), 1);
    }

    #[test]
    fn smoke_close_mission() {
        let mut m = Mission::new("w", "test");
        assert!(m.finished_at.is_none());
        m.close(MissionStatus::Succeeded);
        assert!(matches!(m.status, MissionStatus::Succeeded));
        assert!(m.finished_at.is_some());
    }
}
