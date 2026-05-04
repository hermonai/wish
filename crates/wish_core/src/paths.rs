//! Helper functions for retrieving base paths for storing config/data files.
//!
//! This file should not be directly exposed to or used in integration tests;
//! any paths computed using these functions should be exposed to integration
//! tests through use-case-specific helper functions.
//!
//! `_local_dir` variants of functions are for storing non-portable data, where
//! "portable" refers to the ability to copy that file to another machine.
//! Some examples of non-portable data include things that reference local
//! paths (which may not exist on a different machine), such as paths to shell
//! binaries or user-added theme files.
//!
//! TODO(vorporeal): In general, we should be returning Option<PathBuf> or
//! Result<PathBuf> when we can't compute the home directory instead of
//! returning a relative path.

use std::path::{Path, PathBuf};

use cfg_if::cfg_if;
use directories::BaseDirs;

use crate::{
    channel::{Channel, ChannelState},
    AppId,
};

/// The name of the directory in which to put non-global Wish-specific files.
///
/// This should be used, for example, as the base directory under which
/// repository workflows would be stored (in "./.wish/workflows").
///
/// The directory structure under `.wish/` supports multiple product lines:
/// - `.wish/code/` — for wishcode
/// - `.wish/cli/` — for wish CLI
pub const WISH_CONFIG_DIR: &str = ".wish";

/// Legacy config directory name, kept for backwards compatibility migration.
const LEGACY_WARP_CONFIG_DIR: &str = ".warp";

/// The name of the folder that stores Wish execution logs and network logs.
/// This is currently only used on Windows to maintain backwards compatibility.
pub const WISH_LOGS_DIR: &str = "logs";

/// Backwards-compatible alias for [`WISH_CONFIG_DIR`].
#[deprecated(note = "Use WISH_CONFIG_DIR instead")]
pub const WARP_CONFIG_DIR: &str = ".wish";

/// Backwards-compatible alias for [`WISH_LOGS_DIR`].
#[deprecated(note = "Use WISH_LOGS_DIR instead")]
pub const WARP_LOGS_DIR: &str = "logs";

fn base_wish_config_dir_name() -> String {
    match ChannelState::channel() {
        // All channels share the same `.wish` directory.
        // Product-line subdirs (`.wish/code/`, `.wish/cli/`) handle isolation.
        Channel::Stable | Channel::Preview | Channel::Oss | Channel::Dev | Channel::Local => {
            WISH_CONFIG_DIR.to_owned()
        }
        // Integration tests get their own directory to avoid polluting user config.
        Channel::Integration => format!("{WISH_CONFIG_DIR}-integration"),
    }
}
/// Returns the home-relative Wish config directory name for the current channel and data profile.
///
/// This preserves the historical `.wish*` directory shape while still isolating dev, local,
/// integration, oss, and optional development profiles.
pub fn wish_home_config_dir_name() -> String {
    let base_dir_name = base_wish_config_dir_name();

    if let Some(data_profile) = ChannelState::data_profile() {
        format!("{base_dir_name}-{data_profile}")
    } else {
        base_dir_name
    }
}

/// Returns the home-relative Wish config directory for the current channel and data profile.
///
/// Unlike [`data_dir`] and [`config_local_dir`] on non-macOS platforms, this intentionally keeps
/// Wish-authored, user-facing config under a `.wish*` directory in the home directory instead of
/// using the platform XDG/AppData project directories.
///
/// If the `.wish` directory does not exist but the legacy `.warp` directory does,
/// the legacy path is returned as a fallback for backwards compatibility.
pub fn wish_home_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home_dir| {
        let wish_dir = home_dir.join(wish_home_config_dir_name());
        if wish_dir.exists() {
            return wish_dir;
        }

        // Fallback to the pre-rebrand Warp config. Wish itself always writes to `.wish`.
        let legacy_suffixes: &[&str] = &[LEGACY_WARP_CONFIG_DIR];
        for suffix in legacy_suffixes {
            let legacy_dir = home_dir.join(suffix);
            if legacy_dir.exists() {
                return legacy_dir;
            }
        }

        // Neither exists yet; return the new .wish path so it gets created there.
        wish_dir
    })
}

