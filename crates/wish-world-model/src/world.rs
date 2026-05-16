//! `WishWorld` — the top-level semantic container.

use crate::semantic_id::SemanticId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type WorldId = String;
pub type WorldEventId = String;

/// What kind of world this is. Drives bridge behavior and risk policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldKind {
    GenericProject,
    FinalverseRegion,
    LiveService,
    FintechDemo,
    EducationWorld,
    Custom(String),
}

/// A Wish world. Mirrors `world.json` in `.wishworld/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WishWorld {
    pub schema: String,
    pub id: WorldId,
    pub name: String,
    pub kind: WorldKind,
    #[serde(default)]
    pub intent: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub entities: HashMap<String, WorldEntity>,
    #[serde(default)]
    pub scenes: HashMap<String, WorldScene>,
    #[serde(default)]
    pub agents: HashMap<String, WorldAgent>,
    #[serde(default)]
    pub assets: HashMap<String, WorldAsset>,
    #[serde(default)]
    pub rules: Vec<WorldRule>,
    #[serde(default)]
    pub memory: WorldMemory,
    /// The latest provenance tail held in-memory. The full ledger lives in
    /// `wish-provenance::WorldLine`.
    #[serde(default)]
    pub provenance: Vec<WorldEvent>,
}

impl Default for WishWorld {
    /// An empty `WishWorld` named "untitled". Useful when reading an
    /// otherwise-empty `.wishworld/` skeleton or starting from scratch.
    fn default() -> Self {
        Self::new("untitled", WorldKind::GenericProject)
    }
}

impl WishWorld {
    pub fn new(name: impl Into<String>, kind: WorldKind) -> Self {
        Self {
            schema: crate::WISHWORLD_SCHEMA.to_string(),
            id: format!("world_{}", chrono::Utc::now().timestamp_micros()),
            name: name.into(),
            kind,
            intent: String::new(),
            created_at: Utc::now(),
            entities: HashMap::new(),
            scenes: HashMap::new(),
            agents: HashMap::new(),
            assets: HashMap::new(),
            rules: Vec::new(),
            memory: WorldMemory::default(),
            provenance: Vec::new(),
        }
    }

    pub fn entity(&self, id: &SemanticId) -> Option<&WorldEntity> {
        self.entities.get(&id.to_string())
    }

    pub fn upsert_entity(&mut self, entity: WorldEntity) {
        self.entities.insert(entity.id.to_string(), entity);
    }

    pub fn remove_entity(&mut self, id: &SemanticId) -> Option<WorldEntity> {
        self.entities.remove(&id.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEntity {
    pub id: SemanticId,
    pub kind: EntityKind,
    pub display_name: String,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<SourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_ref: Option<AgentRef>,
    #[serde(default)]
    pub status: EntityStatus,
    #[serde(default)]
    pub agent_editable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_head: Option<WorldEventId>,
}

impl WorldEntity {
    /// Minimal constructor used by tests and bridges.
    pub fn stub(id: SemanticId, display_name: impl Into<String>, kind: EntityKind) -> Self {
        Self {
            id,
            kind,
            display_name: display_name.into(),
            components: Vec::new(),
            source_ref: None,
            agent_ref: None,
            status: EntityStatus::Ok,
            agent_editable: false,
            provenance_head: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    File,
    Function,
    Crate,
    Package,
    Module,
    Service,
    Agent,
    ToolCall,
    Test,
    Commit,
    Diff,
    TerminalBlock,
    DocumentSection,
    Npc,
    Quest,
    SacredArchitecture,
    Asset,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityStatus {
    #[default]
    Ok,
    Warning,
    Error,
    Pending,
    Running,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Component {
    Transform(Transform),
    MeshReference { reference: String },
    MaterialSet { reference: String },
    LightingProfile { preset: String },
    QuestAnchor { quest_ref: String },
    SoundscapeAnchor { reference: String },
    LoreAnchor { reference: String },
    BehaviorScript { reference: String },
    EconomicActor { profile: String },
    Custom(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4], // quaternion
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRef {
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldScene {
    pub id: SemanticId,
    pub display_name: String,
    #[serde(default)]
    pub entity_ids: Vec<SemanticId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldAgent {
    pub id: SemanticId,
    pub display_name: String,
    pub role: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldAsset {
    pub id: SemanticId,
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldRule {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub policy: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldMemory {
    #[serde(default)]
    pub facts: Vec<MemoryFact>,
    #[serde(default)]
    pub lore: Option<String>,
    #[serde(default)]
    pub design_rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFact {
    pub when: DateTime<Utc>,
    pub subject: SemanticId,
    pub fact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEvent {
    pub id: WorldEventId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<WorldEventId>,
    pub branch: String,
    pub timestamp: DateTime<Utc>,
    pub intent: String,
    pub affected: Vec<SemanticId>,
}
