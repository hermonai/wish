//! **Five Universal Primitives + Constraint** — the Universal Reality
//! Engine foundation layer.
//!
//! Strategic frame:
//! [`01-strategy/11-universal-reality-engine.md`](../../../wish-design/wish-plan-20260514/01-strategy/11-universal-reality-engine.md).
//!
//! Every structured reality Wish models reduces to a `Vec<Primitive>`:
//!
//! - **Object** — things with identity (atoms, cells, people, crates,
//!   contracts, assets, files)
//! - **Field** — continuous values over space/time (temperature,
//!   pressure, sentiment, liquidity, risk density)
//! - **Graph** — relationships (bonds, networks, supply chains,
//!   call graphs, social graphs)
//! - **Agent** — decision-making entities (people, AI agents, cells,
//!   firms, governments)
//! - **Event** — things that happen (collisions, reactions, trades,
//!   votes, edits, mutations)
//!
//! Plus **Constraint** — hard, soft, regulatory, ethical, physical,
//! or probabilistic limits that govern the others.
//!
//! These six types together form the wire format for cross-domain
//! reality modeling. A chemistry session, a finance scenario, a
//! social-simulation step, and a codegraph extraction all serialize
//! to the same `Vec<Primitive>` shape — what differs is the
//! `realm` discriminator on each primitive's `SemanticId`.

use crate::semantic_id::{Realm, SemanticId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One of the five universal primitives, plus the cross-cutting
/// Constraint. Tagged via serde so any tool — including AI agents —
/// can produce and consume the same JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "primitive", rename_all = "snake_case")]
pub enum Primitive {
    Object(Object),
    Field(Field),
    Graph(Graph),
    Agent(Agent),
    Event(Event),
    Constraint(Constraint),
}

impl Primitive {
    /// The [`SemanticId`] addressing this primitive. Every primitive
    /// has one — that's the universal-identity invariant.
    pub fn id(&self) -> &SemanticId {
        match self {
            Primitive::Object(o) => &o.id,
            Primitive::Field(f) => &f.id,
            Primitive::Graph(g) => &g.id,
            Primitive::Agent(a) => &a.id,
            Primitive::Event(e) => &e.id,
            Primitive::Constraint(c) => &c.id,
        }
    }

    /// The [`Realm`] this primitive lives in (Code, Finance, Biology,
    /// etc.). Inherited from the SemanticId — primitive and ID never
    /// disagree on realm.
    pub fn realm(&self) -> &Realm {
        &self.id().realm
    }
}

// ─────────────────────────────────────────────────────────────────────
// Object — things with identity
// ─────────────────────────────────────────────────────────────────────

/// An **Object** has stable identity, a typed kind, and a bag of
/// properties (`HashMap<String, PropertyValue>`). The typed kind is
/// a free-form string so each domain plugin can declare its own
/// taxonomy without core changes.
///
/// Examples:
/// - `Object { id: code:crate:wishd-index, kind: "rust_crate", … }`
/// - `Object { id: chem:molecule:caffeine, kind: "molecule", … }`
/// - `Object { id: finance:order:42, kind: "limit_order", … }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: SemanticId,
    pub kind: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, PropertyValue>,
}

impl Object {
    pub fn new(id: SemanticId, kind: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id,
            kind: kind.into(),
            display_name: display_name.into(),
            properties: HashMap::new(),
        }
    }

    pub fn with_property(mut self, key: impl Into<String>, value: PropertyValue) -> Self {
        self.properties.insert(key.into(), value);
        self
    }
}

/// A typed property value. Supports the JSON-ish set most AI agents
/// emit naturally, plus a `Ref(SemanticId)` so properties can point
/// back into the universe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    Number(f64),
    Text(String),
    Bool(bool),
    Ref(Box<SemanticId>),
    List(Vec<PropertyValue>),
    Map(HashMap<String, PropertyValue>),
}

// ─────────────────────────────────────────────────────────────────────
// Field — continuous values over space/time
// ─────────────────────────────────────────────────────────────────────

