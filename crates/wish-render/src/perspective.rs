//! Domain perspectives — eight lenses on the same world.
//!
//! A `Perspective` is a *lens* the viewer applies to a `Canvas` +
//! `WishWorld`. Same SemanticIds, same world model, but the **palette,
//! layout, view mode, and edge emphasis** change to match the domain.
//!
//! This is what makes Wish unique in the world: no other IDE / agent
//! cockpit / 3D editor lets a user toggle from `Engineering` to
//! `Financial` to `Education` on a single canvas without losing
//! identity — every visible object keeps its `SemanticId` across
//! perspectives, so cross-perspective navigation just works.

use eframe::egui::Color32;
use wish_canvas_core::types::{CanvasNodeKind, EdgeKind, LayoutMode};

/// Fifteen first-class lenses, grouped into two tiers:
///
/// **Domain lenses** (vibe-coding / business / creation):
/// `Engineering`, `Architecture`, `Spatial`, `Financial`, `Education`,
/// `Scientific`, `Design`, `Analytic`.
///
/// **Scientific lenses** — the Tensorium fundamentals every higher-
/// order domain inherits from: `Math`, `Geometry`, `Chemistry`,
/// `Physics`, `Linguistic`, `Geologic`, `Biologic`.
///
/// See `wish-design/wish-plan-20260514/01-strategy/06-the-tensorium.md`
/// for the strategic frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Perspective {
    // -- Domain lenses --
    /// Function/file/crate codegraph. The default for coding.
    Engineering,
    /// Top-level architecture — one node per crate, dep edges only.
    Architecture,
    /// Living-world view — NPCs, sacred sites, scenes, Transforms.
    Spatial,
    /// Money flow — `EconomicActor` profile drives color and edges.
    Financial,
    /// Curriculum graph — teacher, students, lessons, halls of learning.
    Education,
    /// Research graph — hypotheses, experiments, results.
    Scientific,
    /// Design system — components, variants, instances.
    Design,
    /// Data pipeline — sources, transforms, metrics, dashboards.
    Analytic,
    // -- Scientific (Tensorium-fundamental) lenses --
    /// Math: concept graph, theorems, proofs.
    Math,
    /// Geometry: shapes, transforms, dimensions, constructions.
    Geometry,
    /// Chemistry: atoms, bonds, reactions, molecules.
    Chemistry,
    /// Physics: position / momentum / energy / time / mass / charge / spin.
    Physics,
    /// Linguistic: conversation graph, dialog turns, semantic edges.
    Linguistic,
    /// Geologic: strata, depth, age, formations.
    Geologic,
    /// Biologic: species, taxa, ecology, trophic levels.
    Biologic,
}

/// Category of a perspective — used to group entries in the toolbar
/// dropdown and in CLI help output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerspectiveCategory {
    Domain,
    Science,
}

impl Default for Perspective {
    fn default() -> Self {
        Perspective::Engineering
    }
}

impl Perspective {
    pub const ALL: [Perspective; 15] = [
        // Domain
        Perspective::Engineering,
        Perspective::Architecture,
        Perspective::Spatial,
        Perspective::Financial,
        Perspective::Education,
        Perspective::Scientific,
        Perspective::Design,
        Perspective::Analytic,
        // Scientific (Tensorium fundamentals)
        Perspective::Math,
        Perspective::Geometry,
        Perspective::Chemistry,
        Perspective::Physics,
        Perspective::Linguistic,
        Perspective::Geologic,
        Perspective::Biologic,
    ];

    /// Which category this perspective belongs to.
    pub fn category(self) -> PerspectiveCategory {
        match self {
            Perspective::Engineering
            | Perspective::Architecture
            | Perspective::Spatial
            | Perspective::Financial
            | Perspective::Education
            | Perspective::Scientific
            | Perspective::Design
            | Perspective::Analytic => PerspectiveCategory::Domain,
            Perspective::Math
            | Perspective::Geometry
            | Perspective::Chemistry
            | Perspective::Physics
            | Perspective::Linguistic
            | Perspective::Geologic
            | Perspective::Biologic => PerspectiveCategory::Science,
        }
    }

