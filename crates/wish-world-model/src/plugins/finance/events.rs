//! Finance plugin — event constructors.
//!
//! Trades, settlements, margin calls, defaults — every financial
//! event flows through these constructors so the URE's
//! [`Event::causes`] / [`Event::effects`] causal-chain semantics
//! work correctly out of the box.

use super::types::{Money, SettlementState};
use crate::primitives::{Event, PropertyValue};
use crate::semantic_id::{Realm, SemanticId};
use std::collections::HashMap;

fn finance_id(kind: &str, key: impl Into<String>) -> SemanticId {
    SemanticId::new(Realm::Finance, kind, key)
}

fn money_property(m: &Money) -> PropertyValue {
    let mut map = HashMap::new();
    map.insert("amount".to_string(), PropertyValue::Number(m.amount));
    map.insert(
        "currency".to_string(),
        PropertyValue::Text(m.currency.as_str().to_string()),
    );
    PropertyValue::Map(map)
}

/// **Trade event** — two orders matched. The causal-graph rule:
/// `causes = [buy_order_id, sell_order_id]`, `effects = []` (the
/// effects — position changes, cash transfers — are produced by
/// downstream events).
pub fn trade(
    id: &str,
    buy_order: &SemanticId,
    sell_order: &SemanticId,
    fill_size: f64,
    fill_price: Money,
    at_step: u64,
) -> Event {
    let mut e = Event::new(finance_id("trade", id), "trade", at_step);
    e.causes.push(buy_order.clone());
    e.causes.push(sell_order.clone());
    e.payload
        .insert("fill_size".to_string(), PropertyValue::Number(fill_size));
    e.payload
        .insert("fill_price".to_string(), money_property(&fill_price));
    let notional = fill_size * fill_price.amount;
    e.payload.insert(
        "notional".to_string(),
        money_property(&Money::new(notional, fill_price.currency.clone())),
    );
    e
}

/// **Settlement event** — a previous trade clears + settles. Causes
/// = `[trade_id]`. Updates the trade's `SettlementState`.
pub fn settlement(id: &str, trade: &Event, new_state: SettlementState, at_step: u64) -> Event {
    let mut e = Event::new(finance_id("settlement", id), "settlement", at_step);
    e.causes.push(trade.id.clone());
    e.payload.insert(
        "state".to_string(),
        PropertyValue::Text(
            serde_json::to_value(&new_state)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default(),
        ),
    );
    e
}

/// **Dividend event** — an asset pays a dividend to position
/// holders. `causes = [asset_id]`, `effects = [holder_account_ids…]`.
pub fn dividend(
    id: &str,
    asset: &SemanticId,
    holders: &[SemanticId],
    amount_per_share: Money,
    at_step: u64,
) -> Event {
    let mut e = Event::new(finance_id("dividend", id), "dividend", at_step);
    e.causes.push(asset.clone());
    for h in holders {
        e.effects.push(h.clone());
    }
    e.payload.insert(
        "amount_per_share".to_string(),
        money_property(&amount_per_share),
    );
    e
}

/// **Margin call** — an account's margin has fallen below the
/// maintenance threshold. `causes = [account_id]`, payload carries
/// the shortfall amount.
pub fn margin_call(id: &str, account: &SemanticId, shortfall: Money, at_step: u64) -> Event {
    let mut e = Event::new(finance_id("margin_call", id), "margin_call", at_step);
    e.causes.push(account.clone());
    e.payload
        .insert("shortfall".to_string(), money_property(&shortfall));
    e.payload.insert(
        "severity".to_string(),
        PropertyValue::Text("high".to_string()),
    );
    e
}

/// **Default event** — an account / counterparty has failed to
/// meet obligations. `causes = [account_id]`. Triggers cascade
/// modeling: the URE simulator can fan-out to the counterparty graph
/// to model contagion.
pub fn default_event(id: &str, account: &SemanticId, amount_owed: Money, at_step: u64) -> Event {
    let mut e = Event::new(finance_id("default", id), "default", at_step);
    e.causes.push(account.clone());
    e.payload
        .insert("amount_owed".to_string(), money_property(&amount_owed));
    e
}