pub fn wish_home_skills_dir() -> Option<PathBuf> {
    wish_home_config_dir().map(|config_dir| config_dir.join("skills"))
}

pub fn wish_home_mcp_config_file_path() -> Option<PathBuf> {
    wish_home_config_dir().map(|config_dir| config_dir.join(".mcp.json"))
}

/// Backwards-compatible alias for [`wish_home_config_dir_name`].
#[deprecated(note = "Use wish_home_config_dir_name instead")]
pub fn warp_home_config_dir_name() -> String {
    wish_home_config_dir_name()
}

/// Backwards-compatible alias for [`wish_home_config_dir`].
#[deprecated(note = "Use wish_home_config_dir instead")]
pub fn warp_home_config_dir() -> Option<PathBuf> {
    wish_home_config_dir()
}

/// Backwards-compatible alias for [`wish_home_skills_dir`].
#[deprecated(note = "Use wish_home_skills_dir instead")]
pub fn warp_home_skills_dir() -> Option<PathBuf> {
    wish_home_skills_dir()
}

/// Backwards-compatible alias for [`wish_home_mcp_config_file_path`].
#[deprecated(note = "Use wish_home_mcp_config_file_path instead")]
pub fn warp_home_mcp_config_file_path() -> Option<PathBuf> {
    wish_home_mcp_config_file_path()
}

/// Returns the macOS config directory name for the current channel.
///
/// All channels share `.wish`; only integration tests get a separate directory.
#[cfg(target_os = "macos")]
fn macos_config_dir_name() -> String {
    match ChannelState::channel() {
        Channel::Stable | Channel::Preview | Channel::Oss | Channel::Dev | Channel::Local => {
            WISH_CONFIG_DIR.to_owned()
        }
        Channel::Integration => format!("{WISH_CONFIG_DIR}-integration"),
    }
}

/// Returns the path to the directory where portable user data should be
/// stored.
///
/// This is the appropriate home for things like custom themes and workflows.
pub fn data_dir() -> PathBuf {
    cfg_if! {
        if #[cfg(target_os = "macos")] {
            // TODO(vorporeal): We should do something better than return a
            // relative path.
            dirs::home_dir().unwrap_or_default().join(macos_config_dir_name())
        } else {
            project_dirs().map(|dirs| dirs.data_dir().to_owned()).unwrap_or_default()
        }
    }
}

/// Returns the path to the directory where non-portable configuration files
/// should be stored.
pub fn config_local_dir() -> PathBuf {
    cfg_if! {
        if #[cfg(target_os = "macos")] {
            // TODO(vorporeal): We should do something better than return a
            // relative path.
            dirs::home_dir().unwrap_or_default().join(macos_config_dir_name())
        } else {
            project_dirs()
                .map(|dirs| dirs.config_local_dir().to_owned())
                .unwrap_or_default()
        }
    }
}

/// Returns the base directory for general config files. Useful for accessing the config files for
/// other programs.
pub fn base_config_dir() -> PathBuf {
    BaseDirs::new()
        .map(|dirs| dirs.config_dir().to_owned())
        .unwrap_or_default()
}

/// Returns the path to the directory where non-portable application state data
/// should be stored.
///
/// This is the appropriate home for files like our sqlite database, which
/// contains durable but non-critical and non-portable data like what windows
/// the user had open and cached state of known Wish Drive objects.
pub fn state_dir() -> PathBuf {
    let Some(project_dirs) = project_dirs() else {
        return PathBuf::new();
    };
    // For platforms that don't have a notion of a "state" directory (e.g.:
    // macOS and Windows), fall back to using the data directory.
    project_dirs
        .state_dir()
        .unwrap_or_else(|| project_dirs.data_local_dir())
        .to_owned()
}

