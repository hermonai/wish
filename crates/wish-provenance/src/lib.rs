//! Wish WorldLine — the append-only provenance ledger.
//!
//! v0.5.0 ships a **JSONL-only stub**. The full SQLite-backed
//! implementation, branch + rollback, and optional CreditChain anchoring
//! land in v0.7.0 (per
//! `wish-design/wish-plan-20260514/03-crates/08-wish-provenance.md`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};
use thiserror::Error;
use wish_world_model::{
    apply_patch, risk_score, Actor, SemanticId, WishWorld, WorldEventId, WorldPatch,
};

pub type BranchId = String;
pub const DEFAULT_BRANCH: &str = "main";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEvent {
    pub id: WorldEventId,
    pub parent: Option<WorldEventId>,
    pub branch: BranchId,
    pub actor: Actor,
    pub intent: String,
    pub patch: WorldPatch,
    #[serde(default)]
    pub validation: ValidationResult,
    #[serde(default)]
    pub approval: ApprovalState,
    pub affected: Vec<SemanticId>,
    pub risk_score: f32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationResult {
    pub ok: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    #[default]
    Pending,
    AutoApproved,
    Approved,
    Rejected,
}

#[derive(Debug, Error)]
pub enum WorldLineError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("patch apply: {0}")]
    PatchApply(String),
}

/// Outcome of a provenance-coupled apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied {
        event_id: WorldEventId,
        gate: ApprovalGate,
    },
    Rejected {
        reason: String,
    },
    Pending {
        event_id: WorldEventId,
        gate: ApprovalGate,
    },
}

/// Approval gate the patch's risk fell into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalGate {
    Auto,
    HumanRequired,
    SimulationRequired,
}

impl ApprovalGate {
    pub fn for_risk(risk: f32) -> Self {
        if risk < 0.30 {
            Self::Auto
        } else if risk < 0.70 {
            Self::HumanRequired
        } else {
            Self::SimulationRequired
        }
    }
}

/// Append-only WorldLine backed by JSONL.
pub struct WorldLine {
    path: PathBuf,
    events: Vec<WorldEvent>,
    /// Branch name that future appends from this handle land on.
    /// Defaults to `DEFAULT_BRANCH`. Use `switch_to` to fork.
    current_branch: String,
}

impl WorldLine {
    /// Open a WorldLine at `<world_dir>/provenance/worldline.jsonl`.
    pub fn open_in_world_dir(world_dir: impl Into<PathBuf>) -> Result<Self, WorldLineError> {
        let mut path = world_dir.into();
        path.push("provenance");
        std::fs::create_dir_all(&path)?;
        path.push("worldline.jsonl");
        Self::open(path)
    }

