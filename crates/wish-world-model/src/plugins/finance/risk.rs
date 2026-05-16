//! Finance plugin — risk computation.
//!
//! Pure functions over URE primitives. Inputs come from the
//! universal substrate (`Object`s with finance-realm properties);
//! outputs are typed risk numbers + diagnostics. The downstream
//! `vibe-finance` project can swap in market-data feeds + run these
//! at production scale.
//!
//! v0.5.0 ships:
//! - **P&L** (realized and unrealized)
//! - **Exposure aggregation** per asset
//! - **Leverage** (gross exposure / equity)
//! - **Parametric VaR** (assuming normal returns)
//!
//! Future waves: historical-simulation VaR, stress scenarios,
//! Greeks for options, multi-currency FX risk.

use super::types::{Money, PositionSide};
use crate::primitives::{Object, PropertyValue};
use std::collections::HashMap;

/// **Unrealized P&L** for a single position at the given mark price.
/// Returns `None` if the position's `entry_price` currency differs
/// from `mark_price.currency` (caller must FX-convert).
///
/// ```ignore
/// pnl = side_sign * size * (mark - entry)
/// ```
pub fn unrealized_pnl(position: &Object, mark_price: &Money) -> Option<Money> {
    let size = number_prop(position, "size")?;
    let entry = money_prop(position, "entry_price")?;
    if entry.currency != mark_price.currency {
        return None;
    }
    let side_sign = match text_prop(position, "side").as_deref() {
        Some("short") => PositionSide::Short.sign(),
        _ => PositionSide::Long.sign(),
    };
    let pnl_amount = side_sign * size * (mark_price.amount - entry.amount);
    Some(Money::new(pnl_amount, mark_price.currency.clone()))
}

/// **Aggregate signed exposure per asset** across many positions.
/// Long positions contribute `+size`, short positions contribute
/// `-size`. Returns a map keyed by the asset's stable_key.
pub fn aggregate_exposure(positions: &[Object]) -> HashMap<String, f64> {
    let mut out: HashMap<String, f64> = HashMap::new();
    for p in positions {
        let Some(asset) = ref_prop(p, "asset") else {
            continue;
        };
        let Some(size) = number_prop(p, "size") else {
            continue;
        };
        let sign = match text_prop(p, "side").as_deref() {
            Some("short") => -1.0,
            _ => 1.0,
        };
        *out.entry(asset.stable_key.clone()).or_insert(0.0) += sign * size;
    }
    out
}

/// **Gross notional exposure** at the given mark prices.
/// `mark_prices` is keyed by asset stable_key. Sums |size × mark|
/// across every position. Same-currency assumed (caller normalizes).
pub fn gross_notional(positions: &[Object], mark_prices: &HashMap<String, f64>) -> f64 {
    let mut total = 0.0;
    for p in positions {
        let Some(asset) = ref_prop(p, "asset") else {
            continue;
        };
        let Some(size) = number_prop(p, "size") else {
            continue;
        };
        let Some(mark) = mark_prices.get(&asset.stable_key) else {
            continue;
        };
        total += (size * mark).abs();
    }
    total
}

/// **Leverage** = gross notional / equity. Equity is the account's
/// balance Money amount. Returns `None` if equity is non-positive.
pub fn leverage(positions: &[Object], mark_prices: &HashMap<String, f64>, equity: f64) -> Option<f64> {
    if equity <= 0.0 {
        return None;
    }
    Some(gross_notional(positions, mark_prices) / equity)
}

/// **Parametric VaR** — Value-at-Risk under a normal-distribution
/// assumption. `daily_vol` is the per-asset daily volatility (decimal,
/// e.g. 0.02 = 2%). `confidence` typically 0.95 or 0.99.
/// Returns Money in the position's currency.
///
/// Single-asset formula (no correlation across assets here — Wave 28
/// adds a covariance matrix); for a portfolio of independent
/// positions we sum-of-squares the per-position VaR. **Conservative
/// approximation** good enough for risk-limit checks; production
/// risk engines use Cornish-Fisher or historical simulation.
pub fn parametric_var_1d(
    positions: &[Object],
    mark_prices: &HashMap<String, f64>,
    daily_vols: &HashMap<String, f64>,
    confidence: f64,
    currency: super::types::Currency,
) -> Money {
    // Z-score lookup: 1.645 for 95%, 2.326 for 99%, etc.
    let z = z_score(confidence);
    let mut sum_sq = 0.0;
    for p in positions {
        let Some(asset) = ref_prop(p, "asset") else {
            continue;
        };
        let Some(size) = number_prop(p, "size") else {
            continue;
        };
        let Some(mark) = mark_prices.get(&asset.stable_key) else {
            continue;
        };
        let Some(vol) = daily_vols.get(&asset.stable_key) else {
            continue;
        };
        let position_var = (size * mark * vol).abs();
        sum_sq += position_var * position_var;
    }
    let portfolio_sigma = sum_sq.sqrt();
    Money::new(z * portfolio_sigma, currency)
}

/// Inverse-normal lookup for common VaR confidence levels.
/// Polynomial approx good to ~4 decimal places for 0.9 ≤ p ≤ 0.995.
fn z_score(p: f64) -> f64 {
    match p {
        x if (x - 0.99).abs() < 1e-6 => 2.326,
        x if (x - 0.95).abs() < 1e-6 => 1.645,
        x if (x - 0.975).abs() < 1e-6 => 1.960,
        x if (x - 0.90).abs() < 1e-6 => 1.282,
        // Beasley-Springer-Moro approximation for arbitrary p.
        _ => beasley_springer_moro(p),
    }
}