/// Returns the path to the secure directory for non-portable application state data.
///
/// Prefer this over [`state_dir`] where possible.
///
/// On macOS, this will use the App Group container directory if available.
pub fn secure_state_dir() -> Option<PathBuf> {
    // Do not use the secure state directory in integration tests, which have a temporary home directory instead.
    if ChannelState::channel() == Channel::Integration {
        return None;
    }

    #[cfg(target_os = "macos")]
    if let Some(app_group_root) = app_group_container_path() {
        // The macOS project_path is the bundle ID (i.e. `ai.hermon.Wish`).
        let project_dirs = project_dirs()?;
        return Some(
            app_group_root
                .join("Library/Application Support")
                .join(project_dirs.project_path()),
        );
    }

    None
}

/// Returns the path to the directory containing the user's custom themes.
pub fn themes_dir() -> PathBuf {
    data_dir().join("themes")
}

/// Returns the path to the directory where files can be stored for caching
/// purposes.
///
/// This is a good place to store things like user profile pictures, which
/// we don't want to fetch on every launch of the app but can be safely
/// deleted by the OS.
pub fn cache_dir() -> PathBuf {
    let Some(project_dirs) = project_dirs() else {
        return PathBuf::new();
    };
    cfg_if! {
        if #[cfg(target_os = "macos")] {
            // TODO(vorporeal): Given that this is just cache data; do we want
            // change the path we use on macOS?
            project_dirs.data_dir().to_owned()
        } else {
            project_dirs.cache_dir().to_owned()
        }
    }
}

/// Returns a display-ready version of the path that is formatted in a
/// home-dir-relative manner, if appropriate.
pub fn home_relative_path(path: &Path) -> String {
    #[cfg(unix)]
    if let Some(base_dirs) = directories::BaseDirs::new() {
        if let Ok(relative_path) = path.strip_prefix(base_dirs.home_dir()) {
            return format!("~/{}", relative_path.display());
        }
    };

    path.display().to_string()
}

/// Returns a [`directories::ProjectDirs`] configured based on the current app ID
/// and the current data profile, if one is set.
///
/// This returns [`None`] if the user's home directory could not be determined.
fn project_dirs() -> Option<directories::ProjectDirs> {
    project_dirs_for_app_id(
        ChannelState::app_id(),
        ChannelState::data_profile().as_deref(),
    )
}

/// Returns a [`directories::ProjectDirs`] configured based on the given app ID
/// and data profile.
///
/// This returns [`None`] if the user's home directory could not be determined.
fn project_dirs_for_app_id(
    app_id: AppId,
    data_profile: Option<&str>,
) -> Option<directories::ProjectDirs> {
    cfg_if::cfg_if! {
        if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            // Adjust the base application name so that we end up with
            // directories like "wish-terminal" and "wish-terminal-dev", to
            // match our Linux package name.
            let base_app_name = match app_id.application_name() {
                "Warp" => "Wish-Terminal".to_owned(),
                "WarpOss" => "Wish-Oss".to_owned(),
                other if other.starts_with("Warp") => other.replace("Warp", "Wish-Terminal-"),
                _ => app_id.application_name().to_owned(),
            };
        } else {
            let base_app_name = app_id.application_name().to_owned();
        }
    }
    let app_name = if let Some(data_profile) = data_profile {
        format!("{base_app_name}-{data_profile}")
    } else {
        base_app_name
    };
    directories::ProjectDirs::from(app_id.qualifier(), app_id.organization(), &app_name)
}

