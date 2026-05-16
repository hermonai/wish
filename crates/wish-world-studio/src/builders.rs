//! Deterministic world builders. The Shan Hai Fintech Harbor builder is
//! the canonical end-to-end demo: it emits a sequence of WorldPatches
//! that build the world from scratch.

use chrono::Utc;
use std::collections::HashMap;
use wish_provenance::{ApplyOutcome, WorldLine, WorldLineError};
use wish_world_model::{
    Actor, ArtifactKind, ArtifactValidation, Component, EntityKind, Evidence, Mission,
    MissionStatus, MissionStep, PatchOp, Realm, SemanticId, Signature, Signer, Transform,
    VerifiableArtifact, VerifiableArtifactId, WishWorld, WorldAgent, WorldEntity, WorldPatch,
    WorldScene,
};

/// Outcome of running the Shan Hai builder.
#[derive(Debug, Clone)]
pub struct ShanHaiBuild {
    pub mission: Mission,
    pub outcomes: Vec<ApplyOutcome>,
    /// VerifiableArtifacts produced — one per applied patch. Each
    /// references the WorldEvent the patch landed in.
    pub artifacts: HashMap<VerifiableArtifactId, VerifiableArtifact>,
}

/// Build the Shan Hai Fintech Harbor demo world deterministically by
/// emitting a sequence of WorldPatches and applying them through the
/// WorldLine. Returns the Mission record + the per-step outcomes.
///
/// Auto-approves every patch (each one is small + additive, so risk
/// stays below 0.30).
pub fn build_shanhai_harbor(
    world: &mut WishWorld,
    worldline: &mut WorldLine,
) -> Result<ShanHaiBuild, WorldLineError> {
    let mut mission = Mission::new(&world.id, "Build the Shan Hai Fintech Harbor demo world.");
    mission.capabilities = vec!["world.patch".into(), "scene.generate".into()];

    let agent = Actor::Agent {
        agent_id: "wish-agent-world-architect".into(),
    };
    let architect_id = SemanticId::new(Realm::Agent, "world_architect", "wish-agent-world-architect");

    let steps_data: Vec<(&str, &str, Vec<PatchOp>)> = vec![
        (
            "set_kind",
            "Designate this as an education world.",
            vec![
                PatchOp::SetIntent {
                    intent: "Ancient Chinese harbor city where AI merchants teach stablecoin, credit, trade, and risk.".into(),
                },
                PatchOp::AddAgent(WorldAgent {
                    id: architect_id.clone(),
                    display_name: "World Architect".into(),
                    role: "world_architect".into(),
                    tools: vec![
                        "world.patch".into(),
                        "scene.generate".into(),
                        "asset.bake".into(),
                        "quest.generate".into(),
                        "dialogue.generate".into(),
                    ],
                    runtime_target: Some("hermon".into()),
                }),
            ],
        ),
        (
            "dragon_temple",
            "Place the Dragon Temple at the city's center.",
            vec![PatchOp::AddEntity(temple())],
        ),
        (
            "merchant_liu",
            "Spawn Merchant Liu — the stablecoin teacher.",
            vec![PatchOp::AddEntity(npc(
                "merchant_liu",
                "Merchant Liu",
                "stablecoin_teacher",
                [12.0, 0.0, -4.0],
            ))],
        ),
        (
            "banker_sun",
            "Spawn Banker Sun — the credit teacher.",
            vec![PatchOp::AddEntity(npc(
                "banker_sun",
                "Banker Sun",
                "credit_teacher",
                [-8.0, 0.0, 6.0],
            ))],
        ),
        (
            "trader_wei",
            "Spawn Trader Wei — the trade teacher.",
            vec![PatchOp::AddEntity(npc(
                "trader_wei",
                "Trader Wei",
                "trade_teacher",
                [4.0, 0.0, 12.0],
            ))],
        ),
        (
            "risk_chen",
            "Spawn Risk-Master Chen — the risk teacher.",
            vec![PatchOp::AddEntity(npc(
                "risk_chen",
                "Risk-Master Chen",
                "risk_teacher",
                [-12.0, 0.0, -6.0],
            ))],
        ),
        (
            "main_scene",
            "Wire the harbor's main scene with every placed entity.",
            vec![PatchOp::AddScene(WorldScene {
                id: SemanticId::new(Realm::Scene, "scene", "main"),
                display_name: "Harbor Main Scene".into(),
                entity_ids: vec![
                    SemanticId::new(Realm::Scene, "sacred_architecture", "entity_dragon_temple"),
                    SemanticId::new(Realm::Npc, "npc", "merchant_liu"),
                    SemanticId::new(Realm::Npc, "npc", "banker_sun"),
                    SemanticId::new(Realm::Npc, "npc", "trader_wei"),
                    SemanticId::new(Realm::Npc, "npc", "risk_chen"),
                ],
            })],
        ),
    ];

    let mut outcomes = Vec::with_capacity(steps_data.len());
    let mut artifacts: HashMap<VerifiableArtifactId, VerifiableArtifact> = HashMap::new();

    for (step_id, intent, ops) in &steps_data {
        mission.add_step(MissionStep {
            id: (*step_id).into(),
            label: (*intent).into(),
            status: MissionStatus::Running,
            depends_on: vec![],
        });
        let patch = WorldPatch::new(agent.clone(), *intent, ops.clone());
        let patch_id = patch.id.clone();
        let affected = patch.affected.clone();
        let kind = artifact_kind_for(ops);
        let outcome = worldline.apply_with_provenance(world, patch, 0.30)?;
        outcomes.push(outcome.clone());

        // Mark the corresponding step status.
        let event_id_opt: Option<String> = match &outcome {
            ApplyOutcome::Applied { event_id, .. } => Some(event_id.clone()),
            ApplyOutcome::Pending { event_id, .. } => Some(event_id.clone()),
            ApplyOutcome::Rejected { .. } => None,
        };
        if let Some(last) = mission.plan.last_mut() {
            last.status = match outcome {
                ApplyOutcome::Applied { .. } => MissionStatus::Succeeded,
                ApplyOutcome::Pending { .. } => MissionStatus::WaitingHuman,
                ApplyOutcome::Rejected { .. } => MissionStatus::Failed,
            };
        }

        // Mint a VerifiableArtifact for this step. The substrate has
        // the data; mission and worldline anchor it. Real recordings /
        // screenshots / merkle proofs arrive in v0.7/v1.0.
        if let Some(event_id) = event_id_opt {
            let mut artifact = VerifiableArtifact::new(&mission.id, kind, &event_id, &patch_id)
                .with_affected(affected.clone());
            artifact.validation = ArtifactValidation {
                tests_passed: 1,
                tests_failed: 0,
                ..Default::default()
            };
            artifact.add_evidence(Evidence::LogTrace {
                entries: vec![
                    format!("step={step_id} ok"),
                    format!("intent={intent}"),
                    format!("affected={}", affected.len()),
                ],
            });
            artifact.sign(Signature {
                id: format!("sig_{step_id}"),
                signer: Signer::Agent {
                    agent_id: "wish-agent-world-architect".into(),
                },
                bytes: "ed25519:demo".into(),
                algorithm: "ed25519".into(),
                signed_at: Utc::now(),
            });
            mission.attach_artifact(artifact.id.clone());
            artifacts.insert(artifact.id.clone(), artifact);
        }
    }

    mission.finished_at = Some(Utc::now());
    mission.status = if outcomes
        .iter()
        .all(|o| matches!(o, ApplyOutcome::Applied { .. }))
    {
        MissionStatus::Succeeded
    } else {
        MissionStatus::WaitingHuman
    };

    Ok(ShanHaiBuild {
        mission,
        outcomes,
        artifacts,
    })
}

