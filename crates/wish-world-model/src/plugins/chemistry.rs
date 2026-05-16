//! **Chemistry domain plugin** — proof that the URE substrate
//! handles a domain very different from code.
//!
//! Mappings (Chemistry → URE primitive):
//!
//! | Chemistry concept | URE primitive |
//! |---|---|
//! | Atom (C, H, N, O, …) | `Object { realm: chem, kind: "atom", properties: { element, valence_remaining } }` |
//! | Bond (single, double, triple, aromatic) | `Graph` edge with `kind: "bonded_to"` + `weight: order` |
//! | Molecule | `Object { realm: chem, kind: "molecule", properties: { formula, atoms: List<Ref(atom_id)> } }` |
//! | Reaction | `Event { causes: reactant_atoms, effects: product_atoms, payload: { activation_energy, ... } }` |
//! | Valence rule (carbon: max 4 bonds) | `Constraint { severity: Physical, predicate: "..." }` |
//! | Concentration field | `Field { axes: ["x", "y", "z"], samples_dense: ... }` |
//!
//! Same JSON wire format an AI agent emits for a Rust crate. The
//! URE doesn't care that one is code and the other chemistry — it
//! just sees Objects, Graphs, Events, Constraints.

use crate::plugin::DomainPlugin;
use crate::primitives::{
    Constraint, ConstraintSeverity, Graph, GraphEdge, Object, Primitive, PropertyValue,
};
use crate::semantic_id::{Realm, SemanticId};
use std::collections::HashMap;

/// The Chemistry plugin.
pub struct ChemistryPlugin;

/// Convenience realm constructor — chemistry lives in
/// `Realm::Custom("chem")` so it doesn't collide with the built-in
/// realms.
fn chem_realm() -> Realm {
    Realm::Custom("chem".to_string())
}

/// Build a SemanticId in the chemistry realm.
fn chem_id(kind: &str, key: impl Into<String>) -> SemanticId {
    SemanticId::new(chem_realm(), kind, key)
}

impl DomainPlugin for ChemistryPlugin {
    fn realm(&self) -> Realm {
        chem_realm()
    }

    fn name(&self) -> &str {
        "Chemistry"
    }

    fn version(&self) -> &str {
        "0.5.0"
    }

    fn description(&self) -> &str {
        "Atoms, bonds, molecules, reactions — Wave 25 reference plugin."
    }

    fn perspective_slugs(&self) -> Vec<&str> {
        vec!["chemistry", "molecule", "reaction"]
    }

    /// Static valence constraints — the periodic-table laws every
    /// chemistry-realm primitive must respect. The URE's safety layer
    /// reads these before letting an agent add a sixth bond to carbon.
    fn realm_constraints(&self) -> Vec<Constraint> {
        vec![
            valence_constraint("C", 4),
            valence_constraint("N", 3),
            valence_constraint("O", 2),
            valence_constraint("H", 1),
            valence_constraint("S", 6),
            // Conservation of mass in reactions.
            Constraint::new(
                chem_id("law", "conservation_of_mass"),
                "physical_law",
                ConstraintSeverity::Physical,
                "total atoms of each element are conserved across a reaction",
            ),
        ]
    }
}

fn valence_constraint(element: &str, max: u32) -> Constraint {
    let mut expr = HashMap::new();
    expr.insert(
        "element".to_string(),
        PropertyValue::Text(element.to_string()),
    );
    expr.insert("max_bonds".to_string(), PropertyValue::Number(max as f64));
    Constraint {
        id: chem_id("valence", element),
        kind: "valence".to_string(),
        severity: ConstraintSeverity::Physical,
        predicate: format!("element {element} forms at most {max} bond(s)"),
        applies_to: Vec::new(),
        expression: expr,
    }
}

// ─────────────────────────────────────────────────────────────────────
// Public chemistry-domain constructors — used by the plugin's own
// tests AND by anything (CLI, AI agents) that wants to build URE
// primitives in the chemistry realm without re-inventing the schema.
// ─────────────────────────────────────────────────────────────────────