/// **Market event** — a price shock, halt, circuit breaker, news.
/// `causes` and `effects` are usually empty for the initiator (it
/// caused itself); downstream trades/positions track it via Events
/// listing the MarketEvent's id in their causes.
pub fn market_event(id: &str, kind: &str, magnitude: f64, at_step: u64) -> Event {
    let mut e = Event::new(finance_id("market_event", id), "market_event", at_step);
    e.payload
        .insert("kind".to_string(), PropertyValue::Text(kind.to_string()));
    e.payload
        .insert("magnitude".to_string(), PropertyValue::Number(magnitude));
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Event;

    fn fake_order_id(s: &str) -> SemanticId {
        finance_id("order", s)
    }

    #[test]
    fn trade_links_both_orders_as_causes() {
        let buy = fake_order_id("buy_1");
        let sell = fake_order_id("sell_1");
        let t = trade("t1", &buy, &sell, 0.5, Money::usd(67_000.0), 1);
        assert_eq!(t.causes, vec![buy, sell]);
        assert!(matches!(
            t.payload.get("fill_size"),
            Some(PropertyValue::Number(n)) if (*n - 0.5).abs() < 1e-9
        ));
        // Notional = 0.5 * 67_000 = 33_500.
        if let Some(PropertyValue::Map(m)) = t.payload.get("notional") {
            if let Some(PropertyValue::Number(n)) = m.get("amount") {
                assert!((*n - 33_500.0).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn settlement_carries_state_and_trade_cause() {
        let buy = fake_order_id("b");
        let sell = fake_order_id("s");
        let t = trade("t1", &buy, &sell, 1.0, Money::usd(100.0), 1);
        let s = settlement("set_1", &t, SettlementState::Settled, 5);
        assert_eq!(s.causes, vec![t.id.clone()]);
        assert!(matches!(
            s.payload.get("state"),
            Some(PropertyValue::Text(state)) if state == "settled"
        ));
    }

    #[test]
    fn dividend_fans_out_to_every_holder() {
        let aapl = finance_id("asset", "AAPL");
        let h1 = finance_id("account", "h1");
        let h2 = finance_id("account", "h2");
        let h3 = finance_id("account", "h3");
        let d = dividend(
            "d1",
            &aapl,
            &[h1.clone(), h2.clone(), h3.clone()],
            Money::usd(0.25),
            10,
        );
        assert_eq!(d.causes, vec![aapl]);
        assert_eq!(d.effects, vec![h1, h2, h3]);
    }

    #[test]
    fn margin_call_records_shortfall() {
        let acc = finance_id("account", "alice");
        let mc = margin_call("mc1", &acc, Money::usd(1_500.0), 7);
        assert_eq!(mc.causes, vec![acc]);
        if let Some(PropertyValue::Map(m)) = mc.payload.get("shortfall") {
            if let Some(PropertyValue::Number(n)) = m.get("amount") {
                assert!((*n - 1_500.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn default_event_carries_amount_owed() {
        let acc = finance_id("account", "bob");
        let d = default_event("def1", &acc, Money::usd(2_000_000.0), 20);
        assert_eq!(d.causes, vec![acc]);
        if let Some(PropertyValue::Map(m)) = d.payload.get("amount_owed") {
            if let Some(PropertyValue::Number(n)) = m.get("amount") {
                assert!((*n - 2_000_000.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn market_event_records_kind_and_magnitude() {
        let m = market_event("crash_1", "circuit_breaker", -0.07, 100);
        assert!(matches!(
            m.payload.get("kind"),
            Some(PropertyValue::Text(k)) if k == "circuit_breaker"
        ));
        assert!(matches!(
            m.payload.get("magnitude"),
            Some(PropertyValue::Number(n)) if (*n + 0.07).abs() < 1e-9
        ));
    }
}
