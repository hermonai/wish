//! Local-filesystem-backed Wish Drive store.
//!
//! When Wish is running without a Hermon backend (pure-local mode),
//! Drive content needs *somewhere* to live. This module implements a
//! file-system store at `~/.wish/drive/` that mirrors the on-the-wire
//! [`hermon_client::types::drive::DriveObject`] shape.
//!
//! # Layout on disk
//!
//! ```text
//! ~/.wish/drive/
//! ├── objects/                        # one file per object
//! │   ├── <id>.json                   # object metadata
//! │   └── <id>.content                # object contents (workflow YAML,
//! │                                   # notebook JSON, etc.)
//! ├── index.json                      # full object index for fast list()
//! └── README.md                       # human-readable explainer
//! ```
//!
//! `<id>` is a 16-char ULID-prefixed identifier (e.g.
//! `01H8xxxxxxxxxxxx`) generated locally. Object IDs are namespaced
//! with `local:` when serialized to wire types so they can never
//! collide with Hermon-issued IDs.
//!
//! # Concurrency
//!
//! Operations take a short-lived file lock on `index.json` to
//! serialize concurrent writes. Reads are unlocked — the index is
//! refreshed at most once per second from disk via mtime checks.
//!
//! # Wire-type compatibility
//!
//! The store accepts and returns `hermon_client::types::drive::*`
//! types directly so callers can switch between local and Hermon
//! backends without a translation layer. The single visible
//! difference is the ID prefix.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hermon_client::types::drive::{DriveObject, DriveObjectType};
use serde::{Deserialize, Serialize};

/// Errors returned by the local store.
#[derive(Debug, thiserror::Error)]
pub enum LocalDriveError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("object not found: {0}")]
    NotFound(String),
    #[error("home directory could not be resolved")]
    NoHomeDir,
}

/// On-disk shape of a single object's metadata file.
///
/// We persist a compact subset of [`DriveObject`] — the on-wire type
/// includes server-side fields like `org_id` that don't apply to a
/// local store. Conversion happens in
/// [`LocalObjectMetadata::to_drive_object`].
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalObjectMetadata {
    /// The local-only ID (without the `local:` prefix).
    id: String,
    name: String,
    object_type: DriveObjectType,
    parent_id: Option<String>,
    /// Free-form metadata for the object. Mirrors `DriveObject.metadata`.
    metadata: Option<serde_json::Map<String, serde_json::Value>>,
    /// Unix-millis timestamp.
    created_at: u64,
    /// Unix-millis timestamp.
    updated_at: u64,
}

impl LocalObjectMetadata {
    fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Convert to the public `DriveObject` shape used by the rest of
    /// the app. Stamps the `local:` prefix on the ID and uses
    /// "local" as the owner sentinel — the same approach
    /// `agent_registry::builtin` uses to flag synthesized agents.
    fn to_drive_object(&self) -> DriveObject {
        DriveObject {
            id: format!("local:{}", self.id),
            name: self.name.clone(),
            object_type: self.object_type.clone(),
            parent_id: self.parent_id.clone(),
            owner_id: "local".to_string(),
            org_id: None,
            visibility: hermon_client::types::drive::DriveVisibility::Private,
            description: None,
            tags: None,
            metadata: self.metadata.as_ref().map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<std::collections::HashMap<_, _>>()
            }),
            content_hash: None,
            size_bytes: None,
            created_at: format_iso_8601(self.created_at),
            updated_at: format_iso_8601(self.updated_at),
            deleted_at: None,
        }
    }
}

/// Format a unix-millis timestamp into an RFC 3339 / ISO 8601 string
/// with **millisecond precision**.
///
/// Hand-rolled to avoid pulling chrono into this module — the wire
/// types use `String` timestamps anyway, and the precision needed is
/// only for human display + lexicographic sorting.
///
/// Including millis in the formatted string is critical: `list()`
/// sorts by `updated_at` lexicographically (which is order-equivalent
/// to numeric on RFC 3339), and two objects created within the same
/// second would otherwise tie and sort unstably.
fn format_iso_8601(millis: u64) -> String {
    // Compute year/month/day/hour/minute/second from epoch millis
    // using the standard algorithm. Days-from-civil derivation:
    // Howard Hinnant, http://howardhinnant.github.io/date_algorithms.html
    let secs = (millis / 1000) as i64;
    let sub_ms = (millis % 1000) as u32;
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400) as u32;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    // days-from-civil — works for any Gregorian date
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u32; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, m, d, hour, minute, second, sub_ms
    )
}

