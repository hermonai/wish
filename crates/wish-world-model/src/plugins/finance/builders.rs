//! Finance plugin — Object + Graph builders.
//!
//! These functions construct URE primitives in the finance realm.
//! Designed so a downstream `vibe-finance` project can use them as
//! a vocabulary: every method here matches a real-world financial
//! concept.

use super::types::{AssetClass, Currency, Money, OrderKind, OrderStatus, PositionSide, Side};
use crate::primitives::{Graph, GraphEdge, Object, PropertyValue};
use crate::semantic_id::{Realm, SemanticId};

/// Build a finance-realm SemanticId.
pub fn finance_id(kind: &str, key: impl Into<String>) -> SemanticId {
    SemanticId::new(Realm::Finance, kind, key)
}

fn money_property(m: &Money) -> PropertyValue {
    let mut map = std::collections::HashMap::new();
    map.insert("amount".to_string(), PropertyValue::Number(m.amount));
    map.insert(
        "currency".to_string(),
        PropertyValue::Text(m.currency.as_str().to_string()),
    );
    PropertyValue::Map(map)
}

/// **Account** — a balance bucket owned by an institution / person.
///
/// Properties:
/// - `owner` — owner's SemanticId (Agent or Institution)
/// - `currency` — base currency
/// - `balance` — current balance Money
/// - `kind` (overridden on the Object) carries the account type:
///   `"cash"`, `"margin"`, `"custody"`, etc.
pub fn account(
    id: &str,
    owner: &SemanticId,
    currency: Currency,
    balance: f64,
    kind: &str,
) -> Object {
    let mut o = Object::new(
        finance_id("account", id),
        format!("account_{kind}"),
        format!("Account {id}"),
    );
    o.properties.insert(
        "owner".to_string(),
        PropertyValue::Ref(Box::new(owner.clone())),
    );
    o.properties.insert(
        "currency".to_string(),
        PropertyValue::Text(currency.as_str().to_string()),
    );
    o.properties.insert(
        "balance".to_string(),
        money_property(&Money::new(balance, currency)),
    );
    o
}

/// **Asset** — a tradable thing (equity, bond, crypto, commodity).
pub fn asset(symbol: &str, name: &str, asset_class: AssetClass) -> Object {
    let class_json = match &asset_class {
        AssetClass::Custom(s) => s.clone(),
        c => serde_json::to_value(c)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default(),
    };
    let mut o = Object::new(finance_id("asset", symbol), "asset", name);
    o.properties.insert(
        "symbol".to_string(),
        PropertyValue::Text(symbol.to_string()),
    );
    o.properties
        .insert("asset_class".to_string(), PropertyValue::Text(class_json));
    o
}

/// **Institution** — bank, exchange, fund, broker, regulator. Built
/// as an `Object` (a *thing* in the world) but the FinancePlugin
/// will additionally treat regulators as URE Agents for action
/// modeling.
pub fn institution(id: &str, name: &str, kind: &str) -> Object {
    let mut o = Object::new(finance_id("institution", id), "institution", name);
    o.properties.insert(
        "institution_kind".to_string(),
        PropertyValue::Text(kind.to_string()),
    );
    o
}

/// **Order** — a request to trade. The status starts `Pending`.
pub fn order(
    id: &str,
    account: &SemanticId,
    asset: &SemanticId,
    side: Side,
    size: f64,
    kind: OrderKind,
) -> Object {
    let mut o = Object::new(finance_id("order", id), "order", format!("Order {id}"));
    o.properties.insert(
        "account".to_string(),
        PropertyValue::Ref(Box::new(account.clone())),
    );
    o.properties.insert(
        "asset".to_string(),
        PropertyValue::Ref(Box::new(asset.clone())),
    );
    o.properties.insert(
        "side".to_string(),
        PropertyValue::Text(match side {
            Side::Buy => "buy".to_string(),
            Side::Sell => "sell".to_string(),
        }),
    );
    o.properties
        .insert("size".to_string(), PropertyValue::Number(size));
    let (kind_str, price_attr) = match &kind {
        OrderKind::Market => ("market", None),
        OrderKind::Limit { price } => ("limit", Some(("limit_price", price.clone()))),
        OrderKind::Stop { trigger } => ("stop", Some(("stop_trigger", trigger.clone()))),
        OrderKind::StopLimit { trigger, price } => {
            o.properties
                .insert("stop_trigger".to_string(), money_property(trigger));
            ("stop_limit", Some(("limit_price", price.clone())))
        }
    };
    o.properties.insert(
        "order_kind".to_string(),
        PropertyValue::Text(kind_str.into()),
    );
    if let Some((key, m)) = price_attr {
        o.properties.insert(key.to_string(), money_property(&m));
    }
    o.properties.insert(
        "status".to_string(),
        PropertyValue::Text(serialize_status(OrderStatus::Pending)),
    );
    o.properties
        .insert("filled_size".to_string(), PropertyValue::Number(0.0));
    o
}

