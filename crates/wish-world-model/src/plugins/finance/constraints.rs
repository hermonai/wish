//! Finance plugin — constraint constructors.
//!
//! Each constructor returns a `Constraint` ready to register with
//! the URE. The downstream `vibe-finance` project consumes these
//! to enforce risk + regulatory rules before allowing patches to
//! apply.

use super::types::Money;
use crate::primitives::{Constraint, ConstraintSeverity, PropertyValue};
use crate::semantic_id::{Realm, SemanticId};
use std::collections::HashMap;

fn finance_id(kind: &str, key: impl Into<String>) -> SemanticId {
    SemanticId::new(Realm::Finance, kind, key)
}

fn money_expression(key: &str, m: &Money) -> (String, PropertyValue) {
    let mut map = HashMap::new();
    map.insert("amount".to_string(), PropertyValue::Number(m.amount));
    map.insert(
        "currency".to_string(),
        PropertyValue::Text(m.currency.as_str().to_string()),
    );
    (key.to_string(), PropertyValue::Map(map))
}

/// **VaR limit** — Value-at-Risk ceiling on an account or
/// portfolio. Severity = `RequiresApproval` because breaching it
/// should pause for human review, not be impossible.
pub fn var_limit(account: &SemanticId, limit: Money, confidence: f64) -> Constraint {
    let mut expr = HashMap::new();
    let (k, v) = money_expression("limit", &limit);
    expr.insert(k, v);
    expr.insert("confidence".to_string(), PropertyValue::Number(confidence));
    Constraint {
        id: finance_id("constraint", format!("var-{}", account.stable_key)),
        kind: "var_limit".to_string(),
        severity: ConstraintSeverity::RequiresApproval,
        predicate: format!(
            "VaR({:.0}%) ≤ {} {}",
            confidence * 100.0,
            limit.amount,
            limit.currency.as_str()
        ),
        applies_to: vec![account.clone()],
        expression: expr,
    }
}

/// **Position limit** — maximum size of a single position. Hard
/// constraint (exchanges and prime brokers enforce these directly).
pub fn position_limit(asset: &SemanticId, max_size: f64) -> Constraint {
    let mut expr = HashMap::new();
    expr.insert("max_size".to_string(), PropertyValue::Number(max_size));
    Constraint {
        id: finance_id("constraint", format!("poslim-{}", asset.stable_key)),
        kind: "position_limit".to_string(),
        severity: ConstraintSeverity::Hard,
        predicate: format!("position size ≤ {max_size}"),
        applies_to: vec![asset.clone()],
        expression: expr,
    }
}

/// **Capital ratio** — minimum capital / risk-weighted assets
/// (Basel III). Regulatory severity.
pub fn capital_ratio(institution: &SemanticId, min_ratio: f64) -> Constraint {
    let mut expr = HashMap::new();
    expr.insert("min_ratio".to_string(), PropertyValue::Number(min_ratio));
    Constraint {
        id: finance_id("constraint", format!("capratio-{}", institution.stable_key)),
        kind: "capital_ratio".to_string(),
        severity: ConstraintSeverity::Regulatory,
        predicate: format!("CET1 / RWA ≥ {:.2}%", min_ratio * 100.0),
        applies_to: vec![institution.clone()],
        expression: expr,
    }
}

/// **Settlement deadline** — a trade must settle within `t_plus`
/// days. Probabilistic severity (failure to settle has a probability;
/// repeated failures escalate).
pub fn settlement_deadline(trade: &SemanticId, t_plus_days: u32) -> Constraint {
    let mut expr = HashMap::new();
    expr.insert(
        "t_plus_days".to_string(),
        PropertyValue::Number(t_plus_days as f64),
    );
    Constraint {
        id: finance_id("constraint", format!("settle-{}", trade.stable_key)),
        kind: "settlement_deadline".to_string(),
        severity: ConstraintSeverity::Probabilistic,
        predicate: format!("trade settles within T+{t_plus_days}"),
        applies_to: vec![trade.clone()],
        expression: expr,
    }
}

/// **Margin requirement** — account must maintain `min_margin` ratio
/// (equity / position-value) to avoid margin call.
pub fn margin_requirement(account: &SemanticId, min_margin_pct: f64) -> Constraint {
    let mut expr = HashMap::new();
    expr.insert(
        "min_margin_pct".to_string(),
        PropertyValue::Number(min_margin_pct),
    );
    Constraint {
        id: finance_id("constraint", format!("margin-{}", account.stable_key)),
        kind: "margin_requirement".to_string(),
        severity: ConstraintSeverity::Hard,
        predicate: format!("equity / position_value ≥ {:.2}%", min_margin_pct * 100.0),
        applies_to: vec![account.clone()],
        expression: expr,
    }
}

/// **No-short-sale rule** — a regulatory ban on shorting a specific
/// asset (e.g. during a market crisis). Regulatory severity.
pub fn no_short_sale(asset: &SemanticId, reason: &str) -> Constraint {
    let mut expr = HashMap::new();
    expr.insert(
        "reason".to_string(),
        PropertyValue::Text(reason.to_string()),
    );
    Constraint {
        id: finance_id("constraint", format!("noshort-{}", asset.stable_key)),
        kind: "no_short_sale".to_string(),
        severity: ConstraintSeverity::Regulatory,
        predicate: format!("short sales of {} prohibited: {reason}", asset.stable_key),
        applies_to: vec![asset.clone()],
        expression: expr,
    }
}

/// **KYC required** — agent must complete identity verification
/// before opening positions. Ethical severity (because lapses harm
/// the wider system, not just the account).
pub fn kyc_required(agent: &SemanticId) -> Constraint {
    Constraint {
        id: finance_id("constraint", format!("kyc-{}", agent.stable_key)),
        kind: "kyc_required".to_string(),
        severity: ConstraintSeverity::Ethical,
        predicate: "KYC verification required before trading".to_string(),
        applies_to: vec![agent.clone()],
        expression: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc() -> SemanticId {
        finance_id("account", "alice")
    }

    #[test]
    fn var_limit_records_money_and_confidence() {
        let c = var_limit(&acc(), Money::usd(1_000_000.0), 0.99);
        assert_eq!(c.severity, ConstraintSeverity::RequiresApproval);
        assert!(c.predicate.contains("99%"));
        assert!(c.predicate.contains("1000000"));
    }

    #[test]
    fn position_limit_is_hard_severity() {
        let aapl = finance_id("asset", "AAPL");
        let c = position_limit(&aapl, 10_000.0);
        assert_eq!(c.severity, ConstraintSeverity::Hard);
    }

    #[test]
    fn capital_ratio_is_regulatory() {
        let gs = finance_id("institution", "gs");
        let c = capital_ratio(&gs, 0.105);
        assert_eq!(c.severity, ConstraintSeverity::Regulatory);
        assert!(c.predicate.contains("10.50%") || c.predicate.contains("10.5"));
    }

    #[test]
    fn no_short_sale_records_reason() {
        let gme = finance_id("asset", "GME");
        let c = no_short_sale(&gme, "regulatory_emergency");
        if let Some(PropertyValue::Text(r)) = c.expression.get("reason") {
            assert_eq!(r, "regulatory_emergency");
        }
    }

    #[test]
    fn settlement_deadline_carries_t_plus() {
        let t = finance_id("trade", "t1");
        let c = settlement_deadline(&t, 2);
        if let Some(PropertyValue::Number(n)) = c.expression.get("t_plus_days") {
            assert_eq!(*n as u32, 2);
        }
    }
}