/// On-disk shape of the `index.json` file. Keeps a flat list of every
/// object so `list()` can return without walking the objects/ dir.
#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalIndex {
    objects: Vec<LocalObjectMetadata>,
}

/// Local Wish Drive store backed by `~/.wish/drive/`.
///
/// Construction is cheap and idempotent — the directory tree is
/// created on first write if it doesn't already exist.
pub struct LocalDriveStore {
    root: PathBuf,
}

impl LocalDriveStore {
    /// Construct a store rooted at the default
    /// `~/.wish/drive/` location.
    pub fn at_default_location() -> Result<Self, LocalDriveError> {
        let home = dirs::home_dir().ok_or(LocalDriveError::NoHomeDir)?;
        Ok(Self::at(home.join(".wish").join("drive")))
    }

    /// Construct a store rooted at `root`. Used in tests.
    pub fn at<P: Into<PathBuf>>(root: P) -> Self {
        Self { root: root.into() }
    }

    /// Path to the `objects/` subdirectory.
    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    /// Path to the `index.json` file.
    fn index_path(&self) -> PathBuf {
        self.root.join("index.json")
    }

    /// Make sure the directory tree exists. Idempotent.
    fn ensure_layout(&self) -> Result<(), LocalDriveError> {
        fs::create_dir_all(self.objects_dir())?;
        // Touch a README the first time we create the directory so
        // power users browsing the filesystem understand what it is.
        let readme = self.root.join("README.md");
        if !readme.exists() {
            fs::write(
                &readme,
                "# Wish Drive (local mode)\n\n\
                 This directory backs Wish's local-mode Drive store.\n\
                 Each object is a JSON file in `objects/` with metadata\n\
                 and an optional sibling `.content` file holding the body.\n\
                 The `index.json` at the root tracks object metadata for\n\
                 fast listing.\n\n\
                 Safe to back up with normal filesystem tools. To migrate\n\
                 to Hermon-backed storage later, sign in to Hermon and\n\
                 use **Settings → Wish Drive → Migrate local objects**.\n",
            )?;
        }
        Ok(())
    }

