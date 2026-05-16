//! `.wishworld/` directory format — read and write.
//!
//! See `wish-design/wish-plan-20260514/04-data-model/02-wishworld-format.md`
//! for the on-disk layout.
//!
//! v0.5.0 ships a working reader and writer for the core surfaces:
//! - `world.json` (top-level manifest)
//! - `entities/*.entity.json`
//! - `scenes/*.scene.json`
//! - `agents/world_agents/*.agent.json`
//! - `assets/index.json` (catalog only; binary assets are referenced)
//! - `missions/*.mission.json`
//! - `missions/<id>/artifacts/*.artifact.json`
//! - `provenance/worldline.jsonl` (handled by `wish-provenance`, but we
//!   write the tail when present in memory)
//!
//! Reader is liberal — missing directories are treated as empty.
//! Writer is strict — every entity / scene / agent gets its own file.

use crate::mission::{Mission, MissionId, VerifiableArtifact, VerifiableArtifactId};
use crate::semantic_id::SemanticId;
use crate::world::{WishWorld, WorldAgent, WorldAsset, WorldEntity, WorldEvent, WorldScene};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WishWorldIoError {
    #[error("io error at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("json error at {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("invalid world directory: {0}")]
    Invalid(String),
    #[error("schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: String, found: String },
}

fn io_err<P: AsRef<Path>>(path: P) -> impl FnOnce(io::Error) -> WishWorldIoError {
    let p = path.as_ref().to_path_buf();
    move |source| WishWorldIoError::Io { path: p, source }
}

fn json_err<P: AsRef<Path>>(path: P) -> impl FnOnce(serde_json::Error) -> WishWorldIoError {
    let p = path.as_ref().to_path_buf();
    move |source| WishWorldIoError::Json { path: p, source }
}

/// What's loaded from a `.wishworld/` directory. The core world plus its
/// missions and artifacts (which live in `missions/` and are not part of
/// the canonical `WishWorld` struct).
#[derive(Debug, Clone, Default)]
pub struct WishWorldBundle {
    pub world: WishWorld,
    pub missions: HashMap<MissionId, Mission>,
    pub artifacts: HashMap<VerifiableArtifactId, VerifiableArtifact>,
}

/// Read a `.wishworld/` directory into a `WishWorldBundle`.
pub fn read_world_dir(dir: impl AsRef<Path>) -> Result<WishWorldBundle, WishWorldIoError> {
    let dir = dir.as_ref();
    if !dir.is_dir() {
        return Err(WishWorldIoError::Invalid(format!(
            "{} is not a directory",
            dir.display()
        )));
    }

    let manifest_path = dir.join("world.json");
    let manifest_bytes = fs::read(&manifest_path).map_err(io_err(&manifest_path))?;
    let mut world: WishWorld =
        serde_json::from_slice(&manifest_bytes).map_err(json_err(&manifest_path))?;

    // Reset collections that live in their own files. The manifest may
    // pre-populate them for compatibility, but the per-file directories
    // are authoritative when present.
    let entities_dir = dir.join("entities");
    if entities_dir.is_dir() {
        world.entities.clear();
        for entry in read_dir(&entities_dir)? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(io_err(&path))?;
            let entity: WorldEntity = serde_json::from_slice(&bytes).map_err(json_err(&path))?;
            world.entities.insert(entity.id.to_string(), entity);
        }
    }

    let scenes_dir = dir.join("scenes");
    if scenes_dir.is_dir() {
        world.scenes.clear();
        for entry in read_dir(&scenes_dir)? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(io_err(&path))?;
            let scene: WorldScene = serde_json::from_slice(&bytes).map_err(json_err(&path))?;
            world.scenes.insert(scene.id.to_string(), scene);
        }
    }

    let agents_dir = dir.join("agents").join("world_agents");
    if agents_dir.is_dir() {
        world.agents.clear();
        for entry in read_dir(&agents_dir)? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(io_err(&path))?;
            let agent: WorldAgent = serde_json::from_slice(&bytes).map_err(json_err(&path))?;
            world.agents.insert(agent.id.to_string(), agent);
        }
    }

    let assets_index = dir.join("assets").join("index.json");
    if assets_index.is_file() {
        let bytes = fs::read(&assets_index).map_err(io_err(&assets_index))?;
        let assets: Vec<WorldAsset> =
            serde_json::from_slice(&bytes).map_err(json_err(&assets_index))?;
        world.assets.clear();
        for a in assets {
            world.assets.insert(a.id.to_string(), a);
        }
    }

    // Missions + artifacts.
    let mut missions: HashMap<MissionId, Mission> = HashMap::new();
    let mut artifacts: HashMap<VerifiableArtifactId, VerifiableArtifact> = HashMap::new();
    let missions_dir = dir.join("missions");
    if missions_dir.is_dir() {
        for entry in read_dir(&missions_dir)? {
            let path = entry.path();
            if path.is_dir() {
                // Per-mission artifact subdir.
                let art_dir = path.join("artifacts");
                if art_dir.is_dir() {
                    for art_entry in read_dir(&art_dir)? {
                        let ap = art_entry.path();
                        if ap.extension().and_then(|s| s.to_str()) != Some("json") {
                            continue;
                        }
                        let bytes = fs::read(&ap).map_err(io_err(&ap))?;
                        let art: VerifiableArtifact =
                            serde_json::from_slice(&bytes).map_err(json_err(&ap))?;
                        artifacts.insert(art.id.clone(), art);
                    }
                }
            } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let bytes = fs::read(&path).map_err(io_err(&path))?;
                let m: Mission = serde_json::from_slice(&bytes).map_err(json_err(&path))?;
                missions.insert(m.id.clone(), m);
            }
        }
    }

    // Provenance tail (best-effort; full ledger is in wish-provenance).
    let worldline_path = dir.join("provenance").join("worldline.jsonl");
    if worldline_path.is_file() {
        let txt = fs::read_to_string(&worldline_path).map_err(io_err(&worldline_path))?;
        world.provenance.clear();
        for line in txt.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(ev) = serde_json::from_str::<WorldEvent>(trimmed) {
                world.provenance.push(ev);
            }
        }
    }

    Ok(WishWorldBundle {
        world,
        missions,
        artifacts,
    })
}