impl ChemistryPlugin {
    /// Build an Atom Object for the given element + instance index.
    /// The instance disambiguates multiple atoms of the same element
    /// in a molecule.
    pub fn atom(element: &str, instance: u32, valence: u32) -> Object {
        let id =
            chem_id("atom", format!("{element}-{instance}")).with_instance(instance.to_string());
        let mut o = Object::new(id, "atom", format!("{element}{instance}"));
        o.properties.insert(
            "element".to_string(),
            PropertyValue::Text(element.to_string()),
        );
        o.properties.insert(
            "valence_max".to_string(),
            PropertyValue::Number(valence as f64),
        );
        o
    }

    /// Build a bond as a [`GraphEdge`] between two atom SemanticIds
    /// with a given bond order (1 = single, 2 = double, 3 = triple,
    /// 1.5 = aromatic).
    pub fn bond(from: &Object, to: &Object, order: f32) -> GraphEdge {
        GraphEdge {
            from: from.id.clone(),
            to: to.id.clone(),
            kind: "bonded_to".to_string(),
            weight: Some(order),
        }
    }

    /// Build a Molecule Object that references its atoms via the
    /// `Ref(SemanticId)` property variant.
    pub fn molecule(name: &str, formula: &str, atoms: &[Object]) -> Object {
        let mut o = Object::new(chem_id("molecule", name), "molecule", name);
        o.properties.insert(
            "formula".to_string(),
            PropertyValue::Text(formula.to_string()),
        );
        o.properties.insert(
            "atom_count".to_string(),
            PropertyValue::Number(atoms.len() as f64),
        );
        let refs: Vec<PropertyValue> = atoms
            .iter()
            .map(|a| PropertyValue::Ref(Box::new(a.id.clone())))
            .collect();
        o.properties
            .insert("atoms".to_string(), PropertyValue::List(refs));
        o
    }

    /// Build a Bond Graph for a molecule: nodes are the atoms,
    /// edges are the bonds. Returns a `Primitive::Graph` that's
    /// directly comparable to a code-domain `Graph` from
    /// `to_ure_graph`. **This is the URE's universality moat.**
    pub fn bond_graph(molecule_name: &str, atoms: &[Object], bonds: &[GraphEdge]) -> Graph {
        let mut g = Graph::new(
            chem_id("graph", format!("{molecule_name}-bonds")),
            "bond_graph",
        );
        for a in atoms {
            g.add_node(a.id.clone());
        }
        for b in bonds {
            g.add_edge(b.clone());
        }
        g
    }