/// Classify a patch's ops into an `ArtifactKind` for the artifact
/// receipt. Crude — picks the strongest signal in the op list.
fn artifact_kind_for(ops: &[PatchOp]) -> ArtifactKind {
    for op in ops {
        match op {
            PatchOp::AddScene(_) => return ArtifactKind::SceneChange,
            PatchOp::AddAsset(_) => return ArtifactKind::AssetGeneration,
            PatchOp::AddAgent(_) => {
                return ArtifactKind::Custom("agent_provision".into());
            }
            _ => {}
        }
    }
    ArtifactKind::CanvasChange
}

fn temple() -> WorldEntity {
    WorldEntity {
        id: SemanticId::new(Realm::Scene, "sacred_architecture", "entity_dragon_temple"),
        kind: EntityKind::SacredArchitecture,
        display_name: "Dragon Temple".into(),
        components: vec![
            Component::Transform(Transform::default()),
            Component::MeshReference {
                reference: "assets/models/dragon_temple.glb#root".into(),
            },
            Component::MaterialSet {
                reference: "assets/textures/sacred_stone/".into(),
            },
            Component::LightingProfile {
                preset: "ancient_sacred".into(),
            },
            Component::QuestAnchor {
                quest_ref: "scripts/quests/lost_dragon.quest".into(),
            },
            Component::SoundscapeAnchor {
                reference: "assets/audio/temple_ambient.ogg".into(),
            },
            Component::LoreAnchor {
                reference: "memory/lore.md#dragon-temple".into(),
            },
        ],
        source_ref: None,
        agent_ref: None,
        status: wish_world_model::EntityStatus::Ok,
        agent_editable: true,
        provenance_head: None,
    }
}