/// Approximate inverse normal CDF. Accurate enough for risk-limit
/// dashboards; do not use for option-pricing exact quantiles.
fn beasley_springer_moro(p: f64) -> f64 {
    let p = p.clamp(1e-6, 1.0 - 1e-6);
    let a = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    let b = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    let t = p - 0.5;
    if t.abs() <= 0.42 {
        let r = t * t;
        t * (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5])
            / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0)
    } else {
        // Tail approximation.
        let r = if p < 0.5 { p } else { 1.0 - p };
        let r = (-((1.0 - r).ln())).sqrt();
        let z = r
            - (2.515_517 + 0.802_853 * r + 0.010_328 * r * r)
                / (1.0 + 1.432_788 * r + 0.189_269 * r * r + 0.001_308 * r * r * r);
        if p < 0.5 {
            -z
        } else {
            z
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Helpers — pull typed properties off a Primitive::Object cleanly
// ─────────────────────────────────────────────────────────────────────

fn number_prop(o: &Object, key: &str) -> Option<f64> {
    match o.properties.get(key)? {
        PropertyValue::Number(n) => Some(*n),
        _ => None,
    }
}

fn text_prop(o: &Object, key: &str) -> Option<String> {
    match o.properties.get(key)? {
        PropertyValue::Text(s) => Some(s.clone()),
        _ => None,
    }
}

fn ref_prop(o: &Object, key: &str) -> Option<crate::semantic_id::SemanticId> {
    match o.properties.get(key)? {
        PropertyValue::Ref(id) => Some((**id).clone()),
        _ => None,
    }
}

fn money_prop(o: &Object, key: &str) -> Option<Money> {
    let PropertyValue::Map(m) = o.properties.get(key)? else {
        return None;
    };
    let amount = match m.get("amount")? {
        PropertyValue::Number(n) => *n,
        _ => return None,
    };
    let currency = match m.get("currency")? {
        PropertyValue::Text(s) => s.clone(),
        _ => return None,
    };
    Some(Money::new(amount, super::types::Currency::new(currency)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::builders::{account, asset, position};
    use super::super::types::{AssetClass, Currency, PositionSide};
    use crate::semantic_id::{Realm, SemanticId};

    fn alice() -> SemanticId {
        SemanticId::new(Realm::Custom("agent".into()), "person", "alice")
    }

    #[test]
    fn unrealized_pnl_long_profitable() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let p = position("p1", &acc.id, &btc.id, PositionSide::Long, 0.5, Money::usd(60_000.0));
        // Mark at 70k → P&L = 0.5 * (70k - 60k) = +5,000.
        let pnl = unrealized_pnl(&p, &Money::usd(70_000.0)).unwrap();
        assert!((pnl.amount - 5_000.0).abs() < 1e-5);
    }

    #[test]
    fn unrealized_pnl_short_profitable_when_price_falls() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let p = position("p1", &acc.id, &btc.id, PositionSide::Short, 0.5, Money::usd(70_000.0));
        // Mark at 60k → short profit = -1 * 0.5 * (60k - 70k) = +5,000.
        let pnl = unrealized_pnl(&p, &Money::usd(60_000.0)).unwrap();
        assert!((pnl.amount - 5_000.0).abs() < 1e-5);
    }

    #[test]
    fn unrealized_pnl_cross_currency_returns_none() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let p = position("p1", &acc.id, &btc.id, PositionSide::Long, 0.5, Money::usd(60_000.0));
        // Mark is in EUR → no FX conversion → return None.
        assert!(unrealized_pnl(&p, &Money::eur(55_000.0)).is_none());
    }

    #[test]
    fn aggregate_exposure_nets_long_and_short() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let long = position("l", &acc.id, &btc.id, PositionSide::Long, 0.7, Money::usd(65_000.0));
        let short = position("s", &acc.id, &btc.id, PositionSide::Short, 0.3, Money::usd(65_000.0));
        let agg = aggregate_exposure(&[long, short]);
        let net = agg.get("BTC").copied().unwrap_or(0.0);
        assert!((net - 0.4).abs() < 1e-6);
    }

    #[test]
    fn leverage_returns_none_for_zero_equity() {
        assert!(leverage(&[], &HashMap::new(), 0.0).is_none());
        assert!(leverage(&[], &HashMap::new(), -100.0).is_none());
    }

    #[test]
    fn parametric_var_scales_with_size_and_vol() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let p = position("p1", &acc.id, &btc.id, PositionSide::Long, 1.0, Money::usd(60_000.0));
        let marks: HashMap<String, f64> = [("BTC".to_string(), 65_000.0)].into_iter().collect();
        let vols: HashMap<String, f64> = [("BTC".to_string(), 0.04)].into_iter().collect();
        // VaR(95%) = 1.645 * 1 * 65000 * 0.04 = 4_277.
        let var = parametric_var_1d(&[p], &marks, &vols, 0.95, Currency::usd());
        assert!((var.amount - 4_277.0).abs() < 10.0);
    }

    #[test]
    fn z_score_known_quantiles() {
        assert!((z_score(0.95) - 1.645).abs() < 0.001);
        assert!((z_score(0.99) - 2.326).abs() < 0.001);
        assert!((z_score(0.975) - 1.960).abs() < 0.001);
    }
}