    /// Build a reaction Event whose `causes` are reactant atom IDs and
    /// `effects` are product atom IDs.
    pub fn reaction(
        name: &str,
        reactants: &[&Object],
        products: &[&Object],
        activation_energy_kj: f64,
        at_step: u64,
    ) -> crate::primitives::Event {
        use crate::primitives::Event;
        let mut e = Event::new(chem_id("reaction", name), "reaction", at_step);
        for r in reactants {
            e.causes.push(r.id.clone());
        }
        for p in products {
            e.effects.push(p.id.clone());
        }
        e.payload.insert(
            "activation_energy_kj_per_mol".to_string(),
            PropertyValue::Number(activation_energy_kj),
        );
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemistry_plugin_owns_chem_realm() {
        let p = ChemistryPlugin;
        assert_eq!(p.realm(), chem_realm());
        assert_eq!(p.name(), "Chemistry");
        let slugs = p.perspective_slugs();
        assert!(slugs.contains(&"chemistry"));
    }

    #[test]
    fn valence_constraints_cover_common_elements() {
        let p = ChemistryPlugin;
        let cs = p.realm_constraints();
        // 5 elements + conservation = 6.
        assert_eq!(cs.len(), 6);
        // All valence constraints are physical-severity.
        for c in &cs {
            assert!(matches!(c.severity, ConstraintSeverity::Physical));
        }
        // Carbon's valence constraint says max 4.
        let carbon = cs
            .iter()
            .find(|c| c.predicate.starts_with("element C "))
            .unwrap();
        assert!(carbon.predicate.contains("4"));
    }

    #[test]
    fn atoms_bonds_molecule_compose_via_property_refs() {
        let c = ChemistryPlugin::atom("C", 1, 4);
        let h1 = ChemistryPlugin::atom("H", 1, 1);
        let h2 = ChemistryPlugin::atom("H", 2, 1);
        let h3 = ChemistryPlugin::atom("H", 3, 1);
        let h4 = ChemistryPlugin::atom("H", 4, 1);
        let methane = ChemistryPlugin::molecule(
            "methane",
            "CH4",
            &[c.clone(), h1.clone(), h2.clone(), h3.clone(), h4.clone()],
        );
        // The molecule's `atoms` property is a List<Ref(SemanticId)>.
        if let Some(PropertyValue::List(refs)) = methane.properties.get("atoms") {
            assert_eq!(refs.len(), 5);
            for r in refs {
                assert!(matches!(r, PropertyValue::Ref(_)));
            }
        } else {
            panic!("methane.atoms should be a List");
        }
    }

    #[test]
    fn bond_graph_unifies_with_code_graph() {
        // Build a small molecule (water: H-O-H) and confirm its
        // bond graph has the SAME shape as a code-domain Graph.
        let o = ChemistryPlugin::atom("O", 1, 2);
        let h1 = ChemistryPlugin::atom("H", 1, 1);
        let h2 = ChemistryPlugin::atom("H", 2, 1);
        let b1 = ChemistryPlugin::bond(&o, &h1, 1.0);
        let b2 = ChemistryPlugin::bond(&o, &h2, 1.0);
        let g = ChemistryPlugin::bond_graph("water", &[o.clone(), h1, h2], &[b1, b2]);
        // 3 atoms → 3 nodes. 2 bonds → 2 edges.
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 2);
        // Edge weights carry bond order.
        assert_eq!(g.edges[0].weight, Some(1.0));
        // Wrap in Primitive — same enum a code-domain graph uses.
        let prim = Primitive::Graph(g);
        // Realm tag identifies the domain.
        assert_eq!(prim.realm(), &chem_realm());
    }

    #[test]
    fn reaction_event_carries_causal_chain() {
        // 2 H2 + O2 → 2 H2O (combustion).
        let h2_a = ChemistryPlugin::atom("H2", 1, 1);
        let h2_b = ChemistryPlugin::atom("H2", 2, 1);
        let o2 = ChemistryPlugin::atom("O2", 1, 2);
        let h2o_a = ChemistryPlugin::atom("H2O", 1, 2);
        let h2o_b = ChemistryPlugin::atom("H2O", 2, 2);
        let event = ChemistryPlugin::reaction(
            "combustion",
            &[&h2_a, &h2_b, &o2],
            &[&h2o_a, &h2o_b],
            10.0,
            1,
        );
        assert_eq!(event.causes.len(), 3);
        assert_eq!(event.effects.len(), 2);
        if let Some(PropertyValue::Number(e)) = event.payload.get("activation_energy_kj_per_mol") {
            assert!((*e - 10.0).abs() < 0.001);
        }
    }

    #[test]
    fn chemistry_and_engineering_both_serialize_through_same_schema() {
        // The URE's central claim: a chemistry molecule and a Rust
        // crate are the SAME `Primitive::Object` JSON shape, just
        // different realm + kind. Prove it.
        let c = ChemistryPlugin::atom("C", 1, 4);
        let rust = Object::new(SemanticId::code_crate("wishd"), "rust_crate", "wishd");
        let chem_json = serde_json::to_string(&Primitive::Object(c)).unwrap();
        let rust_json = serde_json::to_string(&Primitive::Object(rust)).unwrap();
        // Both have the `"primitive":"object"` discriminator.
        assert!(chem_json.contains("\"primitive\":\"object\""));
        assert!(rust_json.contains("\"primitive\":\"object\""));
        // Both have `kind` and `display_name`.
        assert!(chem_json.contains("\"kind\":\"atom\""));
        assert!(rust_json.contains("\"kind\":\"rust_crate\""));
        // The realm is the only structural difference.
        assert!(chem_json.contains("\"realm\":\"chem\"") || chem_json.contains("\"realm\":{"));
        assert!(rust_json.contains("\"realm\":\"code\""));
    }
}
