//! `WorldPatch` — the single mutation primitive.

use crate::semantic_id::SemanticId;
use crate::world::{Component, WishWorld, WorldAgent, WorldAsset, WorldEntity, WorldRule, WorldScene};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type PatchId = String;

/// Who emitted a patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    Human { user_id: String },
    Agent { agent_id: String },
    System,
}

/// A single, atomic operation on a [`WishWorld`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOp {
    AddEntity(WorldEntity),
    RemoveEntity { id: SemanticId },
    UpdateComponent { entity: SemanticId, component: Component },
    AddScene(WorldScene),
    AddAgent(WorldAgent),
    AddAsset(WorldAsset),
    AddRule(WorldRule),
    SetIntent { intent: String },
    Custom(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPatch {
    pub id: PatchId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<PatchId>,
    pub author: Actor,
    pub intent: String,
    pub ops: Vec<PatchOp>,
    #[serde(default)]
    pub affected: Vec<SemanticId>,
    pub created_at: DateTime<Utc>,
}

impl WorldPatch {
    pub fn new(author: Actor, intent: impl Into<String>, ops: Vec<PatchOp>) -> Self {
        let id = format!("patch_{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
        let affected = ops_affected(&ops);
        Self {
            id,
            parent: None,
            author,
            intent: intent.into(),
            ops,
            affected,
            created_at: Utc::now(),
        }
    }
}

fn ops_affected(ops: &[PatchOp]) -> Vec<SemanticId> {
    let mut out = Vec::new();
    for op in ops {
        match op {
            PatchOp::AddEntity(e) => out.push(e.id.clone()),
            PatchOp::RemoveEntity { id } => out.push(id.clone()),
            PatchOp::UpdateComponent { entity, .. } => out.push(entity.clone()),
            PatchOp::AddScene(s) => out.push(s.id.clone()),
            PatchOp::AddAgent(a) => out.push(a.id.clone()),
            PatchOp::AddAsset(a) => out.push(a.id.clone()),
            PatchOp::AddRule(_) | PatchOp::SetIntent { .. } | PatchOp::Custom(_) => {}
        }
    }
    out
}

#[derive(Debug, Error)]
pub enum PatchError {
    #[error("entity not found: {0}")]
    EntityNotFound(String),
    #[error("invalid patch: {0}")]
    Invalid(String),
}

pub fn apply_patch(world: &mut WishWorld, patch: &WorldPatch) -> Result<(), PatchError> {
    for op in &patch.ops {
        match op {
            PatchOp::AddEntity(entity) => {
                world.upsert_entity(entity.clone());
            }
            PatchOp::RemoveEntity { id } => {
                if world.remove_entity(id).is_none() {
                    return Err(PatchError::EntityNotFound(id.to_string()));
                }
            }
            PatchOp::UpdateComponent { entity, component } => {
                let key = entity.to_string();
                let target = world
                    .entities
                    .get_mut(&key)
                    .ok_or_else(|| PatchError::EntityNotFound(key))?;
                target.components.push(component.clone());
            }
            PatchOp::AddScene(scene) => {
                world.scenes.insert(scene.id.to_string(), scene.clone());
            }
            PatchOp::AddAgent(agent) => {
                world.agents.insert(agent.id.to_string(), agent.clone());
            }
            PatchOp::AddAsset(asset) => {
                world.assets.insert(asset.id.to_string(), asset.clone());
            }
            PatchOp::AddRule(rule) => {
                world.rules.push(rule.clone());
            }
            PatchOp::SetIntent { intent } => {
                world.intent = intent.clone();
            }
            PatchOp::Custom(_) => {}
        }
    }
    Ok(())
}

/// Heuristic risk score in `[0.0, 1.0]`.
///
/// Drives the approval gate. The current heuristic is intentionally
/// simple — it will be replaced by a learned model in v0.9.0.
pub fn risk_score(patch: &WorldPatch) -> f32 {
    let mut score: f32 = 0.0;

    // Reach: number of distinct affected entities.
    let reach = patch.affected.len() as f32;
    score += (reach / 50.0).min(0.4);

    // Destructive ops weigh heavier than additive ones.
    for op in &patch.ops {
        match op {
            PatchOp::RemoveEntity { .. } => score += 0.15,
            PatchOp::UpdateComponent { .. } => score += 0.05,
            PatchOp::SetIntent { .. } => score += 0.02,
            _ => score += 0.01,
        }
    }

    // Agent-authored patches get a small confidence-floor penalty.
    if matches!(patch.author, Actor::Agent { .. }) {
        score += 0.05;
    }

    score.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_id::SemanticId;
    use crate::world::{EntityKind, WishWorld, WorldEntity, WorldKind};

    #[test]
    fn add_then_remove() {
        let mut w = WishWorld::new("t", WorldKind::GenericProject);
        let id = SemanticId::code_function("a::b");
        let e = WorldEntity::stub(id.clone(), "b", EntityKind::Function);
        let add = WorldPatch::new(Actor::System, "add", vec![PatchOp::AddEntity(e)]);
        let rm = WorldPatch::new(
            Actor::System,
            "rm",
            vec![PatchOp::RemoveEntity { id: id.clone() }],
        );
        apply_patch(&mut w, &add).unwrap();
        assert!(w.entity(&id).is_some());
        apply_patch(&mut w, &rm).unwrap();
        assert!(w.entity(&id).is_none());
    }

    #[test]
    fn remove_missing_errors() {
        let mut w = WishWorld::new("t", WorldKind::GenericProject);
        let id = SemanticId::code_function("nope");
        let rm = WorldPatch::new(
            Actor::System,
            "rm",
            vec![PatchOp::RemoveEntity { id }],
        );
        assert!(matches!(
            apply_patch(&mut w, &rm),
            Err(PatchError::EntityNotFound(_))
        ));
    }

    #[test]
    fn risk_bounds() {
        let p = WorldPatch::new(Actor::System, "no-op", vec![]);
        let r = risk_score(&p);
        assert!((0.0..=1.0).contains(&r));
    }
}