/// Returns the path to the app's secure group container on macOS.
///
/// Returns `None` if the container URL cannot be resolved or converted.
///
/// See:
/// * [Configuring app groups](https://developer.apple.com/documentation/Xcode/configuring-app-groups)
/// * The [App Groups entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.application-groups?language=objc)
/// * [`containerURLForSecurityApplicationGroupIdentifier`](https://developer.apple.com/documentation/foundation/filemanager/containerurl(forsecurityapplicationgroupidentifier:)?language=objc)
#[cfg(target_os = "macos")]
pub fn app_group_container_path() -> Option<PathBuf> {
    use std::sync::LazyLock;
    static CONTAINER_PATH: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
        use objc2_foundation::{NSFileManager, NSString};

        let fm = NSFileManager::defaultManager();
        // Keep in sync with Entitlements.plist
        let group_id = format!("{}.ai.hermon.wish", crate::macos::APPLE_TEAM_ID);
        let group_id = NSString::from_str(&group_id);
        // containerURLForSecurityApplicationGroupIdentifier always returns a value on macOS (unlike iOS).
        // We have to double-check that the path points to a directory we can actually use. In addition to
        // macOS returning a path that may not exist, processes may list the container directory without
        // having permissions to read to or write from it.
        if let Some(url) = fm.containerURLForSecurityApplicationGroupIdentifier(&group_id) {
            if let Some(ns_path) = url.path() {
                let path = PathBuf::from(ns_path.to_string());
                if tempfile::tempfile_in(&path).is_ok() {
                    return Some(path);
                }
            }
        }

        None
    });
    LazyLock::force(&CONTAINER_PATH).clone()
}

/// Returns the legacy secure state directory from the old `dev.warp` group container.
///
/// When entitlements were updated from `2BBY89MBSN.dev.warp` to `2BBY89MBSN.ai.hermon.wish`,
/// [`secure_state_dir`] stopped resolving the old container. This constructs the old path
/// directly so `init_db` can perform a one-time data migration.
#[cfg(target_os = "macos")]
pub fn legacy_secure_state_dir() -> Option<PathBuf> {
    let project_dirs = project_dirs()?;
    let home = BaseDirs::new()?.home_dir().to_owned();
    let path = home
        .join("Library/Group Containers")
        .join(format!("{}.dev.warp", crate::macos::APPLE_TEAM_ID))
        .join("Library/Application Support")
        .join(project_dirs.project_path());
    path.is_dir().then_some(path)
}

/// Returns the path to resources included in the Wish distribution.
///
/// Unlike [`wishui::AssetProvider`] assets, which are generally embedded in the binary, these are
/// stored on the filesystem alongside the rest of Wish.
///
/// ## macOS
/// The resources directory is `$APP_DIR/Contents/Resources` (e.g. `/Applications/Wish.app/Contents/Resources`).
///
/// ## Linux
/// The resources directory is `$INSTALL_DIR/resources`, where `$INSTALL_DIR` depends on the
/// specific package manager. For example, on Ubuntu this might be `/opt/hermonai/wish-terminal/resources`.
///
/// ## Windows
/// The resources directory is `$INSTALL_DIR/resources`, where `$INSTALL_DIR` is the directory
/// containing the Warp executable (e.g. `C:\Program Files\WarpDev\resources`).
pub fn bundled_resources_dir() -> Option<PathBuf> {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            crate::macos::get_bundle_path().ok()
                .map(|bundle_path| {
                    PathBuf::from(bundle_path)
                        .join("Contents")
                        .join("Resources")
                })
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            std::env::current_exe()
                .ok()
                .and_then(|executable| std::fs::canonicalize(executable).ok())
                .and_then(|executable| executable.parent().map(|parent| parent.join("resources")))
        } else if #[cfg(target_os = "windows")] {
            std::env::current_exe()
                .ok()
                .and_then(|executable| std::fs::canonicalize(executable).ok())
                .and_then(|executable| executable.parent().map(|parent| parent.join("resources")))
        } else {
            None
        }
    }
}

#[cfg(all(test, feature = "local_fs"))]
#[path = "paths_tests.rs"]
mod tests;