/// Write a `WishWorldBundle` to a `.wishworld/` directory.
pub fn write_world_dir(
    dir: impl AsRef<Path>,
    bundle: &WishWorldBundle,
) -> Result<(), WishWorldIoError> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir).map_err(io_err(dir))?;

    // world.json — top-level manifest minus the per-file collections.
    let mut shallow = bundle.world.clone();
    shallow.entities.clear();
    shallow.scenes.clear();
    shallow.agents.clear();
    shallow.assets.clear();
    shallow.provenance.clear();
    let manifest_path = dir.join("world.json");
    let manifest_bytes =
        serde_json::to_vec_pretty(&shallow).map_err(json_err(&manifest_path))?;
    fs::write(&manifest_path, manifest_bytes).map_err(io_err(&manifest_path))?;

    // entities/
    let entities_dir = dir.join("entities");
    fs::create_dir_all(&entities_dir).map_err(io_err(&entities_dir))?;
    for entity in bundle.world.entities.values() {
        let filename = safe_filename(&entity.id);
        let path = entities_dir.join(format!("{filename}.entity.json"));
        let bytes = serde_json::to_vec_pretty(entity).map_err(json_err(&path))?;
        fs::write(&path, bytes).map_err(io_err(&path))?;
    }

    // scenes/
    let scenes_dir = dir.join("scenes");
    fs::create_dir_all(&scenes_dir).map_err(io_err(&scenes_dir))?;
    for scene in bundle.world.scenes.values() {
        let filename = safe_filename(&scene.id);
        let path = scenes_dir.join(format!("{filename}.scene.json"));
        let bytes = serde_json::to_vec_pretty(scene).map_err(json_err(&path))?;
        fs::write(&path, bytes).map_err(io_err(&path))?;
    }

    // agents/world_agents/
    let agents_dir = dir.join("agents").join("world_agents");
    fs::create_dir_all(&agents_dir).map_err(io_err(&agents_dir))?;
    for agent in bundle.world.agents.values() {
        let filename = safe_filename(&agent.id);
        let path = agents_dir.join(format!("{filename}.agent.json"));
        let bytes = serde_json::to_vec_pretty(agent).map_err(json_err(&path))?;
        fs::write(&path, bytes).map_err(io_err(&path))?;
    }

    // assets/index.json
    if !bundle.world.assets.is_empty() {
        let assets_dir = dir.join("assets");
        fs::create_dir_all(&assets_dir).map_err(io_err(&assets_dir))?;
        let index_path = assets_dir.join("index.json");
        let assets: Vec<&WorldAsset> = bundle.world.assets.values().collect();
        let bytes = serde_json::to_vec_pretty(&assets).map_err(json_err(&index_path))?;
        fs::write(&index_path, bytes).map_err(io_err(&index_path))?;
    }

    // missions/<id>.mission.json + missions/<id>/artifacts/*.artifact.json
    if !bundle.missions.is_empty() || !bundle.artifacts.is_empty() {
        let missions_dir = dir.join("missions");
        fs::create_dir_all(&missions_dir).map_err(io_err(&missions_dir))?;
        for mission in bundle.missions.values() {
            let mp = missions_dir.join(format!("{}.mission.json", sanitize(&mission.id)));
            let bytes = serde_json::to_vec_pretty(mission).map_err(json_err(&mp))?;
            fs::write(&mp, bytes).map_err(io_err(&mp))?;
        }
        // Group artifacts under their mission directories.
        let mut by_mission: HashMap<&str, Vec<&VerifiableArtifact>> = HashMap::new();
        for a in bundle.artifacts.values() {
            by_mission.entry(a.mission_id.as_str()).or_default().push(a);
        }
        for (mid, arts) in by_mission {
            let art_dir = missions_dir.join(sanitize(mid)).join("artifacts");
            fs::create_dir_all(&art_dir).map_err(io_err(&art_dir))?;
            for a in arts {
                let ap = art_dir.join(format!("{}.artifact.json", sanitize(&a.id)));
                let bytes = serde_json::to_vec_pretty(a).map_err(json_err(&ap))?;
                fs::write(&ap, bytes).map_err(io_err(&ap))?;
            }
        }
    }

    // provenance/worldline.jsonl — `wish-provenance::WorldLine` owns
    // this file. We only write it on *initial seeding* (when it
    // doesn't yet exist), to avoid stomping richer wish-provenance
    // events with the slimmer wish-world-model::WorldEvent shape on
    // every `write_world_dir` call.
    if !bundle.world.provenance.is_empty() {
        let prov_dir = dir.join("provenance");
        fs::create_dir_all(&prov_dir).map_err(io_err(&prov_dir))?;
        let wl_path = prov_dir.join("worldline.jsonl");
        if !wl_path.exists() {
            let mut out = String::new();
            for ev in &bundle.world.provenance {
                let line = serde_json::to_string(ev).map_err(json_err(&wl_path))?;
                out.push_str(&line);
                out.push('\n');
            }
            fs::write(&wl_path, out).map_err(io_err(&wl_path))?;
        }
    }

    Ok(())
}

