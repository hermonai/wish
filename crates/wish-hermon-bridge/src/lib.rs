//! Wish Hermon Bridge — façade over `hermon_client`, lifted to the World
//! Model layer.
//!
//! v0.5.0 ships **type signatures and a stub `RoutingHint` resolver** only.
//! Wiring to the live `hermon_client` lands in `v0.5.0-step-10`. This
//! keeps the crate compilable and lets dependent crates code against the
//! façade today.

use serde::{Deserialize, Serialize};
use wish_world_model::{WishWorld, WorldKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingHint {
    /// Hermon-recommended model id, e.g., `claude-opus-4-7` or
    /// `ollama/qwen2.5-coder`.
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub reason: String,
}

/// v0.5.0 stub: choose a model from world kind and intent length.
///
/// Replaced by a live Hermon call in v0.5.0-step-10.
pub fn routing_hint(world: &WishWorld, intent: &str) -> RoutingHint {
    let big = intent.len() > 240
        || matches!(
            world.kind,
            WorldKind::FinalverseRegion | WorldKind::FintechDemo | WorldKind::EducationWorld
        );
    if big {
        RoutingHint {
            model: "claude-opus-4-7".into(),
            temperature: 0.4,
            max_tokens: 8192,
            reason: "complex world or long intent → frontier model".into(),
        }
    } else {
        RoutingHint {
            model: "ollama/qwen2.5-coder".into(),
            temperature: 0.2,
            max_tokens: 2048,
            reason: "short intent on a generic world → local model".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_routing_short() {
        let w = WishWorld::new("t", WorldKind::GenericProject);
        let h = routing_hint(&w, "fix bug");
        assert!(h.model.contains("ollama"));
    }

    #[test]
    fn smoke_routing_long() {
        let w = WishWorld::new("t", WorldKind::FintechDemo);
        let h = routing_hint(
            &w,
            "build a Shan Hai education world that teaches stablecoin and credit.",
        );
        assert_eq!(h.model, "claude-opus-4-7");
    }
}