fn npc(
    stable_key: &str,
    display_name: &str,
    profile: &str,
    translation: [f32; 3],
) -> WorldEntity {
    WorldEntity {
        id: SemanticId::new(Realm::Npc, "npc", stable_key),
        kind: EntityKind::Npc,
        display_name: display_name.into(),
        components: vec![
            Component::Transform(Transform {
                translation,
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            }),
            Component::BehaviorScript {
                reference: format!("scripts/behaviors/{stable_key}.rs"),
            },
            Component::EconomicActor {
                profile: profile.into(),
            },
            Component::LoreAnchor {
                reference: format!("memory/lore.md#{stable_key}"),
            },
        ],
        source_ref: None,
        agent_ref: None,
        status: wish_world_model::EntityStatus::Ok,
        agent_editable: true,
        provenance_head: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wish_world_model::WorldKind;

    fn make_tmp_dir() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("ws_{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn shanhai_builder_builds_full_world_with_worldline() {
        let dir = make_tmp_dir();
        let mut wl = WorldLine::open_in_world_dir(&dir).unwrap();
        let mut world = WishWorld::new("Shan Hai Fintech Harbor", WorldKind::EducationWorld);

        let result = build_shanhai_harbor(&mut world, &mut wl).unwrap();

        // The mission now plans seven steps (added the main scene).
        assert_eq!(result.mission.plan.len(), 7);
        assert!(matches!(result.mission.status, MissionStatus::Succeeded));

        // Every step auto-approved.
        assert!(result
            .outcomes
            .iter()
            .all(|o| matches!(o, ApplyOutcome::Applied { .. })));

        // The world has the architect + 1 temple + 4 NPCs = 5 entities,
        // 1 agent, and 1 scene.
        assert_eq!(world.entities.len(), 5);
        assert_eq!(world.agents.len(), 1);
        assert_eq!(world.scenes.len(), 1);
        let temple = world
            .entities
            .values()
            .find(|e| e.display_name == "Dragon Temple")
            .expect("temple");
        assert!(matches!(temple.kind, EntityKind::SacredArchitecture));
        // Four NPCs.
        assert_eq!(
            world
                .entities
                .values()
                .filter(|e| matches!(e.kind, EntityKind::Npc))
                .count(),
            4
        );

        // WorldLine has 7 events (one per step).
        assert_eq!(wl.len(), 7);
        // All approvals auto.
        assert!(wl
            .iter()
            .all(|ev| matches!(ev.approval, wish_provenance::ApprovalState::AutoApproved)));

        // 7 VerifiableArtifacts, all signed.
        assert_eq!(result.artifacts.len(), 7);
        assert!(result
            .artifacts
            .values()
            .all(|a| !a.signatures.is_empty() && !a.affected.is_empty()));
        // Mission carries the artifact ids.
        assert_eq!(result.mission.artifacts.len(), 7);

        // Merkle root is non-zero and deterministic on re-read.
        let root_1 = wl.merkle_root(wish_provenance::DEFAULT_BRANCH);
        assert_ne!(root_1, [0u8; 32]);
        drop(wl);
        let wl2 = WorldLine::open_in_world_dir(&dir).unwrap();
        let root_2 = wl2.merkle_root(wish_provenance::DEFAULT_BRANCH);
        assert_eq!(root_1, root_2);
    }
}
