//! Finance plugin — value types.
//!
//! Carefully chosen primitives so a downstream `vibe-finance`
//! project can build trading models, risk engines, and portfolio
//! views without re-inventing the data model.

use serde::{Deserialize, Serialize};

/// **Currency** — ISO 4217 alphabetic code (`"USD"`, `"EUR"`,
/// `"JPY"`, `"BTC"`, `"ETH"`). Stored as a 3–6 char string so
/// crypto tickers fit naturally.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Currency(pub String);

impl Currency {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into().to_uppercase())
    }
    pub fn usd() -> Self {
        Self("USD".to_string())
    }
    pub fn eur() -> Self {
        Self("EUR".to_string())
    }
    pub fn btc() -> Self {
        Self("BTC".to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// **Money** — amount + currency. Stored as f64 for v0.5.0; future
/// waves replace with a fixed-point `Decimal` for exact accounting.
///
/// **Caveat**: f64 is sufficient for risk modeling and what-if
/// scenarios but NOT for production settlement (rounding errors
/// accumulate). The downstream `vibe-finance` project should swap to
/// `rust_decimal::Decimal` when it needs exact ledgers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Money {
    pub amount: f64,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount: f64, currency: Currency) -> Self {
        Self { amount, currency }
    }
    pub fn usd(amount: f64) -> Self {
        Self::new(amount, Currency::usd())
    }
    pub fn eur(amount: f64) -> Self {
        Self::new(amount, Currency::eur())
    }
    pub fn btc(amount: f64) -> Self {
        Self::new(amount, Currency::btc())
    }
    pub fn zero(currency: Currency) -> Self {
        Self::new(0.0, currency)
    }
    pub fn is_zero(&self) -> bool {
        self.amount.abs() < 1e-9
    }

    /// Same-currency addition. Returns `None` if currencies differ —
    /// the caller must FX-convert first. This deliberate friction
    /// keeps multi-currency bugs visible at the type level.
    pub fn checked_add(&self, other: &Money) -> Option<Money> {
        if self.currency == other.currency {
            Some(Money::new(
                self.amount + other.amount,
                self.currency.clone(),
            ))
        } else {
            None
        }
    }

    pub fn checked_sub(&self, other: &Money) -> Option<Money> {
        if self.currency == other.currency {
            Some(Money::new(
                self.amount - other.amount,
                self.currency.clone(),
            ))
        } else {
            None
        }
    }

    /// Scale by a unitless multiplier (e.g. position size × price).
    pub fn scale(&self, k: f64) -> Money {
        Money::new(self.amount * k, self.currency.clone())
    }
}

/// **Side** — buy or sell. Used by orders, positions, trades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    /// Signed multiplier — `Buy = +1`, `Sell = -1`. Used in P&L math.
    pub fn sign(self) -> f64 {
        match self {
            Side::Buy => 1.0,
            Side::Sell => -1.0,
        }
    }

    pub fn flip(self) -> Side {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

/// **OrderKind** — execution model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderKind {
    /// Execute immediately at best available price.
    Market,
    /// Execute only at `price` or better.
    Limit { price: Money },
    /// Become a market order when last-trade hits `trigger`.
    Stop { trigger: Money },
    /// Become a limit order at `price` when last-trade hits `trigger`.
    StopLimit { trigger: Money, price: Money },
}

/// **OrderStatus** — lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    #[default]
    Pending,
    PartiallyFilled,
    Filled,
    Canceled,
    Rejected,
    Expired,
}

/// **AssetClass** — broad bucket. Plugins can extend via `Custom`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Equity,
    Bond,
    Cash,
    Commodity,
    Crypto,
    Derivative,
    RealEstate,
    Custom(String),
}

/// **PositionSide** — long or short. Distinct from `Side` because a
/// short position is *opened by selling* but the position itself is
/// "short" not "sell".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionSide {
    Long,
    Short,
}

impl PositionSide {
    pub fn sign(self) -> f64 {
        match self {
            PositionSide::Long => 1.0,
            PositionSide::Short => -1.0,
        }
    }
}

/// **SettlementState** — where a trade sits in the settlement
/// pipeline. T+0 (instant), T+1, T+2 (US equities) all flow through
/// these states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SettlementState {
    #[default]
    Pending,
    Cleared,
    Settled,
    Failed,
    Reversed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_normalizes_to_uppercase() {
        assert_eq!(Currency::new("usd").as_str(), "USD");
        assert_eq!(Currency::new("Eur").as_str(), "EUR");
    }

    #[test]
    fn money_same_currency_add_sub() {
        let a = Money::usd(100.0);
        let b = Money::usd(50.0);
        assert_eq!(a.checked_add(&b).unwrap().amount, 150.0);
        assert_eq!(a.checked_sub(&b).unwrap().amount, 50.0);
    }

    #[test]
    fn money_cross_currency_returns_none() {
        let a = Money::usd(100.0);
        let b = Money::eur(50.0);
        assert!(a.checked_add(&b).is_none());
        assert!(a.checked_sub(&b).is_none());
    }

    #[test]
    fn money_scale_multiplies_amount_preserves_currency() {
        let m = Money::usd(100.0).scale(2.5);
        assert_eq!(m.amount, 250.0);
        assert_eq!(m.currency, Currency::usd());
    }

    #[test]
    fn side_signs_compose() {
        assert_eq!(Side::Buy.sign(), 1.0);
        assert_eq!(Side::Sell.sign(), -1.0);
        assert_eq!(Side::Buy.flip(), Side::Sell);
    }

    #[test]
    fn position_side_signs_compose() {
        assert_eq!(PositionSide::Long.sign(), 1.0);
        assert_eq!(PositionSide::Short.sign(), -1.0);
    }

    #[test]
    fn order_kind_serializes_with_kind_tag() {
        let limit = OrderKind::Limit {
            price: Money::usd(100.0),
        };
        let json = serde_json::to_string(&limit).unwrap();
        // Internally tagged variant.
        assert!(json.contains("limit"));
        assert!(json.contains("price"));
    }
}