/// A **Field** is a typed value defined over a region of space-time
/// — a temperature field, a sentiment field, a risk-density field.
/// The `axes` carry the dimensions (e.g. `["x", "y"]` for a 2D
/// thermal map; `["lat", "lng", "time"]` for a weather model).
///
/// Sample values are stored as a flat array indexed by lexicographic
/// axis order, plus the shape of the sampled grid. Sparse fields use
/// `samples_sparse` instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub id: SemanticId,
    pub kind: String,
    pub axes: Vec<String>,
    /// Grid shape per axis (e.g. `[64, 64]` for a 64×64 2D sample).
    /// Length must equal `axes.len()`. Empty for pure-sparse fields.
    #[serde(default)]
    pub shape: Vec<usize>,
    /// Dense sample array, row-major. Empty when using sparse.
    #[serde(default)]
    pub samples_dense: Vec<f32>,
    /// Sparse samples: `(coords, value)` where coords has `axes.len()` entries.
    #[serde(default)]
    pub samples_sparse: Vec<(Vec<f32>, f32)>,
    /// Optional units string (`"K"`, `"USD/m²"`, `"sentiment"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

impl Field {
    pub fn dense(
        id: SemanticId,
        kind: impl Into<String>,
        axes: Vec<String>,
        shape: Vec<usize>,
        samples: Vec<f32>,
    ) -> Self {
        Self {
            id,
            kind: kind.into(),
            axes,
            shape,
            samples_dense: samples,
            samples_sparse: Vec::new(),
            unit: None,
        }
    }

    pub fn sparse(
        id: SemanticId,
        kind: impl Into<String>,
        axes: Vec<String>,
        samples: Vec<(Vec<f32>, f32)>,
    ) -> Self {
        Self {
            id,
            kind: kind.into(),
            axes,
            shape: Vec::new(),
            samples_dense: Vec::new(),
            samples_sparse: samples,
            unit: None,
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    /// Total number of samples (dense + sparse).
    pub fn sample_count(&self) -> usize {
        self.samples_dense.len() + self.samples_sparse.len()
    }
}

// ─────────────────────────────────────────────────────────────────────
// Graph — relationships
// ─────────────────────────────────────────────────────────────────────

/// A **Graph** is a set of nodes (referenced by `SemanticId`) and
/// directed edges with typed kinds. Use for chemistry bonds, supply
/// chains, social networks, gene regulation, codegraph dep edges,
/// causal chains — anything where the *structure* of relationships
/// is the modeled quantity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub id: SemanticId,
    pub kind: String,
    pub nodes: Vec<SemanticId>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: SemanticId,
    pub to: SemanticId,
    pub kind: String,
    /// Optional strength / weight / probability — semantics depend on `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
}

impl Graph {
    pub fn new(id: SemanticId, kind: impl Into<String>) -> Self {
        Self {
            id,
            kind: kind.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(&mut self, id: SemanticId) {
        if !self.nodes.contains(&id) {
            self.nodes.push(id);
        }
    }

    pub fn add_edge(&mut self, edge: GraphEdge) {
        self.add_node(edge.from.clone());
        self.add_node(edge.to.clone());
        self.edges.push(edge);
    }
}

// ─────────────────────────────────────────────────────────────────────
// Agent — decision-making entities
// ─────────────────────────────────────────────────────────────────────

/// An **Agent** is a decision-making entity — human, AI, cell, firm,
/// government, organism. Agents have beliefs (state), goals
/// (preferences), and action spaces (what they can do). The action
/// space is an enumerated list of action-IDs; the URE's AI interface
/// dispatches `Action(SemanticId)` through the agent's plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub id: SemanticId,
    pub kind: String,
    pub display_name: String,
    /// Free-form belief / state snapshot — agent's view of the world.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub beliefs: HashMap<String, PropertyValue>,
    /// Goals or objectives the agent is pursuing (free-form strings).
    #[serde(default)]
    pub goals: Vec<String>,
    /// Available actions. Each action is itself a SemanticId so it
    /// can be referenced by Events and Constraints.
    #[serde(default)]
    pub action_space: Vec<SemanticId>,
}

impl Agent {
    pub fn new(id: SemanticId, kind: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id,
            kind: kind.into(),
            display_name: display_name.into(),
            beliefs: HashMap::new(),
            goals: Vec::new(),
            action_space: Vec::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Event — things that happen
// ─────────────────────────────────────────────────────────────────────

/// An **Event** is a moment of change. Carries `at` (logical time as
/// monotonic u64), `causes` and `effects` (other Event SemanticIds —
/// the causal graph!), and an optional `payload` for domain-specific
/// data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: SemanticId,
    pub kind: String,
    pub at: u64,
    #[serde(default)]
    pub causes: Vec<SemanticId>,
    #[serde(default)]
    pub effects: Vec<SemanticId>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub payload: HashMap<String, PropertyValue>,
    /// The actor responsible for this event (Agent, system, unknown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<SemanticId>,
}

impl Event {
    pub fn new(id: SemanticId, kind: impl Into<String>, at: u64) -> Self {
        Self {
            id,
            kind: kind.into(),
            at,
            causes: Vec::new(),
            effects: Vec::new(),
            payload: HashMap::new(),
            actor: None,
        }
    }