fn read_dir(path: &Path) -> Result<Vec<fs::DirEntry>, WishWorldIoError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(io_err(path))? {
        let entry = entry.map_err(io_err(path))?;
        entries.push(entry);
    }
    // Deterministic order — important for reproducible bundles.
    entries.sort_by_key(|e| e.path());
    Ok(entries)
}

fn safe_filename(id: &SemanticId) -> String {
    sanitize(&id.to_string())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::{
        ArtifactKind, ArtifactValidation, Mission, MissionStep, MissionStatus,
        VerifiableArtifact,
    };
    use crate::semantic_id::SemanticId;
    use crate::world::{EntityKind, WishWorld, WorldEntity, WorldKind};

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "wish_world_io_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn roundtrip_world_with_entities() {
        let dir = tmp();
        let mut world = WishWorld::new("shanhai", WorldKind::EducationWorld);
        world.upsert_entity(WorldEntity::stub(
            SemanticId::new(crate::Realm::Scene, "sacred_architecture", "dragon_temple"),
            "Dragon Temple",
            EntityKind::SacredArchitecture,
        ));
        world.upsert_entity(WorldEntity::stub(
            SemanticId::new(crate::Realm::Npc, "npc", "aling"),
            "A-Ling",
            EntityKind::Npc,
        ));
        let bundle = WishWorldBundle {
            world,
            missions: HashMap::new(),
            artifacts: HashMap::new(),
        };
        write_world_dir(&dir, &bundle).unwrap();

        let read = read_world_dir(&dir).unwrap();
        assert_eq!(read.world.name, "shanhai");
        assert_eq!(read.world.entities.len(), 2);
        let temple = read
            .world
            .entities
            .values()
            .find(|e| e.display_name == "Dragon Temple")
            .expect("temple");
        assert!(matches!(temple.kind, EntityKind::SacredArchitecture));
    }

    #[test]
    fn roundtrip_missions_and_artifacts() {
        let dir = tmp();
        let world = WishWorld::new("m_test", WorldKind::GenericProject);
        let mut mission = Mission::new(&world.id, "build harbor");
        mission.add_step(MissionStep {
            id: "1".into(),
            label: "terrain".into(),
            status: MissionStatus::Succeeded,
            depends_on: vec![],
        });
        let mission_id = mission.id.clone();
        let mut artifact = VerifiableArtifact::new(
            mission_id.clone(),
            ArtifactKind::CodeChange,
            "ev1",
            "patch1",
        );
        artifact.validation = ArtifactValidation {
            tests_passed: 41,
            tests_failed: 0,
            ..Default::default()
        };
        let mut missions = HashMap::new();
        missions.insert(mission_id.clone(), mission);
        let mut artifacts = HashMap::new();
        artifacts.insert(artifact.id.clone(), artifact);
        let bundle = WishWorldBundle {
            world,
            missions,
            artifacts,
        };
        write_world_dir(&dir, &bundle).unwrap();

        let read = read_world_dir(&dir).unwrap();
        assert_eq!(read.missions.len(), 1);
        assert_eq!(read.artifacts.len(), 1);
        let m = read.missions.get(&mission_id).unwrap();
        assert_eq!(m.intent, "build harbor");
        assert_eq!(m.plan.len(), 1);
    }

    #[test]
    fn read_missing_dir_returns_err() {
        let r = read_world_dir("/this/path/does/not/exist");
        assert!(r.is_err());
    }

    #[test]
    fn read_empty_world_dir_works() {
        let dir = tmp();
        let bundle = WishWorldBundle::default();
        // write a minimal world.json
        write_world_dir(&dir, &bundle).unwrap();
        let read = read_world_dir(&dir).unwrap();
        assert!(read.world.entities.is_empty());
        assert!(read.missions.is_empty());
        assert!(read.artifacts.is_empty());
    }
}
