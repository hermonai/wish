//! `SemanticId` — the universal identifier.
//!
//! See `wish-design/wish-plan-20260514/04-data-model/01-semantic-id.md` for
//! the canonical contract.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Namespacing axis for a [`SemanticId`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Realm {
    Code,
    Repo,
    Terminal,
    Diagnostics,
    Agent,
    World,
    Scene,
    Canvas,
    Asset,
    Service,
    Npc,
    Quest,
    Finance,
    Custom(String),
}

/// Universal identifier for anything Wish "knows about."
///
/// `(realm, kind, stable_key)` is content-derived; `instance` disambiguates
/// truly duplicate stable keys (e.g., two terminal blocks from the same
/// command).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemanticId {
    pub realm: Realm,
    pub kind: String,
    pub stable_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl SemanticId {
    pub fn new(realm: Realm, kind: impl Into<String>, stable_key: impl Into<String>) -> Self {
        Self {
            realm,
            kind: kind.into(),
            stable_key: stable_key.into(),
            instance: None,
        }
    }

    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    pub fn code_file(path: &str) -> Self {
        Self::new(Realm::Code, "file", path)
    }

    pub fn code_function(qualified_name: &str) -> Self {
        Self::new(Realm::Code, "function", qualified_name)
    }

    pub fn code_crate(name: &str) -> Self {
        Self::new(Realm::Code, "crate", name)
    }

    pub fn terminal_block(stable_key: impl Into<String>) -> Self {
        Self::new(Realm::Terminal, "block", stable_key)
    }

    pub fn agent_run(session_id: impl Into<String>) -> Self {
        Self::new(Realm::Agent, "run", session_id)
    }
}

impl fmt::Display for SemanticId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let realm = match &self.realm {
            Realm::Code => "code",
            Realm::Repo => "repo",
            Realm::Terminal => "terminal",
            Realm::Diagnostics => "diagnostics",
            Realm::Agent => "agent",
            Realm::World => "world",
            Realm::Scene => "scene",
            Realm::Canvas => "canvas",
            Realm::Asset => "asset",
            Realm::Service => "service",
            Realm::Npc => "npc",
            Realm::Quest => "quest",
            Realm::Finance => "finance",
            Realm::Custom(s) => s.as_str(),
        };
        match &self.instance {
            Some(inst) => write!(f, "{}:{}:{}#{}", realm, self.kind, self.stable_key, inst),
            None => write!(f, "{}:{}:{}", realm, self.kind, self.stable_key),
        }
    }
}

/// Error produced when [`SemanticId::from_str`] fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSemanticIdError {
    pub input: String,
    pub reason: &'static str,
}

impl fmt::Display for ParseSemanticIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid SemanticId {:?}: {}", self.input, self.reason)
    }
}

impl std::error::Error for ParseSemanticIdError {}

impl FromStr for SemanticId {
    type Err = ParseSemanticIdError;

    /// Parse the canonical `realm:kind:stable_key[#instance]` form.
    ///
    /// The `stable_key` may contain `:` characters (e.g. qualified
    /// names like `editor::Editor::new`), so we split on the first
    /// two colons only.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = |reason| ParseSemanticIdError {
            input: s.to_string(),
            reason,
        };
        // Split off optional `#instance` suffix.
        let (body, instance) = match s.rsplit_once('#') {
            Some((b, i)) if !i.is_empty() => (b, Some(i.to_string())),
            _ => (s, None),
        };
        let mut parts = body.splitn(3, ':');
        let realm_str = parts.next().ok_or_else(|| err("missing realm"))?;
        let kind = parts.next().ok_or_else(|| err("missing kind"))?;
        let stable_key = parts.next().ok_or_else(|| err("missing stable_key"))?;
        if realm_str.is_empty() || kind.is_empty() || stable_key.is_empty() {
            return Err(err("empty component"));
        }
        let realm = match realm_str {
            "code" => Realm::Code,
            "repo" => Realm::Repo,
            "terminal" => Realm::Terminal,
            "diagnostics" => Realm::Diagnostics,
            "agent" => Realm::Agent,
            "world" => Realm::World,
            "scene" => Realm::Scene,
            "canvas" => Realm::Canvas,
            "asset" => Realm::Asset,
            "service" => Realm::Service,
            "npc" => Realm::Npc,
            "quest" => Realm::Quest,
            "finance" => Realm::Finance,
            other => Realm::Custom(other.to_string()),
        };
        Ok(SemanticId {
            realm,
            kind: kind.to_string(),
            stable_key: stable_key.to_string(),
            instance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format() {
        let id = SemanticId::code_function("editor::Editor::new");
        assert_eq!(format!("{id}"), "code:function:editor::Editor::new");
    }

    #[test]
    fn display_with_instance() {
        let id = SemanticId::terminal_block("cargo-test").with_instance("01HXYZ");
        assert_eq!(format!("{id}"), "terminal:block:cargo-test#01HXYZ");
    }

    #[test]
    fn equality_ignores_serialization() {
        let a = SemanticId::code_file("src/lib.rs");
        let b = SemanticId::code_file("src/lib.rs");
        assert_eq!(a, b);
    }

    #[test]
    fn from_str_roundtrip_basic() {
        let id = SemanticId::code_file("src/lib.rs");
        let s = id.to_string();
        let parsed: SemanticId = s.parse().unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn from_str_roundtrip_qualified_function() {
        // stable_key contains `::` which is two colons each — the
        // parser must split on the *first two* colons only.
        let id = SemanticId::code_function("editor::Editor::new");
        let parsed: SemanticId = id.to_string().parse().unwrap();
        assert_eq!(parsed, id);
        assert_eq!(parsed.stable_key, "editor::Editor::new");
    }

    #[test]
    fn from_str_roundtrip_with_instance() {
        let id = SemanticId::terminal_block("cargo-test").with_instance("01HXYZ");
        let parsed: SemanticId = id.to_string().parse().unwrap();
        assert_eq!(parsed, id);
        assert_eq!(parsed.instance.as_deref(), Some("01HXYZ"));
    }

    #[test]
    fn from_str_custom_realm() {
        let parsed: SemanticId = "music:track:moon-river".parse().unwrap();
        assert_eq!(parsed.realm, Realm::Custom("music".to_string()));
        assert_eq!(parsed.kind, "track");
        assert_eq!(parsed.stable_key, "moon-river");
    }

    #[test]
    fn from_str_rejects_missing_parts() {
        assert!("code:file".parse::<SemanticId>().is_err());
        assert!("code".parse::<SemanticId>().is_err());
        assert!("".parse::<SemanticId>().is_err());
        assert!("::".parse::<SemanticId>().is_err());
    }

    #[test]
    fn from_str_ignores_lone_hash() {
        // A trailing `#` with no instance is treated as absent.
        let parsed: SemanticId = "code:file:src/main.rs#".parse().unwrap();
        assert_eq!(parsed.instance, None);
    }
}
