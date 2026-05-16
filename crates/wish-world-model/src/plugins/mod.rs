//! **Reference domain plugins** for the URE.
//!
//! These plugins ship with `wish-world-model` as proof that the
//! [`DomainPlugin`](crate::plugin::DomainPlugin) trait can carry any
//! structured reality. The plugins are deliberately diverse —
//! Engineering (existing code-domain wrapped) and Chemistry (a
//! completely different domain) — to demonstrate the substrate's
//! universality.
//!
//! New domains (Legal, Medical, Music, Civic, Climate, …) add
//! themselves by creating a new module here or in an external crate
//! that depends on `wish-world-model`.

pub mod chemistry;
pub mod engineering;
pub mod finance;

pub use chemistry::ChemistryPlugin;
pub use engineering::EngineeringPlugin;
pub use finance::FinancePlugin;