    pub fn caused_by(mut self, cause: SemanticId) -> Self {
        self.causes.push(cause);
        self
    }
}

// ─────────────────────────────────────────────────────────────────────
// Constraint — hard, soft, regulatory, ethical, physical, probabilistic
// ─────────────────────────────────────────────────────────────────────

/// A **Constraint** governs what's possible, allowed, or risky. The
/// `severity` declares whether violation is impossible
/// (`Hard`/`Physical`), discouraged but valid (`Soft`), or
/// gate-keepered (`Regulatory`/`Ethical`/`RequiresApproval`).
///
/// The `predicate` is a free-form string for human reading + AI
/// parsing — domain plugins typically include a structured payload
/// in `expression` for machine-evaluated constraints. v0.5.0 ships
/// the type; expression evaluators land per-plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub id: SemanticId,
    pub kind: String,
    pub severity: ConstraintSeverity,
    pub predicate: String,
    /// What this constraint applies to (e.g. an Object's SemanticId).
    pub applies_to: Vec<SemanticId>,
    /// Optional machine-readable expression. v0.5.0: opaque payload.
    /// Future waves attach domain-specific AST.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub expression: HashMap<String, PropertyValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintSeverity {
    /// Cannot be violated — physics, type invariants, accounting identities.
    Hard,
    /// Physical impossibility — collision, conservation laws.
    Physical,
    /// Strong preference — performance budget, code style.
    Soft,
    /// Regulatory — must comply with law / policy.
    Regulatory,
    /// Ethical — violates a stated ethical line.
    Ethical,
    /// Probabilistic — bounded with confidence interval.
    Probabilistic,
    /// Requires human approval before applying.
    RequiresApproval,
}

impl Constraint {
    pub fn new(
        id: SemanticId,
        kind: impl Into<String>,
        severity: ConstraintSeverity,
        predicate: impl Into<String>,
    ) -> Self {
        Self {
            id,
            kind: kind.into(),
            severity,
            predicate: predicate.into(),
            applies_to: Vec::new(),
            expression: HashMap::new(),
        }
    }

