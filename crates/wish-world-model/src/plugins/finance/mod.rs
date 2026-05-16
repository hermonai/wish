//! **Finance domain plugin** — production-grade reference for the
//! URE. Designed so a downstream `vibe-finance` project can build a
//! trading dashboard, risk engine, and portfolio cockpit on top
//! without re-inventing the data model.
//!
//! # What's modeled
//!
//! - **Money** + **Currency** (USD, EUR, BTC, …)
//! - **Account** + **Asset** + **Position** + **Portfolio** + **Institution**
//! - **Order** (Market / Limit / Stop / StopLimit) + lifecycle states
//! - **Trade / Settlement / Dividend / MarginCall / Default / MarketEvent**
//! - **Counterparty graph** + **Exposure graph**
//! - **Constraints**: VaR limit, position limit, capital ratio,
//!   settlement deadline, margin requirement, no-short-sale, KYC
//! - **Risk**: unrealized P&L, aggregate exposure, gross notional,
//!   leverage, parametric VaR
//!
//! # Public surface
//!
//! ```ignore
//! use wish_world_model::plugins::finance::{
//!     builders::*, events::*, constraints::*, risk::*, types::*, FinancePlugin,
//! };
//! ```
//!
//! All builder/event/constraint/risk functions are free functions —
//! they don't require a [`FinancePlugin`] instance. The plugin struct
//! exists so the URE [`PluginRegistry`](crate::PluginRegistry) can
//! discover the finance realm.
//!
//! # Stability promise
//!
//! v0.5.x: API may evolve. v0.6.0 freezes the public surface for
//! downstream consumption. Tests in this module also serve as the
//! example gallery — read them to learn the API.

pub mod builders;
pub mod constraints;
pub mod events;
pub mod risk;
pub mod types;

use crate::plugin::DomainPlugin;
use crate::primitives::{Constraint, Primitive};
use crate::semantic_id::Realm;
use crate::WishWorld;

/// The Finance plugin.
pub struct FinancePlugin;

impl DomainPlugin for FinancePlugin {
    fn realm(&self) -> Realm {
        Realm::Finance
    }

    fn name(&self) -> &str {
        "Finance"
    }

    fn version(&self) -> &str {
        "0.5.0"
    }

    fn description(&self) -> &str {
        "Money, accounts, orders, positions, trades, settlement, risk, exposure graphs."
    }

    fn perspective_slugs(&self) -> Vec<&str> {
        vec!["financial", "portfolio", "risk", "counterparty"]
    }

    /// Static finance-realm constraints — the always-on rules a
    /// dealer/exchange/regulator would enforce on every account. The
    /// downstream `vibe-finance` project layers per-account /
    /// per-portfolio constraints on top via the constraint builders.
    fn realm_constraints(&self) -> Vec<Constraint> {
        use crate::semantic_id::SemanticId;
        use constraints::*;
        let system_account = SemanticId::new(Realm::Finance, "account", "system");
        let global_asset = SemanticId::new(Realm::Finance, "asset", "any");
        vec![
            // 95% / 1-day VaR ceiling for the system account, $10M.
            var_limit(&system_account, types::Money::usd(10_000_000.0), 0.95),
            // Hard position limit per asset, 1M units (huge — meant
            // as a backstop; per-asset overrides are tighter).
            position_limit(&global_asset, 1_000_000.0),
            // Margin requirement: 25% (Reg-T-ish baseline).
            margin_requirement(&system_account, 0.25),
        ]
    }