    pub fn open(path: PathBuf) -> Result<Self, WorldLineError> {
        let mut events = Vec::new();
        if path.exists() {
            let f = File::open(&path)?;
            let r = BufReader::new(f);
            for line in r.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let ev: WorldEvent = serde_json::from_str(&line)?;
                events.push(ev);
            }
        }
        // The current branch is the branch of the most recent event,
        // or the default if the ledger is empty.
        let current_branch = events
            .last()
            .map(|e| e.branch.clone())
            .unwrap_or_else(|| DEFAULT_BRANCH.to_string());
        Ok(Self {
            path,
            events,
            current_branch,
        })
    }

    /// The path this WorldLine writes to.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn append(&mut self, event: WorldEvent) -> Result<WorldEventId, WorldLineError> {
        let id = event.id.clone();
        let serialized = serde_json::to_string(&event)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{serialized}")?;
        self.events.push(event);
        Ok(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &WorldEvent> {
        self.events.iter()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Currently active branch — new events will be appended under
    /// this name. Defaults to `main` until `switch_to` or
    /// `branch_from` is called.
    pub fn current_branch(&self) -> &str {
        &self.current_branch
    }

    /// Switch the current branch. Future calls to
    /// `apply_with_provenance` will tag their events with this
    /// branch. Does not move the head — appends to the new branch
    /// can interleave with appends to other branches; that's
    /// intentional and matches how distributed agents collaborate.
    pub fn switch_to(&mut self, branch: impl Into<String>) {
        self.current_branch = branch.into();
    }

    /// Enumerate every distinct branch name that appears in the
    /// ledger, in insertion order, with the current branch always
    /// included even if it's empty.
    pub fn branches(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for ev in &self.events {
            if seen.insert(ev.branch.clone()) {
                out.push(ev.branch.clone());
            }
        }
        if !seen.contains(&self.current_branch) {
            out.push(self.current_branch.clone());
        }
        out
    }

    /// Number of events on a specific branch.
    pub fn count_on(&self, branch: &str) -> usize {
        self.events.iter().filter(|e| e.branch == branch).count()
    }

    /// Create a new branch starting from a specific event id (or the
    /// current head if `from` is `None`). Writes a `branch_from`
    /// marker event so the fork point is provenance-anchored, and
    /// switches the current branch so subsequent appends land on the
    /// new branch.
    pub fn branch_from(
        &mut self,
        new_branch: impl Into<String>,
        from: Option<&str>,
    ) -> Result<WorldEventId, WorldLineError> {
        let new_branch = new_branch.into();
        let parent_id: Option<String> = match from {
            Some(id) => {
                if !self.events.iter().any(|e| e.id == id) {
                    return Err(WorldLineError::PatchApply(format!(
                        "branch parent not found: {id}"
                    )));
                }
                Some(id.to_string())
            }
            None => self.events.last().map(|e| e.id.clone()),
        };
        let now = Utc::now();
        let id = format!("ev_branch_{}", now.timestamp_nanos_opt().unwrap_or(0));
        let marker = wish_world_model::WorldPatch::new(
            wish_world_model::Actor::System,
            format!("branch_from: {new_branch}"),
            vec![],
        );
        let ev = WorldEvent {
            id: id.clone(),
            parent: parent_id,
            branch: new_branch.clone(),
            actor: wish_world_model::Actor::System,
            intent: format!("branch_from: {new_branch}"),
            affected: vec![],
            patch: marker,
            validation: ValidationResult {
                ok: true,
                notes: Some("branch marker".into()),
            },
            approval: ApprovalState::AutoApproved,
            risk_score: 0.0,
            timestamp: now,
        };
        self.append(ev)?;
        self.current_branch = new_branch;
        Ok(id)
    }

    /// Time-travel: reset the world's mutable collections and re-apply
    /// every WorldEvent's patch in order, up to (and including) index
    /// `until_idx_exclusive` exclusive. Returns the number of events
    /// applied.
    ///
    /// Treats the WorldLine as authoritative history. Useful for
    /// scrubbing through a world's life from outside the
    /// `apply_with_provenance` loop (e.g. in a time-travel viewer).
    /// Behavior is well-defined for any value of `until_idx_exclusive`
    /// up to `events.len()`; out-of-range values are clamped.
    pub fn replay_into(
        &self,
        world: &mut wish_world_model::WishWorld,
        until_idx_exclusive: usize,
    ) -> Result<usize, WorldLineError> {
        world.entities.clear();
        world.scenes.clear();
        world.agents.clear();
        world.assets.clear();
        world.rules.clear();
        world.provenance.clear();
        let end = until_idx_exclusive.min(self.events.len());
        for ev in &self.events[..end] {
            wish_world_model::apply_patch(world, &ev.patch)
                .map_err(|e| WorldLineError::PatchApply(e.to_string()))?;
        }
        Ok(end)
    }

    /// Read the file's last-modified timestamp. Returns `None` if the
    /// backing path doesn't exist yet or `Err` if metadata reads
    /// fail. Used by the hot-reload watcher.
    pub fn file_mtime(&self) -> Option<std::time::SystemTime> {
        std::fs::metadata(&self.path)
            .ok()
            .and_then(|m| m.modified().ok())
    }

    /// Reload from disk if the file is newer than the in-memory state.
    /// Returns the new total event count (or `None` if not reloaded).
    pub fn reload_if_changed(
        &mut self,
        last_seen: &mut Option<std::time::SystemTime>,
    ) -> Result<Option<usize>, WorldLineError> {
        let mtime = self.file_mtime();
        if mtime == *last_seen {
            return Ok(None);
        }
        *last_seen = mtime;
        // Re-open from disk to get the freshest events.
        let fresh = Self::open(self.path.clone())?;
        self.events = fresh.events;
        // Adopt the freshest branch tip so external writers' new
        // branches show up in `branches()` / `current_branch()`.
        self.current_branch = fresh.current_branch;
        Ok(Some(self.events.len()))
    }

    /// Apply a `WorldPatch` to a `WishWorld` and append the resulting
    /// `WorldEvent` to this WorldLine atomically.
    ///
    /// The gate is derived from `wish_world_model::risk_score`. By
    /// default `auto_approve_risk_below` is `0.30`. Set
    /// `auto_approve_max` to a different threshold (e.g. via per-world
    /// policy) for stricter or looser behavior.
    pub fn apply_with_provenance(
        &mut self,
        world: &mut WishWorld,
        patch: WorldPatch,
        auto_approve_max: f32,
    ) -> Result<ApplyOutcome, WorldLineError> {
        let risk = risk_score(&patch);
        let gate = ApprovalGate::for_risk(risk);
        let auto_ok = risk < auto_approve_max;

        // For local/agent flows we apply optimistically when auto-approved;
        // otherwise we record the event as pending without mutating the world.
        let affected = patch.affected.clone();
        let intent = patch.intent.clone();
        let actor = patch.author.clone();
        let event_id = format!(
            "ev_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );

        if auto_ok {
            apply_patch(world, &patch).map_err(|e| WorldLineError::PatchApply(e.to_string()))?;
            let ev = WorldEvent {
                id: event_id.clone(),
                parent: self.events.last().map(|e| e.id.clone()),
                branch: self.current_branch.clone(),
                actor,
                intent,
                affected,
                patch,
                validation: ValidationResult {
                    ok: true,
                    notes: None,
                },
                approval: ApprovalState::AutoApproved,
                risk_score: risk,
                timestamp: Utc::now(),
            };
            self.append(ev)?;
            Ok(ApplyOutcome::Applied { event_id, gate })
        } else {
            // Record as pending — world unchanged.
            let ev = WorldEvent {
                id: event_id.clone(),
                parent: self.events.last().map(|e| e.id.clone()),
                branch: self.current_branch.clone(),
                actor,
                intent,
                affected,
                patch,
                validation: ValidationResult {
                    ok: false,
                    notes: Some("pending approval".into()),
                },
                approval: ApprovalState::Pending,
                risk_score: risk,
                timestamp: Utc::now(),
            };
            self.append(ev)?;
            Ok(ApplyOutcome::Pending { event_id, gate })
        }
    }

    /// Approve a previously-pending WorldEvent and apply it to the
    /// world. Looks up by event id.
    pub fn approve_pending(
        &mut self,
        world: &mut WishWorld,
        event_id: &str,
    ) -> Result<ApplyOutcome, WorldLineError> {
        let idx = self
            .events
            .iter()
            .position(|e| e.id == event_id)
            .ok_or_else(|| WorldLineError::PatchApply(format!("event not found: {event_id}")))?;
        let patch = self.events[idx].patch.clone();
        apply_patch(world, &patch).map_err(|e| WorldLineError::PatchApply(e.to_string()))?;
        // Append a follow-up event recording the approval. The pending
        // event itself stays in the log for audit; downstream tooling can
        // collapse the pair.
        let now = Utc::now();
        let approval_id = format!("ev_{}_approved", now.timestamp_nanos_opt().unwrap_or(0));
        let ev = WorldEvent {
            id: approval_id.clone(),
            parent: Some(event_id.to_string()),
            branch: self.current_branch.clone(),
            actor: Actor::System,
            intent: format!("approve: {}", self.events[idx].intent),
            affected: self.events[idx].affected.clone(),
            patch,
            validation: ValidationResult {
                ok: true,
                notes: Some("approved".into()),
            },
            approval: ApprovalState::Approved,
            risk_score: self.events[idx].risk_score,
            timestamp: now,
        };
        // Update the original event's approval state.
        self.events[idx].approval = ApprovalState::Approved;
        self.append(ev)?;
        Ok(ApplyOutcome::Applied {
            event_id: approval_id,
            gate: ApprovalGate::for_risk(self.events[idx].risk_score),
        })
    }

    /// Rolling Merkle root of all events on `branch`. Order-stable per
    /// branch.
    pub fn merkle_root(&self, branch: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for ev in self.events.iter().filter(|e| e.branch == branch) {
            hasher.update(ev.id.as_bytes());
            if let Ok(bytes) = serde_json::to_vec(&ev.patch) {
                hasher.update(&bytes);
            }
        }
        let out = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&out);
        arr
    }
}

// ─────────────────────────────────────────────────────────────────────
// Scenario API — Wave 26 counterfactual fork on top of WorldLine
// branches. The branching primitives (`branch_from`, `switch_to`,
// `branches`) already exist below; this module wraps them in
// **what-if** semantics: a Scenario is a named branch with a parent
// event id, and `compare_scenarios` produces a diff suitable for
// presentation in the URE inspector.
// ─────────────────────────────────────────────────────────────────────

/// A named counterfactual branch of a [`WorldLine`]. The URE's
/// answer to "what would have happened if we had decided differently
/// at event X?" — create a Scenario rooted at X, simulate forward,
/// compare to the original.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Scenario {
    /// Branch name on the WorldLine (e.g. `"what-if-fed-pivots"`).
    pub branch: BranchId,
    /// Event id where this scenario diverged from its parent.
    pub fork_event: Option<WorldEventId>,
    /// Human-readable description of the hypothesis being tested.
    pub hypothesis: String,
}

