// SPDX-License-Identifier: MIT

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::model::Location;

const SESSION_VERSION: u32 = 1;
const SESSION_FILE_NAME: &str = "tabs.toml";
const MAX_RESTORED_TABS: usize = 32;
const KIND_PATH: &str = "path";
const KIND_URI: &str = "uri";

const RESTORABLE_URI_SCHEMES: [&str; 9] = [
    "smb", "sftp", "ftp", "ftps", "dav", "davs", "trash", "network", "recent",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct StoredTab {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct StoredSession {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    active: usize,
    #[serde(default)]
    tabs: Vec<StoredTab>,
}

/// Validated tabs from the previous session, in strip order.
pub(crate) struct RestoredSession {
    pub tabs: Vec<Location>,
    pub active: usize,
}

pub(crate) fn session_path() -> PathBuf {
    crate::storage::config_directory().join(SESSION_FILE_NAME)
}

/// Persists the window's tabs in strip order with the active tab's position.
/// Unrepresentable entries (such as non-UTF-8 paths) are skipped. Failures are
/// logged and retried on the next save; the in-memory tabs are unaffected.
pub(crate) fn save(tabs: &[Location], active: usize) {
    if let Err(error) = save_to(&session_path(), tabs, active) {
        tracing::warn!(%error, "unable to save open tabs");
    }
}

/// Drops the saved session, if any. Used when tab restore is disabled so no
/// stale tabs outlive the opt-out.
pub(crate) fn remove() {
    let path = session_path();
    if path.is_file()
        && let Err(error) = fs::remove_file(&path)
    {
        tracing::warn!(%error, "unable to clear saved tabs");
    }
}

/// Loads the saved tabs, keeping only locations that are still restorable:
/// existing local directories and well-formed URIs without credentials or
/// transient children. Returns `None` when nothing usable was saved.
pub(crate) fn load_restorable() -> Option<RestoredSession> {
    load_from(&session_path())
}

pub(crate) fn save_to(path: &Path, tabs: &[Location], active: usize) -> io::Result<()> {
    let stored = tabs.iter().filter_map(|location| {
        if let Some(path) = location.native_path() {
            return path.to_str().map(|value| StoredTab::from(KIND_PATH, value));
        }
        location
            .uri_value()
            .map(|value| StoredTab::from(KIND_URI, value))
    });
    let session = StoredSession {
        version: SESSION_VERSION,
        active: active.min(tabs.len().saturating_sub(1)),
        tabs: stored.collect(),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::storage::atomic_write(
        path,
        toml::to_string(&session)
            .map_err(io::Error::other)?
            .as_bytes(),
    )
}

pub(crate) fn load_from(path: &Path) -> Option<RestoredSession> {
    let contents = fs::read_to_string(path).ok()?;
    let stored: StoredSession = toml::from_str(&contents).ok()?;
    if stored.version != SESSION_VERSION {
        return None;
    }
    let tabs: Vec<Location> = stored
        .tabs
        .iter()
        .filter_map(|tab| restorable_location(&tab.kind, &tab.value))
        .take(MAX_RESTORED_TABS)
        .collect();
    if tabs.is_empty() {
        return None;
    }
    Some(RestoredSession {
        active: stored.active.min(tabs.len() - 1),
        tabs,
    })
}

impl StoredTab {
    fn from(kind: &str, value: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            value: value.to_owned(),
        }
    }
}

fn restorable_location(kind: &str, value: &str) -> Option<Location> {
    match kind {
        KIND_PATH => {
            let path = PathBuf::from(value);
            if !path.is_absolute() || !path.is_dir() {
                return None;
            }
            Some(Location::local(path))
        }
        KIND_URI => restorable_uri(value),
        _ => None,
    }
}

fn restorable_uri(value: &str) -> Option<Location> {
    let scheme = value.split("://").next()?.to_ascii_lowercase();
    if !RESTORABLE_URI_SCHEMES.contains(&scheme.as_str()) {
        return None;
    }
    if has_userinfo_credentials(value) {
        return None;
    }
    match scheme.as_str() {
        "trash" if value != "trash:///" => return None,
        "recent" if value != "recent:///" => return None,
        _ => {}
    }
    Some(Location::uri(value))
}

/// Rejects URIs carrying a password or auth parameters. A bare username (as in
/// `smb://user@host/share`) survives sanitization elsewhere and stays
/// restorable; secrets never reach the session file.
fn has_userinfo_credentials(value: &str) -> bool {
    let Some(after_scheme) = value.split_once("://").map(|(_, rest)| rest) else {
        return true;
    };
    let authority = after_scheme.split('/').next().unwrap_or_default();
    let Some((userinfo, _)) = authority.split_once('@') else {
        return false;
    };
    userinfo.contains([':', ';'])
}

#[cfg(test)]
mod tests;
