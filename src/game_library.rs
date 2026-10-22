//! Bounded client metadata. Reading and editing this module never launches Android.
#![forbid(unsafe_code)]

use crate::{Error, ErrorKind, Result, android_reference::valid_component};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const LIBRARY_SCHEMA: u32 = 1;
pub const MAX_LIBRARY_BYTES: usize = 256 * 1024;
pub const MAX_GAMES: usize = 256;
pub const MAX_PROFILES: usize = 16;
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameLibrary {
    pub schema_version: u32,
    pub runtime_profiles: Vec<RuntimeProfile>,
    pub games: Vec<GameEntry>,
}

impl Default for GameLibrary {
    fn default() -> Self {
        Self {
            schema_version: LIBRARY_SCHEMA,
            runtime_profiles: Vec::new(),
            games: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProfile {
    pub id: String,
    pub name: String,
    pub engine: RuntimeEngine,
    pub sdk_root: PathBuf,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEngine {
    AndroidSdkReference,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameEntry {
    pub id: String,
    pub title: String,
    pub component: String,
    pub runtime_profile_id: String,
    pub installation: Installation,
    pub icon_path: Option<PathBuf>,
    pub fixture: Option<FixtureSpec>,
}

/// Cached installation evidence, not a claim that an offline guest is currently running.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Installation {
    NotInstalled,
    Installed { verified_at_unix_ms: u64 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSpec {
    pub apk_path: PathBuf,
    pub abi: LibraryAbi,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LibraryAbi {
    #[serde(rename = "x86_64")]
    X86_64,
    #[serde(rename = "arm64-v8a")]
    Arm64,
}

impl GameLibrary {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != LIBRARY_SCHEMA {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!(
                    "library schema {} is unsupported; expected {LIBRARY_SCHEMA}",
                    self.schema_version
                ),
            ));
        }
        if self.games.len() > MAX_GAMES || self.runtime_profiles.len() > MAX_PROFILES {
            return Err(invalid("game library exceeds its entry/profile limit"));
        }
        let mut profiles = BTreeSet::new();
        for profile in &self.runtime_profiles {
            if !valid_id(&profile.id) || !profiles.insert(profile.id.as_str()) {
                return Err(invalid("runtime profile IDs must be valid and unique"));
            }
            if !valid_title(&profile.name) || !valid_path(&profile.sdk_root) {
                return Err(invalid("runtime profile name or SDK path is invalid"));
            }
        }
        let mut ids = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for game in &self.games {
            if !valid_id(&game.id) || !ids.insert(game.id.as_str()) {
                return Err(invalid("game IDs must be valid and unique"));
            }
            if !valid_title(&game.title)
                || game.component.len() > 512
                || !valid_component(&game.component)
            {
                return Err(invalid("game title or package/activity is invalid"));
            }
            if !profiles.contains(game.runtime_profile_id.as_str()) {
                return Err(invalid("game refers to a missing runtime profile"));
            }
            let (package, activity) = game
                .component
                .split_once('/')
                .ok_or_else(|| invalid("game component has no activity"))?;
            let activity = if activity.starts_with('.') {
                format!("{package}{activity}")
            } else {
                activity.to_owned()
            };
            if !targets.insert((game.runtime_profile_id.as_str(), package, activity)) {
                return Err(invalid("duplicate game target in the same runtime profile"));
            }
            if matches!(
                game.installation,
                Installation::Installed {
                    verified_at_unix_ms: 0
                }
            ) {
                return Err(invalid(
                    "installed entries require a verification timestamp",
                ));
            }
            if game
                .icon_path
                .as_deref()
                .is_some_and(|path| !valid_path(path))
            {
                return Err(invalid("game icon path is invalid"));
            }
            if let Some(fixture) = &game.fixture
                && (!valid_path(&fixture.apk_path)
                    || package != "dev.emucore.probe"
                    || !matches!(
                        game.component.as_str(),
                        "dev.emucore.probe/.ProbeActivity"
                            | "dev.emucore.probe/dev.emucore.probe.ProbeActivity"
                    ))
            {
                return Err(invalid(
                    "native fixture metadata must reference the owned probe activity and APK",
                ));
            }
        }
        Ok(())
    }

    pub fn installed_games(&self) -> impl Iterator<Item = &GameEntry> {
        self.games
            .iter()
            .filter(|entry| matches!(entry.installation, Installation::Installed { .. }))
    }

    pub fn profile_for(&self, entry: &GameEntry) -> Option<&RuntimeProfile> {
        self.runtime_profiles
            .iter()
            .find(|profile| profile.id == entry.runtime_profile_id)
    }
}

/// A path-only store: construction and reads have no engine, SDK, or process dependency.
pub struct LibraryStore {
    path: PathBuf,
}

impl LibraryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A missing library is empty; malformed existing data is preserved and reported.
    pub fn load(&self) -> Result<GameLibrary> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(GameLibrary::default());
            }
            Err(error) => return Err(Error::io("open game library", error)),
        };
        let mut bytes = Vec::new();
        file.take(MAX_LIBRARY_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| Error::io("read game library", e))?;
        if bytes.len() > MAX_LIBRARY_BYTES {
            return Err(invalid("game library exceeds 256 KiB"));
        }
        let library: GameLibrary = serde_json::from_slice(&bytes)
            .map_err(|error| invalid(format!("invalid game library JSON: {error}")))?;
        library.validate()?;
        Ok(library)
    }