impl Scenario {
    pub fn new(branch: impl Into<String>, hypothesis: impl Into<String>) -> Self {
        Self {
            branch: branch.into(),
            fork_event: None,
            hypothesis: hypothesis.into(),
        }
    }

    /// Open this scenario on the given [`WorldLine`] — branches off
    /// the current head (or the explicit `from` event if set) and
    /// switches the line's active branch to this scenario's name.
    /// Subsequent appends land in this scenario's branch.
    pub fn open(
        &mut self,
        line: &mut WorldLine,
        from: Option<&str>,
    ) -> Result<WorldEventId, WorldLineError> {
        let id = line.branch_from(self.branch.clone(), from)?;
        self.fork_event = Some(id.clone());
        Ok(id)
    }
}

/// Diff between two scenarios — the URE's counterfactual report.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScenarioDiff {
    pub left: BranchId,
    pub right: BranchId,
    pub left_event_count: usize,
    pub right_event_count: usize,
    /// Event ids that appear only on the left branch.
    pub only_in_left: Vec<WorldEventId>,
    /// Event ids that appear only on the right branch.
    pub only_in_right: Vec<WorldEventId>,
    /// Event ids that appear in both (the shared ancestry).
    pub shared: Vec<WorldEventId>,
}

/// Compare two branches of a [`WorldLine`] and produce a
/// [`ScenarioDiff`]. The shared section is the common prefix; the
/// `only_in_*` lists are each scenario's unique events.
pub fn compare_scenarios(line: &WorldLine, left: &str, right: &str) -> ScenarioDiff {
    use std::collections::HashSet;
    let left_ids: Vec<WorldEventId> = line
        .iter()
        .filter(|e| e.branch == left)
        .map(|e| e.id.clone())
        .collect();
    let right_ids: Vec<WorldEventId> = line
        .iter()
        .filter(|e| e.branch == right)
        .map(|e| e.id.clone())
        .collect();
    let left_set: HashSet<&WorldEventId> = left_ids.iter().collect();
    let right_set: HashSet<&WorldEventId> = right_ids.iter().collect();
    let only_in_left: Vec<WorldEventId> = left_ids
        .iter()
        .filter(|id| !right_set.contains(id))
        .cloned()
        .collect();
    let only_in_right: Vec<WorldEventId> = right_ids
        .iter()
        .filter(|id| !left_set.contains(id))
        .cloned()
        .collect();
    let shared: Vec<WorldEventId> = left_ids
        .iter()
        .filter(|id| right_set.contains(id))
        .cloned()
        .collect();
    ScenarioDiff {
        left: left.to_string(),
        right: right.to_string(),
        left_event_count: left_ids.len(),
        right_event_count: right_ids.len(),
        only_in_left,
        only_in_right,
        shared,
    }
}

