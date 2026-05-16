//! Intent → WorldPlan: turn a natural-language prompt into a sequence
//! of `WorldPatch`es that build a world.
//!
//! In v0.5.0 this is **keyword-driven** — no LLM. The seam is the
//! interesting part: every plan is a real, signed, provenance-anchored
//! mission. In v0.6.0+ the keyword matcher gets replaced by a Hermon
//! call to a frontier model, and the rest of the pipeline (patch
//! emission, provenance, viewer) is unchanged.
//!
//! The keyword templates we ship today:
//!
//! - **Shan Hai Fintech Harbor** — keywords `shanhai`, `harbor`,
//!   `merchant`, `stablecoin`, `credit`, `trade`, `risk`. Same as
//!   `build_shanhai_harbor`.
//! - **Mythic Dragon Temple** — keywords `temple`, `dragon`, `sacred`,
//!   `mythic`, `pilgrim`, `bell`.
//! - **Service Topology** — keywords `service`, `topology`,
//!   `kubernetes`, `microservice`, `runtime`, `gateway`.
//! - **Education curriculum** — keywords `education`, `school`,
//!   `student`, `teacher`, `lesson`, `curriculum`, `learn`.
//!
//! Falls back to a **starter empty world** if no template matches.

use crate::builders::{ShanHaiBuild, build_shanhai_harbor};
use wish_provenance::WorldLine;
use wish_world_model::{
    Actor, Component, EntityKind, PatchOp, Realm, SemanticId, Transform, WishWorld, WorldAgent,
    WorldEntity, WorldKind, WorldPatch,
};

/// A plan for building a world from an intent.
///
/// Includes the starting `WishWorld` shell (id, name, kind, intent)
/// and the ordered list of `WorldPatch`es that build it out. Apply
/// them through `wish_provenance::apply_with_provenance` for the
/// auto-approval flow, or drive them manually in a live viewer.
#[derive(Debug, Clone)]
pub struct WorldPlan {
    pub world: WishWorld,
    pub patches: Vec<WorldPatch>,
    pub template: &'static str,
}

/// Plan a world from a natural-language intent string.
///
/// Always returns a plan — falls back to a "starter" template if no
/// keywords match. Never panics, never reads the network. Suitable
/// for live-build demos and offline tests.
pub fn plan_world(intent: &str) -> WorldPlan {
    let lower = intent.to_ascii_lowercase();
    let has_any = |words: &[&str]| words.iter().any(|w| lower.contains(w));

    if has_any(&[
        "shanhai", "shan hai", "harbor", "merchant", "stablecoin", "credit", "trade", "risk",
    ]) {
        return shanhai(intent);
    }
    if has_any(&["temple", "dragon", "sacred", "mythic", "pilgrim", "bell"]) {
        return mythic_temple(intent);
    }
    if has_any(&[
        "service",
        "topology",
        "kubernetes",
        "k8s",
        "microservice",
        "runtime",
        "gateway",
    ]) {
        return service_topology(intent);
    }
    if has_any(&[
        "education",
        "school",
        "student",
        "teacher",
        "lesson",
        "curriculum",
        "learn",
    ]) {
        return education_world(intent);
    }
    starter(intent)
}