    /// Applies an edit to a fresh snapshot under a nonblocking OS file lock, then
    /// syncs a new same-directory temporary file and renames it over the old file.
    pub fn update(&self, edit: impl FnOnce(&mut GameLibrary) -> Result<()>) -> Result<GameLibrary> {
        let parent = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|e| Error::io("create library directory", e))?;
        let filename = self
            .path
            .file_name()
            .ok_or_else(|| invalid("library file name is missing"))?;
        let lock_path = parent.join(format!("{}.lock", filename.to_string_lossy()));
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| Error::io("open library edit lock", e))?;
        lock.try_lock().map_err(|error| {
            Error::new(
                ErrorKind::Host,
                format!("game library is locked or unavailable: {error}"),
            )
        })?;
        let mut library = self.load()?;
        edit(&mut library)?;
        library.validate()?;
        let mut bytes = serde_json::to_vec_pretty(&library)
            .map_err(|e| invalid(format!("serialize game library: {e}")))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_LIBRARY_BYTES {
            return Err(invalid("serialized game library exceeds 256 KiB"));
        }
        let (temporary, mut file) = create_temporary(parent)?;
        let cleanup = Temporary(temporary);
        let written = file.write_all(&bytes).and_then(|_| file.sync_all());
        drop(file);
        written.map_err(|e| Error::io("write new game library", e))?;
        fs::rename(&cleanup.0, &self.path).map_err(|e| Error::io("replace game library", e))?;
        // The old temporary name is gone. The persistent lock file is harmless:
        // its OS lock is released on close or process exit, even after a crash.
        drop(cleanup);
        drop(lock);
        Ok(library)
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn create_temporary(parent: &Path) -> Result<(PathBuf, File)> {
    for _ in 0..32 {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".emucore-library-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Error::io("create temporary game library", error)),
        }
    }
    Err(invalid(
        "could not allocate a unique temporary library file",
    ))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}