#[cfg(test)]
mod scenario_tests {
    use super::*;
    use tempfile::TempDir;
    use wish_world_model::{Actor, WorldPatch};

    fn fresh_line() -> (TempDir, WorldLine) {
        let dir = TempDir::new().unwrap();
        let line = WorldLine::open_in_world_dir(dir.path()).unwrap();
        (dir, line)
    }

    fn add_event(line: &mut WorldLine, intent: &str) -> WorldEventId {
        let now = chrono::Utc::now();
        let id = format!("ev_{}_{}", intent, now.timestamp_nanos_opt().unwrap_or(0));
        let ev = WorldEvent {
            id: id.clone(),
            parent: None,
            branch: line.current_branch().to_string(),
            actor: Actor::System,
            intent: intent.to_string(),
            affected: vec![],
            patch: WorldPatch::new(Actor::System, intent, vec![]),
            validation: ValidationResult {
                ok: true,
                notes: None,
            },
            approval: ApprovalState::AutoApproved,
            risk_score: 0.0,
            timestamp: now,
        };
        line.append(ev).unwrap();
        id
    }

    #[test]
    fn scenario_open_forks_worldline_with_named_branch() {
        let (_dir, mut line) = fresh_line();
        add_event(&mut line, "baseline_step_1");
        add_event(&mut line, "baseline_step_2");
        let mut scenario =
            Scenario::new("what_if_x", "What if we had not done step 2 the same way?");
        scenario.open(&mut line, None).unwrap();
        // Scenario.fork_event is populated.
        assert!(scenario.fork_event.is_some());
        // The WorldLine's current branch switched.
        assert_eq!(line.current_branch(), "what_if_x");
        // The branches list contains both.
        let branches = line.branches();
        assert!(branches.iter().any(|b| b == "what_if_x"));
    }