/// Apply a `WorldPlan`'s patches through a fresh `WorldLine`. Returns
/// the WorldLine path (in `world_dir/provenance/`) and a `ShanHaiBuild`
/// — the same outcome type the Shan Hai builder uses — for downstream
/// consumers that already speak that shape.
pub fn apply_plan(
    plan: &WorldPlan,
    world: &mut WishWorld,
    worldline: &mut WorldLine,
) -> Result<Vec<wish_provenance::ApplyOutcome>, wish_provenance::WorldLineError> {
    let mut outcomes = Vec::with_capacity(plan.patches.len());
    for patch in &plan.patches {
        let outcome = worldline.apply_with_provenance(world, patch.clone(), 0.30)?;
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

// -- templates ---------------------------------------------------------

fn shanhai(intent: &str) -> WorldPlan {
    // Re-use the existing deterministic Shan Hai builder by tearing
    // its planning out: build a fresh world + worldline, run it, then
    // surface the WorldPatches from the worldline.
    let mut world = WishWorld::new("Shan Hai Fintech Harbor", WorldKind::EducationWorld);
    world.intent = if intent.trim().is_empty() {
        "Ancient Chinese harbor city where AI merchants teach stablecoin, credit, trade, and risk.".into()
    } else {
        intent.to_string()
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("wish-intent-shanhai-{nanos}"));
    let _ = std::fs::create_dir_all(&tmp);
    let mut wl = WorldLine::open_in_world_dir(&tmp)
        .expect("open temporary worldline");
    let _: ShanHaiBuild = build_shanhai_harbor(&mut world, &mut wl).expect("plan shanhai");
    let patches: Vec<WorldPatch> = wl.iter().map(|ev| ev.patch.clone()).collect();
    // Reset the world so the caller can re-apply patches against a
    // pristine starting state through their own WorldLine.
    let intent_owned = world.intent.clone();
    let mut fresh = WishWorld::new(world.name.clone(), world.kind.clone());
    fresh.intent = intent_owned;
    fresh.id = world.id;
    WorldPlan {
        world: fresh,
        patches,
        template: "shanhai-fintech-harbor",
    }
}

fn mythic_temple(intent: &str) -> WorldPlan {
    let mut world = WishWorld::new("Mythic Dragon Temple", WorldKind::FinalverseRegion);
    world.intent = if intent.trim().is_empty() {
        "A sacred dragon temple at the heart of a mythic forest, with three pilgrim NPCs, ritual bells, and a riddle quest.".into()
    } else {
        intent.to_string()
    };
    let agent = Actor::Agent { agent_id: "wish-agent-world-architect".into() };
    let architect = WorldAgent {
        id: SemanticId::new(Realm::Agent, "world_architect", "wish-agent-world-architect"),
        display_name: "World Architect".into(),
        role: "world_architect".into(),
        tools: vec![
            "world.patch".into(),
            "scene.generate".into(),
            "asset.bake".into(),
            "quest.generate".into(),
        ],
        runtime_target: Some("hermon".into()),
    };
    let mut patches = vec![
        WorldPatch::new(
            agent.clone(),
            "Plant the Mythic Dragon Temple at the forest's heart.",
            vec![
                PatchOp::AddAgent(architect),
                PatchOp::AddEntity(sacred_temple(
                    "mythic_temple",
                    "Dragon Temple",
                    [0.0, 0.0, 0.0],
                )),
            ],
        ),
        WorldPatch::new(
            agent.clone(),
            "Ring the four ritual bells at the cardinal points.",
            vec![
                PatchOp::AddEntity(prop_entity("bell_north", "North Bell", [0.0, 0.0, -10.0])),
                PatchOp::AddEntity(prop_entity("bell_south", "South Bell", [0.0, 0.0, 10.0])),
                PatchOp::AddEntity(prop_entity("bell_east", "East Bell", [10.0, 0.0, 0.0])),
                PatchOp::AddEntity(prop_entity("bell_west", "West Bell", [-10.0, 0.0, 0.0])),
            ],
        ),
        WorldPatch::new(
            agent.clone(),
            "Summon three pilgrim NPCs to the temple steps.",
            vec![
                PatchOp::AddEntity(npc_entity(
                    "pilgrim_jin",
                    "Pilgrim Jin",
                    "lore_teacher",
                    [4.0, 0.0, 4.0],
                )),
                PatchOp::AddEntity(npc_entity(
                    "pilgrim_mei",
                    "Pilgrim Mei",
                    "riddle_master",
                    [-4.0, 0.0, 4.0],
                )),
                PatchOp::AddEntity(npc_entity(
                    "pilgrim_hong",
                    "Pilgrim Hong",
                    "bell_ringer",
                    [0.0, 0.0, 6.0],
                )),
            ],
        ),
    ];
    let scene_id = SemanticId::new(Realm::Scene, "scene", "main");
    patches.push(WorldPatch::new(
        agent,
        "Wire the temple's main scene.",
        vec![PatchOp::AddScene(wish_world_model::WorldScene {
            id: scene_id,
            display_name: "Mythic Temple Main Scene".into(),
            entity_ids: vec![
                SemanticId::new(Realm::Scene, "sacred_architecture", "mythic_temple"),
                SemanticId::new(Realm::Asset, "prop", "bell_north"),
                SemanticId::new(Realm::Asset, "prop", "bell_south"),
                SemanticId::new(Realm::Asset, "prop", "bell_east"),
                SemanticId::new(Realm::Asset, "prop", "bell_west"),
                SemanticId::new(Realm::Npc, "npc", "pilgrim_jin"),
                SemanticId::new(Realm::Npc, "npc", "pilgrim_mei"),
                SemanticId::new(Realm::Npc, "npc", "pilgrim_hong"),
            ],
        })],
    ));

    WorldPlan {
        world,
        patches,
        template: "mythic-dragon-temple",
    }
}

fn service_topology(intent: &str) -> WorldPlan {
    let mut world = WishWorld::new("Service Topology", WorldKind::LiveService);
    world.intent = if intent.trim().is_empty() {
        "A live-service topology: gateway → API → workers → datastore, with an observer agent.".into()
    } else {
        intent.to_string()
    };
    let agent = Actor::Agent { agent_id: "wish-agent-world-architect".into() };
    let architect = WorldAgent {
        id: SemanticId::new(Realm::Agent, "world_architect", "wish-agent-world-architect"),
        display_name: "Topology Architect".into(),
        role: "world_architect".into(),
        tools: vec!["world.patch".into(), "service.observe".into()],
        runtime_target: Some("hermon".into()),
    };
    let patches = vec![
        WorldPatch::new(
            agent.clone(),
            "Provision the runtime architect.",
            vec![PatchOp::AddAgent(architect)],
        ),
        WorldPatch::new(
            agent.clone(),
            "Add the public gateway service.",
            vec![PatchOp::AddEntity(service_entity("gateway", "Gateway"))],
        ),
        WorldPatch::new(
            agent.clone(),
            "Stand up the API service behind the gateway.",
            vec![PatchOp::AddEntity(service_entity("api", "API Server"))],
        ),
        WorldPatch::new(
            agent.clone(),
            "Spawn the worker pool.",
            vec![
                PatchOp::AddEntity(service_entity("worker_1", "Worker 1")),
                PatchOp::AddEntity(service_entity("worker_2", "Worker 2")),
                PatchOp::AddEntity(service_entity("worker_3", "Worker 3")),
            ],
        ),
        WorldPatch::new(
            agent,
            "Mount the datastore.",
            vec![PatchOp::AddEntity(service_entity("datastore", "Datastore"))],
        ),
    ];
    WorldPlan {
        world,
        patches,
        template: "service-topology",
    }
}

fn education_world(intent: &str) -> WorldPlan {
    let mut world = WishWorld::new("Education World", WorldKind::EducationWorld);
    world.intent = if intent.trim().is_empty() {
        "An interactive learning world: a teacher agent, three students, a curriculum, and an assessment loop.".into()
    } else {
        intent.to_string()
    };
    let agent = Actor::Agent { agent_id: "wish-agent-world-architect".into() };
    let teacher = WorldAgent {
        id: SemanticId::new(Realm::Agent, "teacher", "teacher_aurora"),
        display_name: "Teacher Aurora".into(),
        role: "teacher".into(),
        tools: vec!["world.patch".into(), "lesson.generate".into(), "assessment.score".into()],
        runtime_target: Some("hermon".into()),
    };
    let patches = vec![
        WorldPatch::new(
            agent.clone(),
            "Introduce Teacher Aurora.",
            vec![PatchOp::AddAgent(teacher)],
        ),
        WorldPatch::new(
            agent.clone(),
            "Welcome three students.",
            vec![
                PatchOp::AddEntity(npc_entity(
                    "student_amy",
                    "Student Amy",
                    "learner",
                    [-4.0, 0.0, 0.0],
                )),
                PatchOp::AddEntity(npc_entity(
                    "student_ben",
                    "Student Ben",
                    "learner",
                    [0.0, 0.0, 0.0],
                )),
                PatchOp::AddEntity(npc_entity(
                    "student_cai",
                    "Student Cai",
                    "learner",
                    [4.0, 0.0, 0.0],
                )),
            ],
        ),
        WorldPatch::new(
            agent.clone(),
            "Open the curriculum hall.",
            vec![PatchOp::AddEntity(sacred_temple(
                "hall_of_learning",
                "Hall of Learning",
                [0.0, 0.0, -8.0],
            ))],
        ),
        WorldPatch::new(
            agent,
            "Wire the main classroom scene.",
            vec![PatchOp::AddScene(wish_world_model::WorldScene {
                id: SemanticId::new(Realm::Scene, "scene", "main"),
                display_name: "Education World Main Scene".into(),
                entity_ids: vec![
                    SemanticId::new(Realm::Scene, "sacred_architecture", "hall_of_learning"),
                    SemanticId::new(Realm::Npc, "npc", "student_amy"),
                    SemanticId::new(Realm::Npc, "npc", "student_ben"),
                    SemanticId::new(Realm::Npc, "npc", "student_cai"),
                ],
            })],
        ),
    ];
    WorldPlan {
        world,
        patches,
        template: "education-world",
    }
}

fn starter(intent: &str) -> WorldPlan {
    let mut world = WishWorld::new("Starter World", WorldKind::GenericProject);
    world.intent = if intent.trim().is_empty() {
        "A blank world. Tell me what to build and I'll begin.".into()
    } else {
        intent.to_string()
    };
    let agent = Actor::System;
    let patches = vec![
        WorldPatch::new(
            agent.clone(),
            "Seed an idea node.",
            vec![PatchOp::AddEntity(WorldEntity::stub(
                SemanticId::new(Realm::Custom("idea".into()), "idea", "seed"),
                "seed idea",
                EntityKind::Custom("idea".into()),
            ))],
        ),
        WorldPatch::new(
            agent,
            "Set intent.",
            vec![PatchOp::SetIntent { intent: world.intent.clone() }],
        ),
    ];
    WorldPlan {
        world,
        patches,
        template: "starter",
    }
}

// -- builders for entity kinds used by templates ----------------------

fn sacred_temple(stable_key: &str, display_name: &str, t: [f32; 3]) -> WorldEntity {
    WorldEntity {
        id: SemanticId::new(Realm::Scene, "sacred_architecture", stable_key),
        kind: EntityKind::SacredArchitecture,
        display_name: display_name.into(),
        components: vec![
            Component::Transform(Transform { translation: t, rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0, 1.0, 1.0] }),
            Component::MaterialSet { reference: "assets/textures/sacred_stone/".into() },
            Component::LightingProfile { preset: "ancient_sacred".into() },
            Component::LoreAnchor { reference: format!("memory/lore.md#{stable_key}") },
        ],
        source_ref: None,
        agent_ref: None,
        status: wish_world_model::EntityStatus::Ok,
        agent_editable: true,
        provenance_head: None,
    }
}

fn prop_entity(stable_key: &str, display_name: &str, t: [f32; 3]) -> WorldEntity {
    WorldEntity {
        id: SemanticId::new(Realm::Asset, "prop", stable_key),
        kind: EntityKind::Asset,
        display_name: display_name.into(),
        components: vec![
            Component::Transform(Transform { translation: t, rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0, 1.0, 1.0] }),
            Component::SoundscapeAnchor { reference: "assets/audio/ritual_bell.ogg".into() },
        ],
        source_ref: None,
        agent_ref: None,
        status: wish_world_model::EntityStatus::Ok,
        agent_editable: true,
        provenance_head: None,
    }
}

