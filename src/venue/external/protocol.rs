//! The connector protocol: one JSON request per line on the connector's
//! stdin, one JSON response per line on its stdout, answered in order.
//!
//! ```text
//! → {"id":7,"method":"balances","wallet":"operational"}
//! ← {"id":7,"ok":{"token":"4000000000000","quote":"5000000000"}}
//! → {"id":8,"method":"execute","order":{"type":"remove_band"},"market":{…}}
//! ← {"id":8,"error":"no_position"}
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::market::snapshot::MarketSnapshot;
use crate::venue::order::{VenueOrder, Wallet};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Call {
    /// Catch up with the market; answer `ok: null`.
    Observe { market: MarketSnapshot },
    /// Execute an order; answer a fill.
    Execute {
        order: VenueOrder,
        market: MarketSnapshot,
    },
    /// Report a wallet; answer a holding in base units.
    Balances { wallet: Wallet },
    /// Report the band position; answer a position or `null`.
    Position { market: MarketSnapshot },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub call: Call,
}

/// Exactly one of `ok` and `error` is meaningful; `error` wins when present.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default)]
    pub ok: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Call, Request, Response};
    use crate::venue::order::Wallet;

    #[test]
    fn requests_are_flat_json_objects() {
        let request = Request {
            id: 7,
            call: Call::Balances {
                wallet: Wallet::Operational,
            },
        };
        assert_eq!(
            serde_json::to_string(&request).expect("serialises"),
            r#"{"id":7,"method":"balances","wallet":"operational"}"#
        );
    }

    #[test]
    fn responses_carry_either_a_value_or_an_error() {
        let failed: Response =
            serde_json::from_str(r#"{"id":8,"error":"no_position"}"#).expect("parses");
        assert_eq!(failed.error.as_deref(), Some("no_position"));
        let empty: Response = serde_json::from_str(r#"{"id":9,"ok":null}"#).expect("parses");
        assert!(empty.error.is_none() && empty.ok.is_null());
    }
}