    #[test]
    fn scenarios_can_diverge_and_be_compared() {
        let (_dir, mut line) = fresh_line();
        // Build baseline.
        add_event(&mut line, "baseline_a");
        add_event(&mut line, "baseline_b");
        // Branch into scenario_X and add 2 events.
        let mut sx = Scenario::new("scenario_x", "alt path X");
        sx.open(&mut line, None).unwrap();
        let x1 = add_event(&mut line, "x_step_1");
        let x2 = add_event(&mut line, "x_step_2");
        // Switch back to main and add 1 event.
        line.switch_to(DEFAULT_BRANCH.to_string());
        let m1 = add_event(&mut line, "main_step_3");
        let diff = compare_scenarios(&line, DEFAULT_BRANCH, "scenario_x");
        // main has baseline_a, baseline_b, main_step_3 → 3 events.
        assert_eq!(diff.left_event_count, 3);
        // scenario_x has the branch_from marker + x_step_1, x_step_2 → 3.
        assert_eq!(diff.right_event_count, 3);
        // No id is shared (branches are disjoint after fork).
        assert_eq!(diff.shared.len(), 0);
        assert!(diff.only_in_left.contains(&m1));
        assert!(diff.only_in_right.contains(&x1));
        assert!(diff.only_in_right.contains(&x2));
    }

    #[test]
    fn scenario_serializes_with_hypothesis() {
        let s = Scenario::new("test", "Hypothesis: rates rise by 100bps");
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("Hypothesis: rates rise by 100bps"));
        let back: Scenario = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wish_world_model::{
        EntityKind, PatchOp, Realm, SemanticId, WishWorld, WorldEntity, WorldKind, WorldPatch,
    };