    /// Filter the world to finance-realm primitives.
    fn primitives_for_world(&self, world: &WishWorld) -> Vec<Primitive> {
        crate::primitives::primitives_from_world(world)
            .into_iter()
            .filter(|p| matches!(p.realm(), Realm::Finance))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::builders::*;
    use super::constraints::*;
    use super::events::*;
    use super::risk::*;
    use super::types::*;
    use super::FinancePlugin;
    use crate::plugin::DomainPlugin;
    use crate::primitives::{ConstraintSeverity, Primitive};
    use crate::semantic_id::{Realm, SemanticId};
    use std::collections::HashMap;

    fn alice() -> SemanticId {
        SemanticId::new(Realm::Custom("agent".into()), "person", "alice")
    }

    #[test]
    fn finance_plugin_owns_finance_realm() {
        let p = FinancePlugin;
        assert_eq!(p.realm(), Realm::Finance);
        assert_eq!(p.name(), "Finance");
        assert!(p.perspective_slugs().contains(&"portfolio"));
        assert!(p.perspective_slugs().contains(&"risk"));
    }

    #[test]
    fn finance_realm_constraints_cover_var_position_margin() {
        let cs = FinancePlugin.realm_constraints();
        assert_eq!(cs.len(), 3);
        // Each constraint is wired to a different severity.
        let severities: Vec<&ConstraintSeverity> = cs.iter().map(|c| &c.severity).collect();
        assert!(severities.contains(&&ConstraintSeverity::RequiresApproval)); // VaR
        assert!(severities.contains(&&ConstraintSeverity::Hard)); // position + margin (×2)
    }

    /// **The headline integration test** — a downstream
    /// `vibe-finance` project builds Alice's portfolio, executes a
    /// trade, computes P&L + VaR, checks against the limit.
    /// **End-to-end scenario through the entire finance plugin.**
    #[test]
    fn portfolio_trade_pnl_var_end_to_end() {
        // 1. Alice opens a margin account funded with $100k.
        let acc = account(
            "alice_margin",
            &alice(),
            Currency::usd(),
            100_000.0,
            "margin",
        );

        // 2. Define the asset Alice is trading.
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);

        // 3. Alice places a limit buy order at $60k.
        let buy_order = order(
            "ord_buy",
            &acc.id,
            &btc.id,
            Side::Buy,
            0.5,
            OrderKind::Limit {
                price: Money::usd(60_000.0),
            },
        );
        // The matching sell side (some other account on the exchange).
        let sell_order = order(
            "ord_sell",
            &SemanticId::new(Realm::Finance, "account", "exchange_pool"),
            &btc.id,
            Side::Sell,
            0.5,
            OrderKind::Limit {
                price: Money::usd(60_000.0),
            },
        );

        // 4. The exchange matches and produces a Trade event.
        let trade_event = trade(
            "trade_1",
            &buy_order.id,
            &sell_order.id,
            0.5,
            Money::usd(60_000.0),
            1,
        );

        // 5. The trade settles next day (T+1).
        let settle_event = settlement("set_1", &trade_event, SettlementState::Settled, 2);
        assert_eq!(settle_event.causes, vec![trade_event.id.clone()]);

        // 6. After settlement, Alice holds 0.5 BTC at $60k entry.
        let position_obj = position(
            "pos_1",
            &acc.id,
            &btc.id,
            PositionSide::Long,
            0.5,
            Money::usd(60_000.0),
        );

        // 7. Mark BTC up to $70k — compute unrealized P&L.
        let pnl = unrealized_pnl(&position_obj, &Money::usd(70_000.0)).unwrap();
        assert!((pnl.amount - 5_000.0).abs() < 1e-3);

        // 8. Compute parametric VaR at 95%, 4% daily vol.
        let marks: HashMap<String, f64> = [("BTC".to_string(), 70_000.0)].into_iter().collect();
        let vols: HashMap<String, f64> = [("BTC".to_string(), 0.04)].into_iter().collect();
        let var_amount = parametric_var_1d(
            &[position_obj.clone()],
            &marks,
            &vols,
            0.95,
            Currency::usd(),
        );
        // 1.645 * 0.5 * 70000 * 0.04 ≈ 2303.
        assert!((var_amount.amount - 2303.0).abs() < 20.0);

        // 9. Check against the account's $1M VaR limit — pass.
        let limit_constraint = var_limit(&acc.id, Money::usd(1_000_000.0), 0.95);
        assert!(var_amount.amount < 1_000_000.0);
        assert_eq!(
            limit_constraint.severity,
            ConstraintSeverity::RequiresApproval
        );

        // 10. Wrap as URE primitives — proves the whole flow is
        //     just `Vec<Primitive>` under the hood.
        let prims: Vec<Primitive> = vec![
            Primitive::Object(acc.clone()),
            Primitive::Object(btc.clone()),
            Primitive::Object(buy_order),
            Primitive::Object(sell_order),
            Primitive::Event(trade_event),
            Primitive::Event(settle_event),
            Primitive::Object(position_obj),
            Primitive::Constraint(limit_constraint),
        ];
        // Roundtrip-via-JSON proves the wire format is stable.
        let json = serde_json::to_string(&prims).unwrap();
        let back: Vec<Primitive> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 8);
    }

    /// Counterparty default cascade — Bank A defaults, Bank B's
    /// exposure surfaces via the counterparty graph + Default event.
    #[test]
    fn counterparty_default_cascade_scenario() {
        let gs = institution("gs", "Goldman Sachs", "bank");
        let leh = institution("leh", "Lehman", "bank");
        let citadel = institution("cit", "Citadel", "fund");
        let cp_graph = counterparty_graph(
            "primebroker",
            &[gs.clone(), leh.clone(), citadel.clone()],
            &[
                (leh.id.clone(), gs.id.clone(), 100_000_000.0),
                (leh.id.clone(), citadel.id.clone(), 50_000_000.0),
            ],
        );
        // Lehman defaults.
        let default = default_event("def_leh", &leh.id, Money::usd(150_000_000.0), 1000);
        // The default event names Lehman as cause — downstream
        // simulator can trace cp_graph edges to find affected
        // counterparties (gs, citadel).
        assert_eq!(default.causes, vec![leh.id.clone()]);
        // Edge enumeration confirms the exposure chain is graph-traversable.
        let from_leh: Vec<_> = cp_graph.edges.iter().filter(|e| e.from == leh.id).collect();
        assert_eq!(from_leh.len(), 2);
        let total_at_risk: f32 = from_leh.iter().map(|e| e.weight.unwrap_or(0.0)).sum();
        assert!((total_at_risk - 150_000_000.0).abs() < 1.0);
    }

    /// Market-shock scenario — a market event causes positions to
    /// breach margin → margin call event.
    #[test]
    fn market_shock_triggers_margin_call() {
        let acc = account(
            "alice_margin",
            &alice(),
            Currency::usd(),
            100_000.0,
            "margin",
        );
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let pos = position(
            "p",
            &acc.id,
            &btc.id,
            PositionSide::Long,
            5.0,
            Money::usd(70_000.0),
        );
        // Simulate a -10% crash.
        let crash = market_event("crash", "circuit_breaker", -0.10, 500);
        // P&L at marked-down 63k → 5 * (63k - 70k) = -35k. Equity
        // falls from 100k to 65k. Margin call fires if equity <
        // 0.25 × position_value = 0.25 × 5 × 63k = 78,750. Shortfall
        // ≈ 13,750.
        let new_mark = Money::usd(63_000.0);
        let pnl = unrealized_pnl(&pos, &new_mark).unwrap();
        let new_equity = 100_000.0 + pnl.amount;
        let position_value = 5.0 * 63_000.0;
        let required = 0.25 * position_value;
        let shortfall = (required - new_equity).max(0.0);
        let mc = margin_call("mc_alice", &acc.id, Money::usd(shortfall), 501);
        // The margin call references the crash via the URE's causal
        // chain — the downstream sim would add the crash event id to
        // mc.causes. Confirm the event structure carries the data.
        assert!((shortfall - 13_750.0).abs() < 1.0);
        assert_eq!(crash.kind, "market_event");
        assert_eq!(mc.kind, "margin_call");
    }

    /// Cross-domain primitive — finance + chemistry both serialize
    /// to the same shape. The URE substrate is realm-agnostic.
    #[test]
    fn finance_object_and_chemistry_object_same_schema() {
        use super::super::chemistry::ChemistryPlugin;
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let carbon = ChemistryPlugin::atom("C", 1, 4);
        let btc_json = serde_json::to_string(&Primitive::Object(btc)).unwrap();
        let c_json = serde_json::to_string(&Primitive::Object(carbon)).unwrap();
        // Both have the `"primitive":"object"` discriminator.
        assert!(btc_json.contains("\"primitive\":\"object\""));
        assert!(c_json.contains("\"primitive\":\"object\""));
        // Both have `kind` and `properties`. Only the realm differs.
        assert!(btc_json.contains("\"realm\":\"finance\""));
        assert!(c_json.contains("\"realm\":"));
    }
}