    /// Short label for the toolbar dropdown.
    pub fn label(self) -> &'static str {
        match self {
            Perspective::Engineering => "🛠 Engineering",
            Perspective::Architecture => "🏛 Architecture",
            Perspective::Spatial => "🌐 Spatial",
            Perspective::Financial => "💰 Financial",
            Perspective::Education => "📚 Education",
            Perspective::Scientific => "🧪 Scientific",
            Perspective::Design => "🎨 Design",
            Perspective::Analytic => "📊 Analytic",
            Perspective::Math => "∑ Math",
            Perspective::Geometry => "△ Geometry",
            Perspective::Chemistry => "⚗ Chemistry",
            Perspective::Physics => "⚛ Physics",
            Perspective::Linguistic => "🗣 Linguistic",
            Perspective::Geologic => "🪨 Geologic",
            Perspective::Biologic => "🧬 Biologic",
        }
    }

    /// Machine-readable name accepted by the CLI `--perspective` flag.
    pub fn slug(self) -> &'static str {
        match self {
            Perspective::Engineering => "engineering",
            Perspective::Architecture => "architecture",
            Perspective::Spatial => "spatial",
            Perspective::Financial => "financial",
            Perspective::Education => "education",
            Perspective::Scientific => "scientific",
            Perspective::Design => "design",
            Perspective::Analytic => "analytic",
            Perspective::Math => "math",
            Perspective::Geometry => "geometry",
            Perspective::Chemistry => "chemistry",
            Perspective::Physics => "physics",
            Perspective::Linguistic => "linguistic",
            Perspective::Geologic => "geologic",
            Perspective::Biologic => "biologic",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "engineering" | "eng" | "code" => Perspective::Engineering,
            "architecture" | "arch" => Perspective::Architecture,
            "spatial" | "world" => Perspective::Spatial,
            "financial" | "finance" => Perspective::Financial,
            "education" | "edu" | "school" => Perspective::Education,
            "scientific" | "science" | "research" => Perspective::Scientific,
            "design" | "ux" => Perspective::Design,
            "analytic" | "analytics" | "data" => Perspective::Analytic,
            "math" | "mathematics" => Perspective::Math,
            "geometry" | "geo" | "shape" => Perspective::Geometry,
            "chemistry" | "chem" | "molecular" => Perspective::Chemistry,
            "physics" | "phys" | "mechanics" => Perspective::Physics,
            "linguistic" | "language" | "chat" | "dialog" => Perspective::Linguistic,
            "geologic" | "geology" | "strata" => Perspective::Geologic,
            "biologic" | "biology" | "bio" | "ecology" => Perspective::Biologic,
            _ => return None,
        })
    }

    /// Tagline shown beneath the toolbar dropdown — one sentence per
    /// perspective so the user always knows what lens they're in.
    pub fn tagline(self) -> &'static str {
        match self {
            Perspective::Engineering => "function / file / crate / call — the codegraph kingdom.",
            Perspective::Architecture => "one node per crate. dep edges only. post-UML.",
            Perspective::Spatial => "NPCs, sacred sites, scenes. positions in 3D.",
            Perspective::Financial => "money flow. economic actors tinted by profile.",
            Perspective::Education => "teachers, students, lessons, halls of learning.",
            Perspective::Scientific => "hypotheses, experiments, results, citations.",
            Perspective::Design => "components, variants, instances, tokens.",
            Perspective::Analytic => "sources, transforms, metrics, dashboards.",
            Perspective::Math => "concepts, theorems, proofs — the symbolic kingdom.",
            Perspective::Geometry => "shapes, transforms, dimensions, constructions.",
            Perspective::Chemistry => "atoms, bonds, reactions — molecular graph.",
            Perspective::Physics => "position, momentum, energy, time — the physical Tensorium.",
            Perspective::Linguistic => "turns, speakers, sentiment, topics — conversation graph.",
            Perspective::Geologic => "strata, depth, age, formations — the deep time axis.",
            Perspective::Biologic => "species, trophic levels, ecology, genome.",
        }
    }

    /// Default canvas layout for this lens.
    pub fn default_layout(self) -> LayoutMode {
        match self {
            Perspective::Engineering => LayoutMode::ForceDirected,
            Perspective::Architecture => LayoutMode::Layered,
            Perspective::Spatial => LayoutMode::ForceDirected,
            Perspective::Financial => LayoutMode::ForceDirected,
            Perspective::Education => LayoutMode::Layered,
            Perspective::Scientific => LayoutMode::Layered,
            Perspective::Design => LayoutMode::Grid,
            Perspective::Analytic => LayoutMode::Layered,
            Perspective::Math => LayoutMode::Layered, // proof-depth as Y
            Perspective::Geometry => LayoutMode::ForceDirected,
            Perspective::Chemistry => LayoutMode::ForceDirected, // bond-aware later
            Perspective::Physics => LayoutMode::ForceDirected,
            Perspective::Linguistic => LayoutMode::Layered, // turns as a stack
            Perspective::Geologic => LayoutMode::Layered, // strata are layered by definition
            Perspective::Biologic => LayoutMode::Layered, // taxonomic tree
        }
    }

    /// Whether this lens prefers the 3D scene view by default. The user
    /// can still toggle freely.
    pub fn prefers_3d(self) -> bool {
        // Spatial is the canonical 3D lens. Physics, Geometry, and
        // Geologic also have natural 3D rendering once the Tensorium
        // axes are populated — for v0.5.0 they default to 2D and the
        // user can toggle.
        matches!(self, Perspective::Spatial)
    }

    /// Which edge kinds should be emphasized (drawn at full opacity).
    /// All other edges are still drawn but faded.
    pub fn emphasized_edges(self) -> &'static [EdgeKind] {
        match self {
            Perspective::Engineering => &[EdgeKind::Calls, EdgeKind::DependsOn, EdgeKind::Imports],
            Perspective::Architecture => &[EdgeKind::DependsOn],
            Perspective::Spatial => &[EdgeKind::Mentions, EdgeKind::Spawned],
            Perspective::Financial => &[EdgeKind::Produces, EdgeKind::Triggers],
            Perspective::Education => &[EdgeKind::Mentions, EdgeKind::Spawned, EdgeKind::Tests],
            Perspective::Scientific => &[EdgeKind::Produces, EdgeKind::SucceededBy, EdgeKind::FailedBy],
            Perspective::Design => &[EdgeKind::DependsOn, EdgeKind::Imports],
            Perspective::Analytic => &[EdgeKind::Produces, EdgeKind::Triggers],
            // Scientific lenses default to structural edges plus
            // domain-natural ones. Math: implies (Calls used as "→").
            Perspective::Math => &[EdgeKind::Calls, EdgeKind::DependsOn],
            // Geometry: spatial neighbors (no first-class shape edge yet).
            Perspective::Geometry => &[EdgeKind::DependsOn, EdgeKind::Mentions],
            // Chemistry: bond as DependsOn.
            Perspective::Chemistry => &[EdgeKind::DependsOn],
            // Physics: causal edges.
            Perspective::Physics => &[EdgeKind::Triggers, EdgeKind::Produces, EdgeKind::SucceededBy],
            // Linguistic: reply chain via SucceededBy + reference via Mentions.
            Perspective::Linguistic => &[EdgeKind::Mentions, EdgeKind::SucceededBy],
            // Geologic: depositional sequence as SucceededBy.
            Perspective::Geologic => &[EdgeKind::SucceededBy],
            // Biologic: lineage as SucceededBy, predation as Produces.
            Perspective::Biologic => &[EdgeKind::SucceededBy, EdgeKind::Produces],
        }
    }

    /// Per-perspective tint for a node kind. Falls back to a sensible
    /// default when the kind isn't specifically painted by this lens.
    pub fn tint(self, kind: &CanvasNodeKind) -> Color32 {
        let base = default_tint(kind);
        match self {
            Perspective::Engineering => base,
            Perspective::Architecture => match kind {
                CanvasNodeKind::Crate => Color32::from_rgb(180, 130, 70),
                CanvasNodeKind::Package | CanvasNodeKind::Module => Color32::from_rgb(140, 110, 160),
                _ => fade(base, 0.5),
            },
            Perspective::Spatial => match kind {
                CanvasNodeKind::Npc => Color32::from_rgb(190, 140, 160),
                CanvasNodeKind::Custom(s) if s.contains("sacred") => {
                    Color32::from_rgb(220, 180, 110)
                }
                CanvasNodeKind::Custom(s) if s == "world" => Color32::from_rgb(70, 110, 160),
                CanvasNodeKind::Quest => Color32::from_rgb(150, 130, 80),
                _ => base,
            },
            Perspective::Financial => match kind {
                CanvasNodeKind::Npc => Color32::from_rgb(220, 180, 90),
                CanvasNodeKind::Custom(s) if s.contains("sacred") => {
                    Color32::from_rgb(120, 120, 110)
                }
                _ => fade(base, 0.6),
            },
            Perspective::Education => match kind {
                CanvasNodeKind::Npc => Color32::from_rgb(150, 200, 130),
                CanvasNodeKind::Agent => Color32::from_rgb(120, 170, 220),
                CanvasNodeKind::Custom(s) if s.contains("sacred") => {
                    Color32::from_rgb(180, 170, 90)
                }
                _ => fade(base, 0.6),
            },
            Perspective::Scientific => match kind {
                CanvasNodeKind::Test => Color32::from_rgb(80, 200, 130),
                CanvasNodeKind::Function => Color32::from_rgb(120, 160, 200),
                CanvasNodeKind::Custom(_) => Color32::from_rgb(170, 130, 200),
                _ => fade(base, 0.6),
            },
            Perspective::Design => match kind {
                CanvasNodeKind::Custom(_) => Color32::from_rgb(200, 140, 180),
                CanvasNodeKind::Module | CanvasNodeKind::Package => {
                    Color32::from_rgb(150, 110, 160)
                }
                _ => fade(base, 0.55),
            },
            Perspective::Analytic => match kind {
                CanvasNodeKind::Service => Color32::from_rgb(70, 130, 180),
                CanvasNodeKind::Test => Color32::from_rgb(110, 180, 110),
                CanvasNodeKind::Function => Color32::from_rgb(140, 150, 170),
                _ => fade(base, 0.6),
            },
            // -- scientific lenses --
            Perspective::Math => match kind {
                CanvasNodeKind::Function => Color32::from_rgb(120, 160, 220),
                CanvasNodeKind::Custom(_) => Color32::from_rgb(170, 130, 220),
                _ => fade(base, 0.55),
            },
            Perspective::Geometry => match kind {
                CanvasNodeKind::Custom(_) => Color32::from_rgb(220, 170, 120),
                CanvasNodeKind::Module => Color32::from_rgb(180, 140, 110),
                _ => fade(base, 0.55),
            },
            Perspective::Chemistry => match kind {
                CanvasNodeKind::Custom(s) if s.contains("atom") => {
                    Color32::from_rgb(200, 90, 90) // red-ish atoms by default
                }
                CanvasNodeKind::Custom(s) if s.contains("bond") => {
                    Color32::from_rgb(140, 140, 160)
                }
                CanvasNodeKind::Custom(s) if s.contains("molecule") => {
                    Color32::from_rgb(110, 180, 110)
                }
                _ => fade(base, 0.6),
            },
            Perspective::Physics => match kind {
                CanvasNodeKind::Custom(s) if s.contains("particle") => {
                    Color32::from_rgb(220, 200, 80)
                }
                CanvasNodeKind::Custom(s) if s.contains("field") => {
                    Color32::from_rgb(80, 160, 200)
                }
                CanvasNodeKind::Custom(s) if s.contains("force") => {
                    Color32::from_rgb(200, 110, 80)
                }
                _ => fade(base, 0.6),
            },
            Perspective::Linguistic => match kind {
                CanvasNodeKind::Custom(s) if s.contains("turn") => {
                    Color32::from_rgb(120, 180, 220)
                }
                CanvasNodeKind::Custom(s) if s.contains("speaker") => {
                    Color32::from_rgb(220, 160, 200)
                }
                CanvasNodeKind::Agent => Color32::from_rgb(140, 190, 220),
                CanvasNodeKind::Npc => Color32::from_rgb(190, 140, 200),
                CanvasNodeKind::DocumentSection => Color32::from_rgb(160, 160, 180),
                _ => fade(base, 0.6),
            },
            Perspective::Geologic => match kind {
                CanvasNodeKind::Custom(s) if s.contains("stratum") || s.contains("layer") => {
                    Color32::from_rgb(160, 110, 80)
                }
                CanvasNodeKind::Custom(s) if s.contains("formation") => {
                    Color32::from_rgb(120, 100, 90)
                }
                _ => fade(base, 0.5),
            },
            Perspective::Biologic => match kind {
                CanvasNodeKind::Custom(s) if s.contains("species") => {
                    Color32::from_rgb(120, 180, 110)
                }
                CanvasNodeKind::Custom(s) if s.contains("genome") => {
                    Color32::from_rgb(180, 140, 120)
                }
                CanvasNodeKind::Npc => Color32::from_rgb(140, 200, 130),
                _ => fade(base, 0.55),
            },
        }
    }
}