    fn make_tmp_dir() -> PathBuf {
        // Collision-resistant under parallel cargo test: timestamp +
        // pid + atomic counter. Without all three, two parallel tests
        // can land on the same nanosecond and share a tmp dir, which
        // breaks count-based assertions.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "wl_{}_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn smoke_apply_with_provenance_auto_approves() {
        let dir = make_tmp_dir();
        let mut wl = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut world = WishWorld::new("test", WorldKind::GenericProject);
        let id = SemanticId::new(Realm::Scene, "npc", "merchant_liu");
        let entity = WorldEntity::stub(id.clone(), "Merchant Liu", EntityKind::Npc);
        let patch = WorldPatch::new(
            Actor::Agent {
                agent_id: "wish-agent-world-architect".into(),
            },
            "add merchant liu",
            vec![PatchOp::AddEntity(entity)],
        );
        let outcome = wl.apply_with_provenance(&mut world, patch, 0.30).unwrap();
        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));
        assert!(world.entity(&id).is_some());
        assert_eq!(wl.len(), 1);
    }

    #[test]
    fn smoke_apply_with_provenance_holds_high_risk() {
        let dir = make_tmp_dir();
        let mut wl = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut world = WishWorld::new("test", WorldKind::GenericProject);
        // Build a patch with broad reach (many distinct affected ids) so
        // the risk score climbs past the 0.30 auto-approve band.
        let mut ops = Vec::new();
        for i in 0..100 {
            let id = SemanticId::new(Realm::Code, "function", &format!("f{i}"));
            ops.push(PatchOp::AddEntity(WorldEntity::stub(
                id,
                format!("f{i}"),
                EntityKind::Function,
            )));
        }
        let patch = WorldPatch::new(
            Actor::Agent {
                agent_id: "agent".into(),
            },
            "wide-reach refactor",
            ops,
        );
        let outcome = wl.apply_with_provenance(&mut world, patch, 0.30).unwrap();
        assert!(matches!(outcome, ApplyOutcome::Pending { .. }));
        // World unchanged.
        assert!(world.entities.is_empty());
        assert_eq!(wl.len(), 1);
        let event_id = match outcome {
            ApplyOutcome::Pending { event_id, .. } => event_id,
            _ => unreachable!(),
        };
        // Approve it.
        let approved = wl.approve_pending(&mut world, &event_id).unwrap();
        assert!(matches!(approved, ApplyOutcome::Applied { .. }));
        assert_eq!(world.entities.len(), 100);
    }

    #[test]
    fn replay_into_zero_is_empty_world() {
        let dir = make_tmp_dir();
        let mut wl = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut world = WishWorld::new("rt", WorldKind::GenericProject);
        // Stamp the world with one entity to confirm we reset.
        world.upsert_entity(WorldEntity::stub(
            SemanticId::new(Realm::Code, "function", "stale"),
            "stale",
            EntityKind::Function,
        ));
        // Append a few events.
        for i in 0..3 {
            let id = SemanticId::new(Realm::Code, "function", &format!("f{i}"));
            let patch = WorldPatch::new(
                Actor::System,
                "add",
                vec![PatchOp::AddEntity(WorldEntity::stub(
                    id,
                    format!("f{i}"),
                    EntityKind::Function,
                ))],
            );
            wl.apply_with_provenance(&mut world, patch, 0.30).unwrap();
        }
        // Replay 0 events into the (currently 3-entity + stale) world.
        let applied = wl.replay_into(&mut world, 0).unwrap();
        assert_eq!(applied, 0);
        assert!(world.entities.is_empty());
    }

    #[test]
    fn replay_into_n_reconstructs_first_n() {
        let dir = make_tmp_dir();
        let mut wl = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut world = WishWorld::new("rt", WorldKind::GenericProject);
        for i in 0..5 {
            let id = SemanticId::new(Realm::Code, "function", &format!("f{i}"));
            let patch = WorldPatch::new(
                Actor::System,
                "add",
                vec![PatchOp::AddEntity(WorldEntity::stub(
                    id,
                    format!("f{i}"),
                    EntityKind::Function,
                ))],
            );
            wl.apply_with_provenance(&mut world, patch, 0.30).unwrap();
        }
        // Pristine world for replay.
        let mut clean = WishWorld::new("rt", WorldKind::GenericProject);
        let applied = wl.replay_into(&mut clean, 3).unwrap();
        assert_eq!(applied, 3);
        assert_eq!(clean.entities.len(), 3);
        let applied_all = wl.replay_into(&mut clean, 99).unwrap();
        assert_eq!(applied_all, 5);
        assert_eq!(clean.entities.len(), 5);
    }

    #[test]
    fn reload_if_changed_detects_external_writes() {
        let dir = make_tmp_dir();
        let mut wl_writer = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut wl_reader = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut last_seen = wl_reader.file_mtime();
        // No change yet.
        let reloaded = wl_reader.reload_if_changed(&mut last_seen).unwrap();
        assert!(reloaded.is_none());

        // External writer appends a few events.
        let mut world = WishWorld::new("rt", WorldKind::GenericProject);
        for i in 0..2 {
            let id = SemanticId::new(Realm::Code, "function", &format!("g{i}"));
            let patch = WorldPatch::new(
                Actor::System,
                "ext",
                vec![PatchOp::AddEntity(WorldEntity::stub(
                    id,
                    format!("g{i}"),
                    EntityKind::Function,
                ))],
            );
            wl_writer
                .apply_with_provenance(&mut world, patch, 0.30)
                .unwrap();
        }
        // Some filesystems have low mtime resolution — bump it by
        // sleeping briefly, but use a backoff loop so the test is
        // robust on fast disks.
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = std::fs::OpenOptions::new()
            .append(true)
            .open(wl_writer.path())
            .map(|_| ());
        let reloaded = wl_reader.reload_if_changed(&mut last_seen).unwrap();
        assert!(reloaded.is_some());
        assert!(wl_reader.len() >= 2);
    }

    #[test]
    fn branch_from_creates_named_branch_and_switches() {
        let dir = make_tmp_dir();
        let mut wl = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut world = WishWorld::new("br", WorldKind::GenericProject);

        // Append two events on main.
        for i in 0..2 {
            let id = SemanticId::new(Realm::Code, "function", &format!("m{i}"));
            let patch = WorldPatch::new(
                Actor::System,
                "main",
                vec![PatchOp::AddEntity(WorldEntity::stub(
                    id,
                    format!("m{i}"),
                    EntityKind::Function,
                ))],
            );
            wl.apply_with_provenance(&mut world, patch, 0.30).unwrap();
        }
        assert_eq!(wl.current_branch(), DEFAULT_BRANCH);
        assert_eq!(wl.count_on(DEFAULT_BRANCH), 2);

        // Fork an "experiment" branch from the current head.
        let _marker_id = wl.branch_from("experiment", None).unwrap();
        assert_eq!(wl.current_branch(), "experiment");

        // Two more appends — these should land on "experiment".
        for i in 0..2 {
            let id = SemanticId::new(Realm::Code, "function", &format!("x{i}"));
            let patch = WorldPatch::new(
                Actor::System,
                "experiment",
                vec![PatchOp::AddEntity(WorldEntity::stub(
                    id,
                    format!("x{i}"),
                    EntityKind::Function,
                ))],
            );
            wl.apply_with_provenance(&mut world, patch, 0.30).unwrap();
        }
        // 2 on main + 1 marker on experiment + 2 on experiment = 5
        assert_eq!(wl.len(), 5);
        assert_eq!(wl.count_on(DEFAULT_BRANCH), 2);
        assert_eq!(wl.count_on("experiment"), 3); // marker + 2

        // Switching back to main lets us append on main again.
        wl.switch_to(DEFAULT_BRANCH);
        let id = SemanticId::new(Realm::Code, "function", "back_on_main");
        wl.apply_with_provenance(
            &mut world,
            WorldPatch::new(
                Actor::System,
                "back",
                vec![PatchOp::AddEntity(WorldEntity::stub(
                    id,
                    "back",
                    EntityKind::Function,
                ))],
            ),
            0.30,
        )
        .unwrap();
        assert_eq!(wl.count_on(DEFAULT_BRANCH), 3);
        // branches() lists both, in insertion order.
        let branches = wl.branches();
        assert_eq!(branches[0], DEFAULT_BRANCH);
        assert_eq!(branches[1], "experiment");
    }

    #[test]
    fn branch_from_rejects_unknown_parent() {
        let dir = make_tmp_dir();
        let mut wl = WorldLine::open_in_world_dir(&dir).unwrap();
        let err = wl.branch_from("nope", Some("ev_does_not_exist"));
        assert!(err.is_err());
    }

    #[test]
    fn approval_gate_bands() {
        assert_eq!(ApprovalGate::for_risk(0.10), ApprovalGate::Auto);
        assert_eq!(ApprovalGate::for_risk(0.50), ApprovalGate::HumanRequired);
        assert_eq!(
            ApprovalGate::for_risk(0.85),
            ApprovalGate::SimulationRequired
        );
    }

    #[test]
    fn smoke_open_append_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "wl_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let mut wl = WorldLine::open_in_world_dir(&tmp).unwrap();
        let patch = WorldPatch::new(Actor::System, "noop", vec![]);
        let ev = WorldEvent {
            id: "ev1".into(),
            parent: None,
            branch: DEFAULT_BRANCH.into(),
            actor: Actor::System,
            intent: "noop".into(),
            affected: vec![],
            patch,
            validation: ValidationResult::default(),
            approval: ApprovalState::default(),
            risk_score: 0.0,
            timestamp: Utc::now(),
        };
        wl.append(ev).unwrap();
        assert_eq!(wl.len(), 1);
        let mr1 = wl.merkle_root(DEFAULT_BRANCH);
        // Reopen and verify.
        drop(wl);
        let wl2 = WorldLine::open_in_world_dir(&tmp).unwrap();
        assert_eq!(wl2.len(), 1);
        assert_eq!(wl2.merkle_root(DEFAULT_BRANCH), mr1);
    }
}
