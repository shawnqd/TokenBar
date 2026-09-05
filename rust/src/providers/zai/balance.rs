use reqwest::Url;
use serde_json::Value;

/// BigModel CN pay-as-you-go wallet (upstream zai plugin account-balance row).
/// Coding Plan 5h/week windows stay on the quota API; this is the remaining
/// prepaid/package wallet on `www.bigmodel.cn`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CnAccountBalance {
    pub available: f64,
    pub recharge: Option<f64>,
    pub give: Option<f64>,
    pub spent: Option<f64>,
}

impl CnAccountBalance {
    /// Transport string consumed by `providerBalance.parseBalanceText`.
    /// Amount first; Paid/Granted in parens so the tray shows 含赠送.
    pub fn format_row(&self) -> String {
        let amount = format!("¥{:.2}", self.available);
        let mut parts = Vec::new();
        if let Some(paid) = self.recharge {
            parts.push(format!("Paid: ¥{paid:.2}"));
        }
        if let Some(granted) = self.give {
            parts.push(format!("Granted: ¥{granted:.2}"));
        }
        if let Some(spent) = self.spent {
            parts.push(format!("Spent: ¥{spent:.2}"));
        }
        if parts.is_empty() {
            format!("{amount} available")
        } else {
            format!("{amount} ({})", parts.join(" / "))
        }
    }
}

pub(super) fn parse_cn_account_balance(data: &Value) -> Option<CnAccountBalance> {
    let available = ["availableBalance", "balance"]
        .into_iter()
        .find_map(|key| finite_nonnegative_number(data.get(key)))?;
    Some(CnAccountBalance {
        available,
        recharge: finite_nonnegative_number(data.get("rechargeAmount")),
        give: finite_nonnegative_number(data.get("giveAmount")),
        spent: finite_nonnegative_number(data.get("totalSpendAmount")),
    })
}

pub(super) async fn fetch_cn_account_balance(
    client: &reqwest::Client,
    authorization: &str,
    url: &Url,
) -> Option<CnAccountBalance> {
    let response = client
        .get(url.as_str())
        .header("Authorization", authorization)
        .header("Accept", "application/json")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().await.ok()?;
    if body.get("success").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    parse_cn_account_balance(body.get("data")?)
}

fn finite_nonnegative_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .filter(|value| value.is_finite() && *value >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_parser_prefers_available_balance_and_rejects_null() {
        let payload = serde_json::json!({
            "availableBalance": 40.0,
            "balance": 42.5,
            "rechargeAmount": 100.0,
            "giveAmount": 20.0,
            "totalSpendAmount": 77.5,
            "frozenBalance": 2.5
        });
        let parsed = parse_cn_account_balance(&payload).expect("wallet");
        assert_eq!(parsed.available, 40.0);
        assert_eq!(parsed.recharge, Some(100.0));
        assert_eq!(parsed.give, Some(20.0));
        assert_eq!(parsed.spent, Some(77.5));
        assert_eq!(
            parsed.format_row(),
            "¥40.00 (Paid: ¥100.00 / Granted: ¥20.00 / Spent: ¥77.50)"
        );
        let null_payload = serde_json::json!({"availableBalance": null, "balance": 42.5});
        let fallback = parse_cn_account_balance(&null_payload).expect("fallback");
        assert_eq!(fallback.available, 42.5);
        assert_eq!(fallback.format_row(), "¥42.50 available");
    }

    #[test]
    fn balance_parser_accepts_numeric_strings_but_rejects_negative() {
        let numeric = serde_json::json!("42.25");
        let negative = serde_json::json!(-1.0);
        assert_eq!(finite_nonnegative_number(Some(&numeric)), Some(42.25));
        assert_eq!(finite_nonnegative_number(Some(&negative)), None);
        let granted_only = serde_json::json!({
            "balance": 42.5,
            "availableBalance": null,
            "rechargeAmount": null,
            "giveAmount": 5.0
        });
        let parsed = parse_cn_account_balance(&granted_only).expect("granted");
        assert_eq!(parsed.format_row(), "¥42.50 (Granted: ¥5.00)");
    }
}