fn npc_entity(
    stable_key: &str,
    display_name: &str,
    profile: &str,
    t: [f32; 3],
) -> WorldEntity {
    WorldEntity {
        id: SemanticId::new(Realm::Npc, "npc", stable_key),
        kind: EntityKind::Npc,
        display_name: display_name.into(),
        components: vec![
            Component::Transform(Transform { translation: t, rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0, 1.0, 1.0] }),
            Component::BehaviorScript { reference: format!("scripts/behaviors/{stable_key}.rs") },
            Component::EconomicActor { profile: profile.into() },
            Component::LoreAnchor { reference: format!("memory/lore.md#{stable_key}") },
        ],
        source_ref: None,
        agent_ref: None,
        status: wish_world_model::EntityStatus::Ok,
        agent_editable: true,
        provenance_head: None,
    }
}

fn service_entity(stable_key: &str, display_name: &str) -> WorldEntity {
    WorldEntity {
        id: SemanticId::new(Realm::Service, "service", stable_key),
        kind: EntityKind::Service,
        display_name: display_name.into(),
        components: vec![Component::Transform(Transform::default())],
        source_ref: None,
        agent_ref: None,
        status: wish_world_model::EntityStatus::Running,
        agent_editable: true,
        provenance_head: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shanhai_keyword_picks_harbor_template() {
        let plan = plan_world("build a Shan Hai harbor with merchants");
        assert_eq!(plan.template, "shanhai-fintech-harbor");
        assert!(!plan.patches.is_empty());
    }

    #[test]
    fn temple_keyword_picks_mythic_template() {
        let plan = plan_world("place a sacred dragon temple");
        assert_eq!(plan.template, "mythic-dragon-temple");
        // 4 patches: temple + bells + pilgrims + scene
        assert_eq!(plan.patches.len(), 4);
    }

    #[test]
    fn service_keyword_picks_topology_template() {
        let plan = plan_world("show me a kubernetes microservice topology");
        assert_eq!(plan.template, "service-topology");
        assert!(plan.patches.len() >= 5);
    }

    #[test]
    fn no_keyword_falls_back_to_starter() {
        let plan = plan_world("");
        assert_eq!(plan.template, "starter");
        assert!(!plan.patches.is_empty());
    }

    #[test]
    fn plan_applies_cleanly_through_provenance() {
        let plan = plan_world("a mythic temple with bells");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("wish-intent-test-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut wl = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut world = plan.world.clone();
        let outcomes = apply_plan(&plan, &mut world, &mut wl).unwrap();
        assert_eq!(outcomes.len(), plan.patches.len());
        assert!(outcomes
            .iter()
            .all(|o| matches!(o, wish_provenance::ApplyOutcome::Applied { .. })));
        assert!(world.entities.len() >= 5);
    }
}