fn valid_title(title: &str) -> bool {
    !title.trim().is_empty() && title.len() <= 256 && !title.chars().any(char::is_control)
}
fn valid_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .to_str()
            .is_some_and(|text| text.len() <= 32767 && !text.chars().any(char::is_control))
}
fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Usage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::windows::fs::OpenOptionsExt, sync::atomic::AtomicUsize};

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "emucore-library-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn store(&self) -> LibraryStore {
            LibraryStore::new(self.0.join("library.json"))
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn library() -> GameLibrary {
        GameLibrary {
            schema_version: LIBRARY_SCHEMA,
            runtime_profiles: vec![RuntimeProfile {
                id: "reference".into(),
                name: "External SDK reference".into(),
                engine: RuntimeEngine::AndroidSdkReference,
                sdk_root: "missing-sdk-is-not-probed".into(),
            }],
            games: vec![GameEntry {
                id: "owned-probe".into(),
                title: "Owned native probe 🦀".into(),
                component: "dev.emucore.probe/.ProbeActivity".into(),
                runtime_profile_id: "reference".into(),
                installation: Installation::Installed {
                    verified_at_unix_ms: 123,
                },
                icon_path: None,
                fixture: Some(FixtureSpec {
                    apk_path: "missing-owned-fixture-is-not-probed.apk".into(),
                    abi: LibraryAbi::X86_64,
                }),
            }],
        }
    }

    #[test]
    fn missing_library_load_is_inert_and_creates_no_files() {
        let directory = Directory::new();
        assert_eq!(directory.store().load().unwrap(), GameLibrary::default());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn metadata_round_trips_without_probing_sdk_or_guest() {
        let directory = Directory::new();
        let store = directory.store();
        let expected = library();
        store
            .update(|value| {
                *value = expected.clone();
                Ok(())
            })
            .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded, expected);
        assert_eq!(loaded.installed_games().count(), 1);
        assert_eq!(
            loaded.profile_for(&loaded.games[0]).unwrap().id,
            "reference"
        );
        store
            .update(|value| {
                value.games[0].installation = Installation::NotInstalled;
                Ok(())
            })
            .unwrap();
        assert_eq!(store.load().unwrap().installed_games().count(), 0);
    }

    #[test]
    fn corrupt_unknown_and_future_metadata_is_preserved() {
        let directory = Directory::new();
        let store = directory.store();
        let valid = serde_json::to_string(&library()).unwrap();
        let unknown = valid.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"unexpected\":true",
            1,
        );
        let future = valid.replacen("\"schema_version\":1", "\"schema_version\":2", 1);
        for input in ["not json", &unknown, &future] {
            fs::write(store.path(), input).unwrap();
            assert!(store.load().is_err());
            assert!(store.update(|_| Ok(())).is_err());
            assert_eq!(fs::read_to_string(store.path()).unwrap(), input);
        }
    }

    #[test]
    fn invalid_or_rejected_edits_preserve_the_previous_library() {
        let directory = Directory::new();
        let store = directory.store();
        store
            .update(|value| {
                *value = library();
                Ok(())
            })
            .unwrap();
        let before = fs::read(store.path()).unwrap();
        assert!(
            store
                .update(|value| {
                    value.games[0].component = "dev.probe/.A;bad".into();
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(fs::read(store.path()).unwrap(), before);
        let error = store
            .update(|_| Err(Error::new(ErrorKind::Runtime, "edit rejected")))
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Runtime);
        assert_eq!(fs::read(store.path()).unwrap(), before);
    }

    #[test]
    fn held_lock_blocks_edits_and_close_releases_it_without_stale_cleanup() {
        let directory = Directory::new();
        let store = directory.store();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.0.join("library.json.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        assert!(store.update(|_| Ok(())).is_err());
        assert!(!store.path().exists());
        drop(lock);
        store.update(|_| Ok(())).unwrap();
        assert_eq!(store.load().unwrap(), GameLibrary::default());
    }

    #[test]
    fn failed_atomic_replace_preserves_old_data_and_removes_temporary_file() {
        let directory = Directory::new();
        let store = directory.store();
        store
            .update(|value| {
                *value = library();
                Ok(())
            })
            .unwrap();
        let before = fs::read(store.path()).unwrap();
        // FILE_SHARE_READ permits the fresh read but blocks replacement/deletion.
        let blocker = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(store.path())
            .unwrap();
        assert!(
            store
                .update(|value| {
                    value.games[0].title = "Changed".into();
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(fs::read(store.path()).unwrap(), before);
        assert!(!fs::read_dir(&directory.0).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|extension| extension == "tmp")
        }));
        drop(blocker);
        store
            .update(|value| {
                value.games[0].title = "Changed".into();
                Ok(())
            })
            .unwrap();
        assert_eq!(store.load().unwrap().games[0].title, "Changed");
    }

    #[test]
    fn duplicate_ids_targets_and_missing_profiles_are_rejected() {
        let mut duplicate = library();
        duplicate.games.push(duplicate.games[0].clone());
        assert!(duplicate.validate().is_err());
        let mut target = library();
        let mut alias = target.games[0].clone();
        alias.id = "alias".into();
        alias.component = "dev.emucore.probe/dev.emucore.probe.ProbeActivity".into();
        target.games.push(alias);
        assert!(target.validate().is_err());
        let mut profile = library();
        profile.runtime_profiles.clear();
        assert!(profile.validate().is_err());
        let mut profiles = library();
        profiles
            .runtime_profiles
            .push(profiles.runtime_profiles[0].clone());
        assert!(profiles.validate().is_err());
    }

    #[test]
    fn resource_limits_and_invalid_cached_evidence_are_rejected() {
        let directory = Directory::new();
        let store = directory.store();
        fs::write(store.path(), vec![b' '; MAX_LIBRARY_BYTES + 1]).unwrap();
        assert!(store.load().is_err());
        let mut too_many = library();
        too_many.games = vec![too_many.games[0].clone(); MAX_GAMES + 1];
        assert!(too_many.validate().is_err());
        let mut unverified = library();
        unverified.games[0].installation = Installation::Installed {
            verified_at_unix_ms: 0,
        };
        assert!(unverified.validate().is_err());
        let mut invalid_name = library();
        invalid_name.games[0].title = "one\ntwo".into();
        assert!(invalid_name.validate().is_err());
        let mut fixture = library();
        fixture.games[0].component = "dev.other/.Activity".into();
        assert!(fixture.validate().is_err());
    }
}