/// Default kind→color used by the Engineering perspective. Other
/// perspectives override selectively and fall back to this.
pub fn default_tint(kind: &CanvasNodeKind) -> Color32 {
    match kind {
        CanvasNodeKind::File => Color32::from_rgb(56, 80, 110),
        CanvasNodeKind::Function => Color32::from_rgb(82, 112, 70),
        CanvasNodeKind::Crate => Color32::from_rgb(150, 100, 50),
        CanvasNodeKind::Package | CanvasNodeKind::Module => Color32::from_rgb(110, 90, 130),
        CanvasNodeKind::Service => Color32::from_rgb(70, 110, 140),
        CanvasNodeKind::Agent => Color32::from_rgb(140, 90, 130),
        CanvasNodeKind::ToolCall | CanvasNodeKind::PlanStep => Color32::from_rgb(120, 110, 60),
        CanvasNodeKind::Test => Color32::from_rgb(80, 130, 90),
        CanvasNodeKind::Commit | CanvasNodeKind::Branch => Color32::from_rgb(90, 100, 130),
        CanvasNodeKind::Diff => Color32::from_rgb(150, 80, 70),
        CanvasNodeKind::TerminalBlock => Color32::from_rgb(50, 60, 60),
        CanvasNodeKind::DocumentSection => Color32::from_rgb(80, 90, 100),
        CanvasNodeKind::Npc => Color32::from_rgb(160, 110, 130),
        CanvasNodeKind::Quest => Color32::from_rgb(130, 120, 70),
        CanvasNodeKind::Custom(_) => Color32::from_rgb(90, 100, 110),
    }
}