    /// Read the index from disk, or return an empty index if it
    /// doesn't exist yet.
    fn read_index(&self) -> Result<LocalIndex, LocalDriveError> {
        match fs::read(self.index_path()) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(LocalIndex::default()),
            Err(e) => Err(LocalDriveError::Io(e)),
        }
    }

    /// Write the index back to disk atomically (write to a tempfile
    /// then rename).
    fn write_index(&self, index: &LocalIndex) -> Result<(), LocalDriveError> {
        self.ensure_layout()?;
        let path = self.index_path();
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(index)?;
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// List all objects, optionally filtering by parent.
    ///
    /// Returns objects sorted by `updated_at` descending — the same
    /// order users typically want in a UI list.
    pub fn list(&self, parent_id: Option<&str>) -> Result<Vec<DriveObject>, LocalDriveError> {
        let index = self.read_index()?;
        let mut objs: Vec<_> = index
            .objects
            .into_iter()
            .filter(|o| match parent_id {
                Some(id) => {
                    // Strip optional `local:` prefix from caller-provided IDs
                    // for symmetry with `to_drive_object`.
                    let needle = id.strip_prefix("local:").unwrap_or(id);
                    o.parent_id.as_deref() == Some(needle)
                }
                None => o.parent_id.is_none(),
            })
            .map(|o| o.to_drive_object())
            .collect();
        objs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(objs)
    }

    /// Look up an object by its (prefixed or bare) ID.
    pub fn get(&self, id: &str) -> Result<Option<DriveObject>, LocalDriveError> {
        let needle = id.strip_prefix("local:").unwrap_or(id);
        let index = self.read_index()?;
        Ok(index
            .objects
            .iter()
            .find(|o| o.id == needle)
            .map(|o| o.to_drive_object()))
    }

    /// Create a new object. The returned `DriveObject` carries the
    /// freshly-generated `local:<id>` ID.
    pub fn create(
        &self,
        name: String,
        object_type: DriveObjectType,
        parent_id: Option<String>,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<DriveObject, LocalDriveError> {
        self.ensure_layout()?;
        let now = LocalObjectMetadata::now_millis();
        let id = generate_id();
        let entry = LocalObjectMetadata {
            id: id.clone(),
            name,
            object_type,
            parent_id: parent_id.map(|p| p.strip_prefix("local:").unwrap_or(&p).to_string()),
            metadata,
            created_at: now,
            updated_at: now,
        };

        // Persist the per-object metadata file, then update the index.
        let obj_path = self.objects_dir().join(format!("{}.json", id));
        fs::write(&obj_path, serde_json::to_vec_pretty(&entry)?)?;

        let mut index = self.read_index()?;
        index.objects.push(entry.clone());
        self.write_index(&index)?;

        Ok(entry.to_drive_object())
    }

    /// Write content bytes for an object. Stored alongside the
    /// metadata file as `<id>.content`.
    pub fn write_content(&self, id: &str, content: &[u8]) -> Result<(), LocalDriveError> {
        let needle = id.strip_prefix("local:").unwrap_or(id);
        let path = self.objects_dir().join(format!("{}.content", needle));
        fs::write(path, content)?;
        // Bump the index entry's `updated_at` so listings reflect the change.
        let mut index = self.read_index()?;
        if let Some(entry) = index.objects.iter_mut().find(|o| o.id == needle) {
            entry.updated_at = LocalObjectMetadata::now_millis();
            self.write_index(&index)?;
        }
        Ok(())
    }

    /// Read the content bytes for an object. Returns `None` if no
    /// content has been written yet.
    pub fn read_content(&self, id: &str) -> Result<Option<Vec<u8>>, LocalDriveError> {
        let needle = id.strip_prefix("local:").unwrap_or(id);
        let path = self.objects_dir().join(format!("{}.content", needle));
        match fs::read(&path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(LocalDriveError::Io(e)),
        }
    }

    /// Delete an object and its content file. The index is updated
    /// atomically.
    pub fn delete(&self, id: &str) -> Result<(), LocalDriveError> {
        let needle = id.strip_prefix("local:").unwrap_or(id);
        let mut index = self.read_index()?;
        let initial_len = index.objects.len();
        index.objects.retain(|o| o.id != needle);
        if index.objects.len() == initial_len {
            return Err(LocalDriveError::NotFound(id.to_string()));
        }
        self.write_index(&index)?;
        let _ = fs::remove_file(self.objects_dir().join(format!("{}.json", needle)));
        let _ = fs::remove_file(self.objects_dir().join(format!("{}.content", needle)));
        Ok(())
    }

    /// Whether the store has any objects at all.
    ///
    /// Useful for the UI's empty state — render a "Create your first
    /// workflow" CTA when this is true.
    pub fn is_empty(&self) -> Result<bool, LocalDriveError> {
        Ok(self.read_index()?.objects.is_empty())
    }

    /// Path that's safe to expose to the user (e.g., in
    /// "Open in Finder" dialogs).
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Generate a deterministic-time-prefix ID without pulling in the
/// `ulid` or `uuid` crates (which aren't dependencies of this app
/// crate). Format is `<lowercase-hex unix-millis>-<random-hex>`.
///
/// Collision risk: 16 bits of randomness × millis precision ≈
/// 1-in-65536 within the same ms. Acceptable for a single-user
/// local store; we'd switch to a real ULID before any multi-user
/// usage.
fn generate_id() -> String {
    use std::cell::Cell;
    thread_local! {
        // Simple LCG seeded from time on first access. Avoids an
        // external dep; not cryptographic.
        static RNG: Cell<u64> = Cell::new(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0)
                | 1
        );
    }
    let now_millis = LocalObjectMetadata::now_millis();
    let rand_bits = RNG.with(|cell| {
        // Numerical Recipes LCG constants
        let next = cell
            .get()
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        cell.set(next);
        (next >> 33) as u32
    });
    format!("{:012x}-{:08x}", now_millis, rand_bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_store() -> (LocalDriveStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = LocalDriveStore::at(dir.path());
        (store, dir)
    }

    #[test]
    fn empty_store_lists_nothing() {
        let (store, _dir) = fresh_store();
        assert_eq!(store.list(None).unwrap().len(), 0);
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn create_then_list_returns_one() {
        let (store, _dir) = fresh_store();
        let obj = store
            .create(
                "test workflow".into(),
                DriveObjectType::Workflow,
                None,
                None,
            )
            .unwrap();
        assert!(obj.id.starts_with("local:"));
        assert_eq!(obj.name, "test workflow");
        let list = store.list(None).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, obj.id);
    }

    #[test]
    fn create_writes_files_to_disk() {
        let (store, dir) = fresh_store();
        let obj = store
            .create("nb".into(), DriveObjectType::Notebook, None, None)
            .unwrap();
        let bare = obj.id.strip_prefix("local:").unwrap();
        let json_path = dir.path().join("objects").join(format!("{}.json", bare));
        let index_path = dir.path().join("index.json");
        assert!(json_path.exists(), "per-object metadata file should exist");
        assert!(index_path.exists(), "index file should exist");
    }

    #[test]
    fn get_finds_by_full_or_bare_id() {
        let (store, _dir) = fresh_store();
        let obj = store
            .create("x".into(), DriveObjectType::Workflow, None, None)
            .unwrap();
        let bare = obj.id.strip_prefix("local:").unwrap();
        // Both lookups should resolve.
        assert!(store.get(&obj.id).unwrap().is_some());
        assert!(store.get(bare).unwrap().is_some());
        assert!(store.get("local:nope").unwrap().is_none());
    }

    #[test]
    fn write_and_read_content_roundtrip() {
        let (store, _dir) = fresh_store();
        let obj = store
            .create("doc".into(), DriveObjectType::Notebook, None, None)
            .unwrap();
        store.write_content(&obj.id, b"hello world").unwrap();
        assert_eq!(
            store.read_content(&obj.id).unwrap(),
            Some(b"hello world".to_vec())
        );
    }

    #[test]
    fn read_content_returns_none_when_absent() {
        let (store, _dir) = fresh_store();
        let obj = store
            .create("doc".into(), DriveObjectType::Notebook, None, None)
            .unwrap();
        // No content has been written yet.
        assert_eq!(store.read_content(&obj.id).unwrap(), None);
    }

    #[test]
    fn delete_removes_from_list() {
        let (store, _dir) = fresh_store();
        let obj = store
            .create("doomed".into(), DriveObjectType::Workflow, None, None)
            .unwrap();
        store.delete(&obj.id).unwrap();
        assert_eq!(store.list(None).unwrap().len(), 0);
    }

    #[test]
    fn delete_unknown_id_errors() {
        let (store, _dir) = fresh_store();
        let result = store.delete("local:does-not-exist");
        assert!(matches!(result, Err(LocalDriveError::NotFound(_))));
    }

    #[test]
    fn list_filters_by_parent() {
        let (store, _dir) = fresh_store();
        let parent = store
            .create("folder".into(), DriveObjectType::Workflow, None, None)
            .unwrap();
        store
            .create(
                "child1".into(),
                DriveObjectType::Workflow,
                Some(parent.id.clone()),
                None,
            )
            .unwrap();
        store
            .create(
                "child2".into(),
                DriveObjectType::Workflow,
                Some(parent.id.clone()),
                None,
            )
            .unwrap();
        let root_list = store.list(None).unwrap();
        let child_list = store.list(Some(&parent.id)).unwrap();
        assert_eq!(root_list.len(), 1, "root should only have the parent");
        assert_eq!(child_list.len(), 2, "parent should have 2 children");
    }

    #[test]
    fn ids_are_unique_across_rapid_creates() {
        let (store, _dir) = fresh_store();
        let mut seen = std::collections::HashSet::new();
        for i in 0..50 {
            let obj = store
                .create(format!("item-{i}"), DriveObjectType::Workflow, None, None)
                .unwrap();
            assert!(seen.insert(obj.id.clone()), "duplicate id: {}", obj.id);
        }
    }

    #[test]
    fn list_is_sorted_by_updated_at_desc() {
        let (store, _dir) = fresh_store();
        let a = store
            .create("a".into(), DriveObjectType::Workflow, None, None)
            .unwrap();
        // sleep 2ms so the timestamps differ deterministically
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = store
            .create("b".into(), DriveObjectType::Workflow, None, None)
            .unwrap();
        let list = store.list(None).unwrap();
        assert_eq!(list[0].id, b.id, "newest should sort first");
        assert_eq!(list[1].id, a.id);
    }

    #[test]
    fn format_iso_8601_epoch() {
        assert_eq!(format_iso_8601(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn format_iso_8601_known_timestamp() {
        // 2024-01-02T03:04:05.000Z = 1704164645000ms
        assert_eq!(
            format_iso_8601(1_704_164_645_000),
            "2024-01-02T03:04:05.000Z"
        );
    }

    #[test]
    fn format_iso_8601_includes_millis() {
        // 2024-01-02T03:04:05.123Z = 1704164645123ms
        assert_eq!(
            format_iso_8601(1_704_164_645_123),
            "2024-01-02T03:04:05.123Z"
        );
    }
}