fn serialize_status(s: OrderStatus) -> String {
    serde_json::to_value(s)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// **Position** — a held quantity of an asset by an account.
pub fn position(
    id: &str,
    account: &SemanticId,
    asset: &SemanticId,
    side: PositionSide,
    size: f64,
    entry_price: Money,
) -> Object {
    let mut o = Object::new(
        finance_id("position", id),
        "position",
        format!("Position {id}"),
    );
    o.properties.insert(
        "account".to_string(),
        PropertyValue::Ref(Box::new(account.clone())),
    );
    o.properties.insert(
        "asset".to_string(),
        PropertyValue::Ref(Box::new(asset.clone())),
    );
    o.properties.insert(
        "side".to_string(),
        PropertyValue::Text(match side {
            PositionSide::Long => "long".to_string(),
            PositionSide::Short => "short".to_string(),
        }),
    );
    o.properties
        .insert("size".to_string(), PropertyValue::Number(size));
    o.properties
        .insert("entry_price".to_string(), money_property(&entry_price));
    o
}

/// **Portfolio** — a named collection of positions + an account.
/// Carries the positions as a `List<Ref(position_id)>`.
pub fn portfolio(id: &str, name: &str, account: &SemanticId, positions: &[Object]) -> Object {
    let mut o = Object::new(finance_id("portfolio", id), "portfolio", name);
    o.properties.insert(
        "account".to_string(),
        PropertyValue::Ref(Box::new(account.clone())),
    );
    let refs: Vec<PropertyValue> = positions
        .iter()
        .map(|p| PropertyValue::Ref(Box::new(p.id.clone())))
        .collect();
    o.properties
        .insert("positions".to_string(), PropertyValue::List(refs));
    o.properties.insert(
        "position_count".to_string(),
        PropertyValue::Number(positions.len() as f64),
    );
    o
}

/// **Counterparty graph** — institutions as nodes, exposures as
/// weighted edges. Edge `kind` = `"exposure"`; `weight` = notional
/// in USD.
pub fn counterparty_graph(
    name: &str,
    institutions: &[Object],
    edges: &[(SemanticId, SemanticId, f64)],
) -> Graph {
    let mut g = Graph::new(
        finance_id("graph", format!("{name}-counterparty")),
        "counterparty_graph",
    );
    for inst in institutions {
        g.add_node(inst.id.clone());
    }
    for (from, to, notional) in edges {
        g.add_edge(GraphEdge {
            from: from.clone(),
            to: to.clone(),
            kind: "exposure".to_string(),
            weight: Some(*notional as f32),
        });
    }
    g
}

/// **Exposure graph** — accounts → assets edges, weight = position
/// size. Useful for "what does this account hold?" + sector
/// aggregation queries.
pub fn exposure_graph(name: &str, positions: &[Object]) -> Graph {
    let mut g = Graph::new(
        finance_id("graph", format!("{name}-exposure")),
        "exposure_graph",
    );
    for p in positions {
        if let (
            Some(PropertyValue::Ref(account)),
            Some(PropertyValue::Ref(asset)),
            Some(PropertyValue::Number(size)),
        ) = (
            p.properties.get("account"),
            p.properties.get("asset"),
            p.properties.get("size"),
        ) {
            let signed_size = match p.properties.get("side") {
                Some(PropertyValue::Text(s)) if s == "short" => -(*size as f32),
                _ => *size as f32,
            };
            g.add_edge(GraphEdge {
                from: (**account).clone(),
                to: (**asset).clone(),
                kind: "holds".to_string(),
                weight: Some(signed_size),
            });
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> SemanticId {
        SemanticId::new(Realm::Custom("agent".to_string()), "person", "alice")
    }

    #[test]
    fn account_carries_owner_currency_balance() {
        let a = account("acc_1", &alice(), Currency::usd(), 10_000.0, "cash");
        assert!(matches!(
            a.properties.get("owner"),
            Some(PropertyValue::Ref(_))
        ));
        assert!(matches!(
            a.properties.get("currency"),
            Some(PropertyValue::Text(s)) if s == "USD"
        ));
        if let Some(PropertyValue::Map(m)) = a.properties.get("balance") {
            assert!(matches!(m.get("amount"), Some(PropertyValue::Number(n)) if *n == 10_000.0));
        } else {
            panic!("balance must be a money map");
        }
    }

    #[test]
    fn asset_records_class_and_symbol() {
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        assert!(matches!(
            btc.properties.get("symbol"),
            Some(PropertyValue::Text(s)) if s == "BTC"
        ));
        // AssetClass::Crypto serializes to "crypto" via snake_case.
        if let Some(PropertyValue::Text(s)) = btc.properties.get("asset_class") {
            assert_eq!(s, "crypto");
        }
    }

    #[test]
    fn limit_order_records_price() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let o = order(
            "ord_1",
            &acc.id,
            &btc.id,
            Side::Buy,
            0.5,
            OrderKind::Limit {
                price: Money::usd(67_000.0),
            },
        );
        assert!(matches!(
            o.properties.get("order_kind"),
            Some(PropertyValue::Text(s)) if s == "limit"
        ));
        assert!(o.properties.contains_key("limit_price"));
        assert!(matches!(
            o.properties.get("status"),
            Some(PropertyValue::Text(s)) if s == "pending"
        ));
    }

    #[test]
    fn stop_limit_records_both_trigger_and_limit() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let o = order(
            "ord_2",
            &acc.id,
            &btc.id,
            Side::Sell,
            0.5,
            OrderKind::StopLimit {
                trigger: Money::usd(65_000.0),
                price: Money::usd(64_500.0),
            },
        );
        assert!(o.properties.contains_key("stop_trigger"));
        assert!(o.properties.contains_key("limit_price"));
    }

    #[test]
    fn position_records_side_size_entry() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let p = position(
            "pos_1",
            &acc.id,
            &btc.id,
            PositionSide::Long,
            0.5,
            Money::usd(67_000.0),
        );
        assert!(matches!(
            p.properties.get("side"),
            Some(PropertyValue::Text(s)) if s == "long"
        ));
        assert!(matches!(
            p.properties.get("size"),
            Some(PropertyValue::Number(n)) if *n == 0.5
        ));
    }

    #[test]
    fn portfolio_references_positions() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let pos1 = position(
            "p1",
            &acc.id,
            &btc.id,
            PositionSide::Long,
            0.5,
            Money::usd(67_000.0),
        );
        let pos2 = position(
            "p2",
            &acc.id,
            &btc.id,
            PositionSide::Long,
            0.25,
            Money::usd(70_000.0),
        );
        let port = portfolio("alpha", "Alpha Strategy", &acc.id, &[pos1, pos2]);
        if let Some(PropertyValue::List(refs)) = port.properties.get("positions") {
            assert_eq!(refs.len(), 2);
        }
    }

    #[test]
    fn counterparty_graph_carries_notional_as_weight() {
        let goldman = institution("gs", "Goldman Sachs", "bank");
        let citadel = institution("cit", "Citadel", "fund");
        let g = counterparty_graph(
            "primebroker",
            &[goldman.clone(), citadel.clone()],
            &[(goldman.id.clone(), citadel.id.clone(), 50_000_000.0)],
        );
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].weight, Some(50_000_000.0));
    }

    #[test]
    fn exposure_graph_signs_short_positions_negative() {
        let acc = account("a", &alice(), Currency::usd(), 0.0, "margin");
        let btc = asset("BTC", "Bitcoin", AssetClass::Crypto);
        let long = position(
            "l",
            &acc.id,
            &btc.id,
            PositionSide::Long,
            0.5,
            Money::usd(67_000.0),
        );
        let short = position(
            "s",
            &acc.id,
            &btc.id,
            PositionSide::Short,
            0.3,
            Money::usd(67_000.0),
        );
        let g = exposure_graph("p", &[long, short]);
        assert_eq!(g.edges.len(), 2);
        // Sum of edge weights should be 0.5 - 0.3 = 0.2.
        let total: f32 = g.edges.iter().map(|e| e.weight.unwrap_or(0.0)).sum();
        assert!((total - 0.2).abs() < 1e-5);
    }
}