fn fade(c: Color32, factor: f32) -> Color32 {
    let factor = factor.clamp(0.0, 1.0);
    let r = (c.r() as f32 * factor) as u8;
    let g = (c.g() as f32 * factor) as u8;
    let b = (c.b() as f32 * factor) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, c.a().max(200))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_slug_round_trips_for_all_perspectives() {
        for p in Perspective::ALL {
            let back = Perspective::from_slug(p.slug()).unwrap();
            assert_eq!(back, p);
        }
    }

    #[test]
    fn from_slug_accepts_aliases() {
        assert_eq!(Perspective::from_slug("eng"), Some(Perspective::Engineering));
        assert_eq!(Perspective::from_slug("arch"), Some(Perspective::Architecture));
        assert_eq!(Perspective::from_slug("finance"), Some(Perspective::Financial));
        assert_eq!(Perspective::from_slug("ux"), Some(Perspective::Design));
        assert_eq!(Perspective::from_slug("data"), Some(Perspective::Analytic));
    }

    #[test]
    fn from_slug_rejects_unknown() {
        assert_eq!(Perspective::from_slug("nope"), None);
    }

    #[test]
    fn perspectives_have_labels_and_taglines() {
        for p in Perspective::ALL {
            assert!(!p.label().is_empty());
            assert!(!p.tagline().is_empty());
            assert!(!p.slug().is_empty());
        }
    }

    #[test]
    fn only_spatial_prefers_3d_by_default() {
        for p in Perspective::ALL {
            if p == Perspective::Spatial {
                assert!(p.prefers_3d());
            } else {
                assert!(!p.prefers_3d(), "{:?} should not default to 3D", p);
            }
        }
    }

    #[test]
    fn architecture_emphasizes_dependson_only() {
        assert_eq!(
            Perspective::Architecture.emphasized_edges(),
            &[EdgeKind::DependsOn]
        );
    }

    #[test]
    fn all_perspectives_count_is_fifteen() {
        assert_eq!(Perspective::ALL.len(), 15);
    }

    #[test]
    fn categories_split_into_domain_and_science() {
        let mut domain = 0;
        let mut science = 0;
        for p in Perspective::ALL {
            match p.category() {
                PerspectiveCategory::Domain => domain += 1,
                PerspectiveCategory::Science => science += 1,
            }
        }
        assert_eq!(domain, 8);
        assert_eq!(science, 7);
    }

    #[test]
    fn scientific_perspective_slugs_parse() {
        for slug in &[
            "math",
            "geometry",
            "chemistry",
            "physics",
            "linguistic",
            "geologic",
            "biologic",
        ] {
            assert!(
                Perspective::from_slug(slug).is_some(),
                "slug should parse: {slug}"
            );
            // Also verify the matched perspective is in the Science category.
            let p = Perspective::from_slug(slug).unwrap();
            assert_eq!(p.category(), PerspectiveCategory::Science);
        }
    }

    #[test]
    fn scientific_aliases_parse() {
        assert_eq!(Perspective::from_slug("mathematics"), Some(Perspective::Math));
        assert_eq!(Perspective::from_slug("chem"), Some(Perspective::Chemistry));
        assert_eq!(Perspective::from_slug("phys"), Some(Perspective::Physics));
        assert_eq!(Perspective::from_slug("chat"), Some(Perspective::Linguistic));
        assert_eq!(Perspective::from_slug("strata"), Some(Perspective::Geologic));
        assert_eq!(Perspective::from_slug("ecology"), Some(Perspective::Biologic));
    }
}