    pub fn applies_to_id(mut self, id: SemanticId) -> Self {
        self.applies_to.push(id);
        self
    }
}

// ─────────────────────────────────────────────────────────────────────
// Adapters — bring the existing Wish data model into the URE substrate
// ─────────────────────────────────────────────────────────────────────

/// Walk a [`crate::WishWorld`] and emit a `Vec<Primitive>` covering
/// every entity (Object), agent (Agent), and event (Event) it holds.
/// This is the **canonical bridge** from Wish's existing semantic
/// state to the URE Five Primitives substrate.
///
/// What this adapter does NOT do:
/// - Extract `Field`s (the world doesn't carry continuous fields
///   today; field discovery is a domain-plugin concern).
/// - Extract `Graph`s from the world's relation structure (the
///   `RepoGraph → Primitive::Graph` adapter handles code; other
///   graph extractors live in their domain plugins).
/// - Extract `Constraint`s (constraints are declared by plugins,
///   not inferred from data).
///
/// Use [`primitives_from_world_with`] to add domain-specific fields,
/// graphs, and constraints.
pub fn primitives_from_world(world: &crate::WishWorld) -> Vec<Primitive> {
    let mut out = Vec::with_capacity(world.entities.len() + world.agents.len());
    for entity in world.entities.values() {
        let mut obj = Object::new(
            entity.id.clone(),
            entity_kind_to_string(&entity.kind),
            entity.display_name.clone(),
        );
        // Lift status into a property so AI can read it.
        obj = obj.with_property(
            "status",
            PropertyValue::Text(format!("{:?}", entity.status).to_lowercase()),
        );
        if let Some(src) = &entity.source_ref {
            let pos = match (src.line, src.column) {
                (Some(l), Some(c)) => format!("{}:{}:{}", src.path, l, c),
                (Some(l), None) => format!("{}:{}", src.path, l),
                _ => src.path.clone(),
            };
            obj = obj.with_property("source_ref", PropertyValue::Text(pos));
        }
        out.push(Primitive::Object(obj));
    }
    for agent in world.agents.values() {
        let a = Agent::new(
            agent.id.clone(),
            "world_agent",
            agent.display_name.clone(),
        );
        out.push(Primitive::Agent(a));
    }
    // Events are sourced from the `WorldLine` (provenance ledger),
    // not from the WishWorld struct itself. A separate adapter in
    // `wish-provenance` will produce `Primitive::Event`s from
    // `WorldLine` entries — kept out of this module to preserve the
    // wish-world-model → no-deps-on-provenance layering.
    out
}

fn entity_kind_to_string(kind: &crate::EntityKind) -> String {
    use crate::EntityKind::*;
    match kind {
        File => "file".to_string(),
        Function => "function".to_string(),
        Crate => "crate".to_string(),
        Package => "package".to_string(),
        Module => "module".to_string(),
        Service => "service".to_string(),
        Agent => "agent".to_string(),
        ToolCall => "tool_call".to_string(),
        Test => "test".to_string(),
        Commit => "commit".to_string(),
        Diff => "diff".to_string(),
        TerminalBlock => "terminal_block".to_string(),
        DocumentSection => "document_section".to_string(),
        Npc => "npc".to_string(),
        Quest => "quest".to_string(),
        SacredArchitecture => "sacred_architecture".to_string(),
        Asset => "asset".to_string(),
        Custom(s) => s.clone(),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_id::Realm;

    #[test]
    fn primitive_id_dispatches_per_variant() {
        let object_id = SemanticId::new(Realm::Code, "function", "main");
        let obj = Object::new(object_id.clone(), "rust_fn", "main");
        let p = Primitive::Object(obj);
        assert_eq!(p.id(), &object_id);
        assert_eq!(p.realm(), &Realm::Code);
    }

    #[test]
    fn object_with_typed_properties_roundtrips_json() {
        let id = SemanticId::new(Realm::Finance, "order", "42");
        let obj = Object::new(id.clone(), "limit_order", "BTC limit at 67000")
            .with_property("side", PropertyValue::Text("buy".to_string()))
            .with_property("price", PropertyValue::Number(67000.0))
            .with_property("filled", PropertyValue::Bool(false));
        let json = serde_json::to_string(&Primitive::Object(obj.clone())).unwrap();
        let back: Primitive = serde_json::from_str(&json).unwrap();
        if let Primitive::Object(o) = back {
            assert_eq!(o.id, id);
            assert_eq!(o.properties.len(), 3);
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn graph_adds_endpoints_when_edge_added() {
        let mut g = Graph::new(
            SemanticId::new(Realm::Code, "call_graph", "lib"),
            "rust_calls",
        );
        let a = SemanticId::code_function("foo::a");
        let b = SemanticId::code_function("foo::b");
        g.add_edge(GraphEdge {
            from: a.clone(),
            to: b.clone(),
            kind: "calls".to_string(),
            weight: None,
        });
        assert_eq!(g.nodes.len(), 2);
        assert!(g.nodes.contains(&a));
        assert!(g.nodes.contains(&b));
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn field_sparse_and_dense_round_trip() {
        let id = SemanticId::new(Realm::Custom("physics".into()), "field", "gravity");
        let f = Field::dense(
            id.clone(),
            "scalar",
            vec!["x".to_string(), "y".to_string()],
            vec![2, 2],
            vec![9.8, 9.8, 9.8, 9.8],
        )
        .with_unit("m/s²");
        assert_eq!(f.sample_count(), 4);
        let json = serde_json::to_string(&Primitive::Field(f.clone())).unwrap();
        let back: Primitive = serde_json::from_str(&json).unwrap();
        if let Primitive::Field(f2) = back {
            assert_eq!(f2.unit.as_deref(), Some("m/s²"));
            assert_eq!(f2.samples_dense, vec![9.8; 4]);
        }
    }

    #[test]
    fn agent_action_space_addressable_by_semantic_id() {
        let agent_id = SemanticId::agent_run("session-42");
        let action_a = SemanticId::new(Realm::Agent, "action", "open_file");
        let action_b = SemanticId::new(Realm::Agent, "action", "edit_file");
        let mut agent = Agent::new(agent_id.clone(), "ai_pair_programmer", "Wish Agent");
        agent.action_space.push(action_a.clone());
        agent.action_space.push(action_b.clone());
        assert_eq!(agent.action_space.len(), 2);
    }

    #[test]
    fn event_carries_causal_chain() {
        let trigger = SemanticId::new(Realm::Custom("finance".into()), "event", "fed_hike");
        let derived =
            SemanticId::new(Realm::Custom("finance".into()), "event", "yield_curve_invert");
        let e = Event::new(derived.clone(), "rate_move", 1_700_000_000)
            .caused_by(trigger.clone());
        assert_eq!(e.causes, vec![trigger]);
        assert_eq!(e.at, 1_700_000_000);
    }

    #[test]
    fn constraint_severity_serializes_snake_case() {
        let id = SemanticId::new(Realm::Custom("biology".into()), "constraint", "no_germline_edit");
        let c = Constraint::new(
            id,
            "regulation",
            ConstraintSeverity::Ethical,
            "human germline edits are not permitted",
        );
        let json = serde_json::to_string(&Primitive::Constraint(c)).unwrap();
        assert!(json.contains("\"severity\":\"ethical\""));
    }

    #[test]
    fn five_primitives_serialize_with_discriminator_tag() {
        let prims = vec![
            Primitive::Object(Object::new(
                SemanticId::code_file("src/main.rs"),
                "rust_file",
                "main.rs",
            )),
            Primitive::Field(Field::dense(
                SemanticId::new(Realm::Custom("physics".into()), "field", "temp"),
                "scalar",
                vec!["x".into()],
                vec![1],
                vec![300.0],
            )),
            Primitive::Graph(Graph::new(
                SemanticId::new(Realm::Code, "graph", "deps"),
                "crate_deps",
            )),
            Primitive::Agent(Agent::new(
                SemanticId::agent_run("a"),
                "ai",
                "Wish Agent",
            )),
            Primitive::Event(Event::new(
                SemanticId::new(Realm::Code, "event", "edit"),
                "file_edit",
                1,
            )),
        ];
        let json = serde_json::to_string(&prims).unwrap();
        assert!(json.contains("\"primitive\":\"object\""));
        assert!(json.contains("\"primitive\":\"field\""));
        assert!(json.contains("\"primitive\":\"graph\""));
        assert!(json.contains("\"primitive\":\"agent\""));
        assert!(json.contains("\"primitive\":\"event\""));
        // Roundtrip
        let back: Vec<Primitive> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 5);
    }

    #[test]
    fn property_value_supports_ref_to_semantic_id() {
        let other = SemanticId::code_file("other.rs");
        let pv = PropertyValue::Ref(Box::new(other.clone()));
        let json = serde_json::to_string(&pv).unwrap();
        let back: PropertyValue = serde_json::from_str(&json).unwrap();
        if let PropertyValue::Ref(id) = back {
            assert_eq!(*id, other);
        } else {
            panic!("expected Ref variant");
        }
    }

    #[test]
    fn adapter_wishworld_to_primitives_lifts_entities_and_agents() {
        use crate::{EntityKind, WishWorld, WorldEntity, WorldKind};

        let mut world = WishWorld::new("test", WorldKind::GenericProject);
        world.upsert_entity(WorldEntity::stub(
            SemanticId::code_file("src/lib.rs"),
            "lib.rs",
            EntityKind::File,
        ));
        world.upsert_entity(WorldEntity::stub(
            SemanticId::code_function("module::main"),
            "main",
            EntityKind::Function,
        ));
        // 2 entities → 2 Object primitives.
        let prims = primitives_from_world(&world);
        let object_count = prims
            .iter()
            .filter(|p| matches!(p, Primitive::Object(_)))
            .count();
        assert_eq!(object_count, 2);
        // Every primitive's id().realm() is reachable.
        for p in &prims {
            assert_eq!(p.realm(), &Realm::Code);
        }
    }

    #[test]
    fn entity_kind_to_string_handles_custom() {
        use crate::EntityKind;
        assert_eq!(entity_kind_to_string(&EntityKind::File), "file");
        assert_eq!(entity_kind_to_string(&EntityKind::Function), "function");
        assert_eq!(
            entity_kind_to_string(&EntityKind::Custom("legal_contract".into())),
            "legal_contract"
        );
    }

    #[test]
    fn constraint_applies_to_chained() {
        let target = SemanticId::new(Realm::Finance, "portfolio", "main");
        let c = Constraint::new(
            SemanticId::new(Realm::Finance, "constraint", "var_limit"),
            "regulation",
            ConstraintSeverity::Regulatory,
            "VaR ≤ 1M USD",
        )
        .applies_to_id(target.clone());
        assert_eq!(c.applies_to, vec![target]);
    }
}
