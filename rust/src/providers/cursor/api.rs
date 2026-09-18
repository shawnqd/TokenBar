//! Cursor API client for fetching usage information
//!
//! Uses browser cookies to authenticate with cursor.com API

use crate::core::{CostSnapshot, NamedRateWindow, ProviderError, RateWindow};
use crate::providers::browser_cookie_header;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;

const BASE_URL: &str = "https://cursor.com";
const COOKIE_DOMAINS: [&str; 2] = ["cursor.com", "cursor.sh"];

pub(super) type CursorUsageResult = (
    RateWindow,
    Option<RateWindow>,
    Option<RateWindow>,
    Option<CostSnapshot>,
    Option<String>,
    Option<String>,
    Option<NamedRateWindow>,
);

/// Cursor API client
pub struct CursorApi {
    client: reqwest::Client,
}

impl CursorApi {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Fetch usage information from Cursor API
    /// Returns (primary, secondary, model_specific, cost, email, plan_type, grok_bot)
    pub async fn fetch_usage(&self) -> Result<CursorUsageResult, ProviderError> {
        // Try to get cookies from browser
        let cookie_header = self.get_cookie_header()?;
        self.fetch_usage_with_cookie_header(&cookie_header).await
    }

    /// Fetch usage information with an already resolved Cookie header.
    pub async fn fetch_usage_with_cookie_header(
        &self,
        cookie_header: &str,
    ) -> Result<CursorUsageResult, ProviderError> {
        // Fetch usage summary and user info in parallel
        let (usage_result, user_result, sand_result) = tokio::join!(
            self.fetch_usage_summary(cookie_header),
            self.fetch_user_info(cookie_header),
            self.fetch_sand_usage(cookie_header)
        );

        let usage_summary = usage_result?;
        let user_info = user_result.ok();

        // Cursor Enterprise/Business summaries can expose a zero or synthetic
        // allowance. An optional, bounded team lookup recovers the verified
        // member budget without making the normal summary path dependent on it.
        let team_budget = if usage_summary.is_team_plan()
            && let Some(email) = user_info
                .as_ref()
                .and_then(|user| user.email.as_deref())
                .map(str::trim)
                .filter(|email| !email.is_empty())
        {
            self.fetch_team_budget(cookie_header, email)
                .await
                .unwrap_or(None)
        } else {
            None
        };

        self.build_result_with_team_budget(
            usage_summary,
            user_info,
            sand_result.ok().flatten(),
            team_budget,
        )
    }

    fn get_cookie_header(&self) -> Result<String, ProviderError> {
        browser_cookie_header(&COOKIE_DOMAINS)
    }

    async fn fetch_usage_summary(
        &self,
        cookie_header: &str,
    ) -> Result<UsageSummary, ProviderError> {
        let url = format!("{}/api/usage-summary", BASE_URL);

        let response = self
            .client
            .get(&url)
            .header("Cookie", cookie_header)
            .header("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?;

        if response.status() == 401 || response.status() == 403 {
            return Err(ProviderError::AuthRequired);
        }

        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "Cursor API returned {}",
                response.status()
            )));
        }

        // Try structured deserialization first, fall back to raw JSON on failure
        let text = response
            .text()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;
        serde_json::from_str::<UsageSummary>(&text).map_err(|e| {
            tracing::warn!(
                "Cursor usage-summary parse error: {e}; response length: {} bytes",
                text.len()
            );
            ProviderError::Parse(e.to_string())
        })
    }

    async fn fetch_sand_usage(
        &self,
        cookie_header: &str,
    ) -> Result<Option<NamedRateWindow>, ProviderError> {
        let url = format!("{}/api/dashboard/get-sand-usage-status", BASE_URL);
        let response = self
            .client
            .post(&url)
            .header("Cookie", cookie_header)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("Origin", BASE_URL)
            .body("{}")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let status: SandUsageStatus = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;
        Ok(status.to_window(Utc::now()))
    }

    async fn fetch_user_info(&self, cookie_header: &str) -> Result<UserInfo, ProviderError> {
        let url = format!("{}/api/auth/me", BASE_URL);

        let response = self
            .client
            .get(&url)
            .header("Cookie", cookie_header)
            .header("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ProviderError::Other(
                "Failed to fetch user info".to_string(),
            ));
        }

        response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))
    }

    async fn fetch_team_budget(
        &self,
        cookie_header: &str,
        email: &str,
    ) -> Result<Option<CursorTeamBudget>, ProviderError> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let teams: CursorTeamsResponse = self
            .fetch_team_dashboard("teams", serde_json::json!({}), cookie_header, deadline)
            .await?;
        let Some(team_id) = teams.selected_id(cookie_header) else {
            return Ok(None);
        };

        let mut expected_pages = None;
        let mut candidate = None;
        for page in 1..=20_i64 {
            let spend: CursorTeamSpendResponse = self
                .fetch_team_dashboard(
                    "get-team-spend",
                    serde_json::json!({
                        "teamId": team_id,
                        "page": page,
                        "pageSize": 50,
                        "sortBy": "name",
                        "sortDirection": "asc"
                    }),
                    cookie_header,
                    deadline,
                )
                .await?;

            let Some(total_pages) = spend.total_pages else {
                return Ok(None);
            };
            if !(1..=20).contains(&total_pages)
                || expected_pages.is_some_and(|expected| expected != total_pages)
                || spend.team_member_spend.is_empty()
                || spend.team_member_spend.len() > 50
                || (page != total_pages && spend.team_member_spend.len() != 50)
            {
                return Ok(None);
            }
            expected_pages = Some(total_pages);

            for member in spend.team_member_spend {
                if member
                    .email
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|member_email| member_email.eq_ignore_ascii_case(email))
                {
                    if candidate.is_some() {
                        return Ok(None);
                    }
                    candidate = member.budget();
                    if candidate.is_none() {
                        return Ok(None);
                    }
                }
            }

            if page == total_pages {
                return Ok(candidate);
            }
        }

        Ok(None)
    }

    async fn fetch_team_dashboard<T: for<'de> serde::Deserialize<'de>>(
        &self,
        endpoint: &str,
        body: serde_json::Value,
        cookie_header: &str,
        deadline: std::time::Instant,
    ) -> Result<T, ProviderError> {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(ProviderError::Timeout);
        }

        let response = self
            .client
            .post(format!("{BASE_URL}/api/dashboard/{endpoint}"))
            .header("Cookie", cookie_header)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("Origin", BASE_URL)
            .header("Referer", format!("{BASE_URL}/dashboard"))
            .timeout(remaining)
            .json(&body)
            .send()
            .await?;

        if response.status() == 401 || response.status() == 403 {
            return Err(ProviderError::AuthRequired);
        }
        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "Cursor team API returned {}",
                response.status()
            )));
        }

        let bytes = response.bytes().await?;
        if std::time::Instant::now() >= deadline {
            return Err(ProviderError::Timeout);
        }
        serde_json::from_slice(&bytes).map_err(|error| ProviderError::Parse(error.to_string()))
    }

    fn build_result(
        &self,
        summary: UsageSummary,
        user_info: Option<UserInfo>,
        grok_bot: Option<NamedRateWindow>,
    ) -> Result<CursorUsageResult, ProviderError> {
        self.build_result_with_team_budget(summary, user_info, grok_bot, None)
    }

    fn build_result_with_team_budget(
        &self,
        summary: UsageSummary,
        user_info: Option<UserInfo>,
        grok_bot: Option<NamedRateWindow>,
        team_budget: Option<CursorTeamBudget>,
    ) -> Result<CursorUsageResult, ProviderError> {
        let billing_end = summary
            .billing_cycle_end
            .as_ref()
            .and_then(|s| parse_iso_date(s));

        let (percent_used, secondary, model_specific, cost_snapshot) =
            if let Some(budget) = team_budget {
                let percent = if budget.limit_usd > 0.0 {
                    clamp_percent(budget.used_usd / budget.limit_usd * 100.0)
                } else {
                    0.0
                };
                let cost = summary
                    .individual_usage
                    .as_ref()
                    .and_then(|individual| {
                        Self::on_demand_cost(individual.on_demand.as_ref(), billing_end)
                    })
                    .or_else(|| {
                        summary.team_usage.as_ref().and_then(|team| {
                            Self::on_demand_cost(team.on_demand.as_ref(), billing_end)
                        })
                    })
                    .or_else(|| Some(budget.to_cost_snapshot(billing_end)));
                (percent, None, None, cost)
            } else if let Some(individual) = &summary.individual_usage {
                if let Some(plan) = &individual.plan {
                    let used_cents = plan.used.unwrap_or(0) as f64;
                    let limit_cents = plan
                        .limit
                        .or_else(|| plan.breakdown.as_ref().and_then(|b| b.total))
                        .unwrap_or(0) as f64;

                    let percent = if let Some(percent) = plan.total_percent_used {
                        percent
                    } else if limit_cents > 0.0 {
                        (used_cents / limit_cents) * 100.0
                    } else {
                        0.0
                    };

                    let secondary = plan
                        .auto_percent_used
                        .map(|v| RateWindow::with_details(v, None, billing_end, None));

                    let model_specific = plan
                        .api_percent_used
                        .map(|v| RateWindow::with_details(v, None, billing_end, None));

                    let cost = Self::on_demand_cost(individual.on_demand.as_ref(), billing_end)
                        .or_else(|| {
                            summary.team_usage.as_ref().and_then(|team| {
                                Self::on_demand_cost(team.on_demand.as_ref(), billing_end)
                            })
                        })
                        .unwrap_or_else(|| {
                            let mut cost = CostSnapshot::new(used_cents / 100.0, "USD", "Monthly");
                            if limit_cents > 0.0 {
                                cost = cost.with_limit(limit_cents / 100.0);
                            }
                            if let Some(reset) = billing_end {
                                cost = cost.with_resets_at(reset);
                            }
                            cost
                        });

                    (percent, secondary, model_specific, Some(cost))
                } else if let Some(overall) = &individual.overall {
                    let percent = Self::usage_percent(overall).unwrap_or(0.0);
                    let cost = Self::on_demand_cost(Some(overall), billing_end);
                    (percent, None, None, cost)
                } else {
                    (0.0, None, None, None)
                }
            } else if let Some(team) = &summary.team_usage {
                if let Some(pooled) = &team.pooled {
                    let percent = Self::usage_percent(pooled).unwrap_or(0.0);
                    let cost = Self::on_demand_cost(Some(pooled), billing_end);
                    (percent, None, None, cost)
                } else {
                    (0.0, None, None, None)
                }
            } else {
                (0.0, None, None, None)
            };

        let primary = RateWindow::with_details(percent_used, None, billing_end, None);

        let plan_type = summary
            .membership_type
            .as_ref()
            .map(|t| match t.to_lowercase().as_str() {
                "enterprise" => "Cursor Enterprise".to_string(),
                "pro" => "Cursor Pro".to_string(),
                "hobby" => "Cursor Hobby".to_string(),
                "team" => "Cursor Team".to_string(),
                other => format!("Cursor {}", capitalize(other)),
            });

        let email = user_info.as_ref().and_then(|u| u.email.clone());

        Ok((
            primary,
            secondary,
            model_specific,
            cost_snapshot,
            email,
            plan_type,
            grok_bot,
        ))
    }

    fn on_demand_cost(
        on_demand: Option<&OnDemandUsage>,
        billing_end: Option<DateTime<Utc>>,
    ) -> Option<CostSnapshot> {
        let usage = on_demand?;
        if usage.enabled == Some(false) {
            return None;
        }

        let used_cents = usage.used.unwrap_or(0) as f64;
        let limit_cents = usage
            .limit
            .or_else(|| {
                usage
                    .remaining
                    .map(|remaining| remaining + usage.used.unwrap_or(0))
            })
            .unwrap_or(0) as f64;

        if used_cents <= 0.0 && limit_cents <= 0.0 {
            return None;
        }

        let mut cost = CostSnapshot::new(used_cents / 100.0, "USD", "Monthly");
        if limit_cents > 0.0 {
            cost = cost.with_limit(limit_cents / 100.0);
        }
        if let Some(reset) = billing_end {
            cost = cost.with_resets_at(reset);
        }
        Some(cost)
    }

    fn usage_percent(usage: &OnDemandUsage) -> Option<f64> {
        let used = usage.used.unwrap_or(0) as f64;
        let limit = usage
            .limit
            .or_else(|| {
                usage
                    .remaining
                    .map(|remaining| remaining + usage.used.unwrap_or(0))
            })
            .unwrap_or(0) as f64;
        (limit > 0.0).then_some(used / limit * 100.0)
    }
}

impl Default for CursorApi {
    fn default() -> Self {
        Self::new()
    }
}

// --- API Response Types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageSummary {
    billing_cycle_start: Option<String>,
    billing_cycle_end: Option<String>,
    membership_type: Option<String>,
    limit_type: Option<String>,
    is_unlimited: Option<bool>,
    individual_usage: Option<IndividualUsage>,
    team_usage: Option<TeamUsage>,
}

impl UsageSummary {
    fn is_team_plan(&self) -> bool {
        let membership = self
            .membership_type
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        matches!(
            membership.as_str(),
            "enterprise" | "business" | "team" | "teams"
        ) || self
            .limit_type
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("team"))
    }
}

#[derive(Debug, Deserialize)]
struct CursorTeamsResponse {
    #[serde(default)]
    teams: Vec<CursorTeam>,
}

#[derive(Debug, Deserialize)]
struct CursorTeam {
    id: i64,
}

impl CursorTeamsResponse {
    fn selected_id(&self, cookie_header: &str) -> Option<i64> {
        let ids: HashSet<i64> = self
            .teams
            .iter()
            .map(|team| team.id)
            .filter(|id| *id > 0)
            .collect();
        if ids.len() != self.teams.len() || ids.is_empty() {
            return None;
        }

        for cookie_name in ["portal-selected-team-id", "team_id"] {
            let values: Vec<&str> = cookie_header
                .split(';')
                .filter_map(|part| {
                    let (name, value) = part.trim().split_once('=')?;
                    (name == cookie_name).then_some(value.trim())
                })
                .collect();
            if !values.is_empty() {
                if values.len() != 1 {
                    return None;
                }
                let id = values[0].parse::<i64>().ok()?;
                return ids.contains(&id).then_some(id);
            }
        }

        (ids.len() == 1).then(|| ids.into_iter().next()).flatten()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorTeamSpendResponse {
    #[serde(default)]
    team_member_spend: Vec<CursorTeamMember>,
    total_pages: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorTeamMember {
    email: Option<String>,
    overall_spend_cents: Option<f64>,
    effective_per_user_limit_dollars: Option<f64>,
    monthly_limit_dollars: Option<f64>,
}

impl CursorTeamMember {
    fn budget(&self) -> Option<CursorTeamBudget> {
        let used_usd = self.overall_spend_cents? / 100.0;
        // If Cursor explicitly reports an effective limit, do not silently
        // replace a zero/unlimited value with the nominal monthly limit.
        let limit_usd = self
            .effective_per_user_limit_dollars
            .or(self.monthly_limit_dollars)?;
        (used_usd.is_finite() && used_usd >= 0.0 && limit_usd.is_finite() && limit_usd > 0.0)
            .then_some(CursorTeamBudget {
                used_usd,
                limit_usd,
            })
    }
}

#[derive(Debug, Clone, Copy)]
struct CursorTeamBudget {
    used_usd: f64,
    limit_usd: f64,
}

impl CursorTeamBudget {
    fn to_cost_snapshot(self, billing_end: Option<DateTime<Utc>>) -> CostSnapshot {
        let mut cost =
            CostSnapshot::new(self.used_usd, "USD", "Monthly").with_limit(self.limit_usd);
        if let Some(reset) = billing_end {
            cost = cost.with_resets_at(reset);
        }
        cost
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndividualUsage {
    plan: Option<PlanUsage>,
    on_demand: Option<OnDemandUsage>,
    overall: Option<OnDemandUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanUsage {
    enabled: Option<bool>,
    used: Option<i64>,
    limit: Option<i64>,
    remaining: Option<i64>,
    breakdown: Option<PlanBreakdown>,
    auto_percent_used: Option<f64>,
    api_percent_used: Option<f64>,
    total_percent_used: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanBreakdown {
    included: Option<i64>,
    bonus: Option<i64>,
    total: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnDemandUsage {
    enabled: Option<bool>,
    used: Option<i64>,
    limit: Option<i64>,
    remaining: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamUsage {
    on_demand: Option<OnDemandUsage>,
    pooled: Option<OnDemandUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserInfo {
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    sub: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    picture: Option<String>,
}

// --- Helper functions ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SandUsageStatus {
    current_period_start: Option<String>,
    next_reset_timestamp_utc: Option<String>,
    usage_percent: Option<f64>,
    has_non_zero_included_limit: Option<bool>,
    included_limit_zero: Option<bool>,
    sand_trial_expires_at: Option<String>,
}

impl SandUsageStatus {
    fn to_window(&self, now: DateTime<Utc>) -> Option<NamedRateWindow> {
        let has_limit = self
            .included_limit_zero
            .map(|is_zero| !is_zero)
            .or(self.has_non_zero_included_limit);
        let has_trial = has_limit != Some(true)
            && self
                .sand_trial_expires_at
                .as_deref()
                .and_then(parse_iso_date)
                .is_some_and(|expires_at| expires_at > now);
        if has_limit != Some(true) && !has_trial {
            return None;
        }
        let used = clamp_percent(self.usage_percent?);
        let start = self
            .current_period_start
            .as_deref()
            .and_then(parse_iso_date);
        let reset = (!has_trial)
            .then(|| {
                self.next_reset_timestamp_utc
                    .as_deref()
                    .and_then(parse_iso_date)
            })
            .flatten();
        let minutes = start.zip(reset).and_then(|(start, reset)| {
            let minutes = (reset - start).num_minutes();
            (minutes > 0).then_some(minutes as u32)
        });
        Some(NamedRateWindow::new(
            "cursor-grok-bot",
            "Grok Bot",
            RateWindow::with_details(used, minutes, reset, None),
        ))
    }
}

fn clamp_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 100.0)
}

fn parse_iso_date(s: &str) -> Option<DateTime<Utc>> {
    // Try with fractional seconds
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }

    // Try without fractional seconds
    if let Ok(dt) = chrono::DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(dt.with_timezone(&Utc));
    }

    None
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api() -> CursorApi {
        CursorApi::new()
    }

    fn parse_summary(json: &str) -> UsageSummary {
        serde_json::from_str(json).expect("fixture should parse")
    }

    #[test]
    fn test_cursor_build_result_with_lanes() {
        let json = r#"{
            "billingCycleStart": "2026-03-01T00:00:00Z",
            "billingCycleEnd": "2026-04-01T00:00:00Z",
            "membershipType": "pro",
            "individualUsage": {
                "plan": {
                    "used": 1500,
                    "limit": 5000,
                    "totalPercentUsed": 30.0,
                    "autoPercentUsed": 20.0,
                    "apiPercentUsed": 10.0
                }
            }
        }"#;

        let summary = parse_summary(json);
        let (primary, secondary, model_specific, cost, _email, plan_type, grok_bot) =
            api().build_result(summary, None, None).unwrap();

        assert!((primary.used_percent - 30.0).abs() < 0.01);

        let sec = secondary.expect("secondary should be present");
        assert!((sec.used_percent - 20.0).abs() < 0.01);
        assert!(sec.resets_at.is_some());

        let ms = model_specific.expect("model_specific should be present");
        assert!((ms.used_percent - 10.0).abs() < 0.01);
        assert!(ms.resets_at.is_some());

        assert!(cost.is_some());
        assert_eq!(plan_type.as_deref(), Some("Cursor Pro"));
        assert!(grok_bot.is_none(), "no sand usage payload in fixture");
    }

    #[test]
    fn test_cursor_build_result_prefers_api_percent_fields() {
        let json = r#"{
            "membershipType": "pro",
            "autoModelSelectedDisplayMessage": "You've used 13% of your included total usage",
            "individualUsage": {
                "plan": {
                    "used": 2000,
                    "limit": 2000,
                    "breakdown": {
                        "included": 2000,
                        "bonus": 580,
                        "total": 2580
                    },
                    "autoPercentUsed": 17.2,
                    "apiPercentUsed": 0,
                    "totalPercentUsed": 13.230769230769232
                }
            }
        }"#;

        let summary = parse_summary(json);
        let (primary, secondary, model_specific, cost, _, plan_type, _grok_bot) =
            api().build_result(summary, None, None).unwrap();

        assert!((primary.used_percent - 13.230769230769232).abs() < 0.01);
        assert!((secondary.unwrap().used_percent - 17.2).abs() < 0.01);
        assert!((model_specific.unwrap().used_percent - 0.0).abs() < 0.01);

        let cost = cost.expect("plan usage should still produce cost snapshot");
        assert!((cost.used - 20.0).abs() < 0.01);
        assert_eq!(cost.limit, Some(20.0));
        assert_eq!(plan_type.as_deref(), Some("Cursor Pro"));
    }

    #[test]
    fn test_cursor_build_result_cents_only() {
        let json = r#"{
            "billingCycleEnd": "2026-04-01T00:00:00Z",
            "membershipType": "pro",
            "individualUsage": {
                "plan": {
                    "used": 2500,
                    "limit": 5000
                }
            }
        }"#;

        let summary = parse_summary(json);
        let (primary, secondary, model_specific, cost, _, _, _grok_bot) =
            api().build_result(summary, None, None).unwrap();

        assert!((primary.used_percent - 50.0).abs() < 0.01);
        assert!(secondary.is_none(), "no autoPercentUsed in payload");
        assert!(model_specific.is_none(), "no apiPercentUsed in payload");
        assert!(cost.is_some());
    }

    #[test]
    fn test_cursor_build_result_missing_plan() {
        let json = r#"{
            "membershipType": "hobby",
            "individualUsage": {}
        }"#;

        let summary = parse_summary(json);
        let (primary, secondary, model_specific, cost, _, _, _grok_bot) =
            api().build_result(summary, None, None).unwrap();

        assert!((primary.used_percent).abs() < 0.01);
        assert!(secondary.is_none());
        assert!(model_specific.is_none());
        assert!(cost.is_none());
    }

    #[test]
    fn test_cursor_on_demand_as_cost() {
        let json = r#"{
            "billingCycleEnd": "2026-04-01T00:00:00Z",
            "membershipType": "pro",
            "individualUsage": {
                "plan": {
                    "used": 800,
                    "limit": 5000,
                    "totalPercentUsed": 16.0
                },
                "onDemand": {
                    "enabled": true,
                    "used": 350,
                    "limit": 1000
                }
            }
        }"#;

        let summary = parse_summary(json);
        let (primary, _, _, cost, _, _, _grok_bot) =
            api().build_result(summary, None, None).unwrap();

        assert!((primary.used_percent - 16.0).abs() < 0.01);
        let cost = cost.expect("cost should exist from on-demand usage");
        assert!((cost.used - 3.5).abs() < 0.01);
        assert_eq!(cost.limit, Some(10.0));
        assert_eq!(cost.period, "Monthly");
    }

    #[test]
    fn test_cursor_individual_overall_fallback() {
        let summary =
            parse_summary(r#"{"individualUsage":{"overall":{"used":2500,"limit":10000}}}"#);
        let (primary, _, _, cost, _, _, _grok_bot) =
            api().build_result(summary, None, None).unwrap();
        assert!((primary.used_percent - 25.0).abs() < 0.01);
        assert_eq!(cost.unwrap().limit, Some(100.0));
    }

    #[test]
    fn test_cursor_team_pooled_fallback() {
        let summary = parse_summary(r#"{"teamUsage":{"pooled":{"used":5000,"limit":10000}}}"#);
        let (primary, _, _, cost, _, _, _grok_bot) =
            api().build_result(summary, None, None).unwrap();
        assert!((primary.used_percent - 50.0).abs() < 0.01);
        assert_eq!(cost.unwrap().used, 50.0);
    }

    #[test]
    fn team_selection_prefers_portal_cookie_and_rejects_ambiguous_state() {
        let teams: CursorTeamsResponse = serde_json::from_value(serde_json::json!({
            "teams": [{ "id": 11 }, { "id": 22 }]
        }))
        .unwrap();
        assert_eq!(
            teams.selected_id("team_id=11; portal-selected-team-id=22"),
            Some(22)
        );
        assert_eq!(teams.selected_id("team_id=11"), Some(11));
        assert_eq!(teams.selected_id("team_id=99"), None);
        assert_eq!(teams.selected_id("team_id=11; team_id=22"), None);
        assert_eq!(teams.selected_id(""), None);
    }

    #[test]
    fn a_single_team_can_be_selected_without_a_cookie() {
        let teams: CursorTeamsResponse = serde_json::from_value(serde_json::json!({
            "teams": [{ "id": 22 }]
        }))
        .unwrap();
        assert_eq!(teams.selected_id("auth=fixture"), Some(22));

        let duplicate: CursorTeamsResponse = serde_json::from_value(serde_json::json!({
            "teams": [{ "id": 22 }, { "id": 22 }]
        }))
        .unwrap();
        assert_eq!(duplicate.selected_id("auth=fixture"), None);
    }

    #[test]
    fn verified_team_budget_overrides_synthetic_summary_allowance() {
        let summary = parse_summary(
            r#"{
                "membershipType":"enterprise",
                "billingCycleEnd":"2026-10-01T00:00:00Z",
                "individualUsage":{
                    "plan":{"used":0,"limit":2000,"totalPercentUsed":0,"autoPercentUsed":0,"apiPercentUsed":0},
                    "onDemand":{"used":250,"limit":1000}
                }
            }"#,
        );
        let budget = CursorTeamBudget {
            used_usd: 13.12,
            limit_usd: 150.0,
        };
        let (primary, secondary, model, cost, _, plan, _) = api()
            .build_result_with_team_budget(summary, None, None, Some(budget))
            .unwrap();

        assert!((primary.used_percent - (13.12 / 150.0 * 100.0)).abs() < 0.0001);
        assert!(secondary.is_none());
        assert!(model.is_none());
        // Extra/on-demand charges remain the cost source when the summary has
        // them; the verified budget still drives the quota percentage.
        assert_eq!(cost.unwrap().used, 2.5);
        assert_eq!(plan.as_deref(), Some("Cursor Enterprise"));
    }

    #[test]
    fn invalid_effective_team_limit_does_not_fall_back_to_monthly_limit() {
        let member: CursorTeamMember = serde_json::from_value(serde_json::json!({
            "email": "member@example.com",
            "overallSpendCents": 1312,
            "effectivePerUserLimitDollars": 0,
            "monthlyLimitDollars": 150
        }))
        .unwrap();
        assert!(member.budget().is_none());

        let member: CursorTeamMember = serde_json::from_value(serde_json::json!({
            "email": "member@example.com",
            "overallSpendCents": 1312,
            "monthlyLimitDollars": 150
        }))
        .unwrap();
        let budget = member.budget().unwrap();
        assert_eq!(budget.used_usd, 13.12);
        assert_eq!(budget.limit_usd, 150.0);
    }

    #[test]
    fn sand_usage_maps_paid_allowance_from_explicit_zero_flag() {
        let status = SandUsageStatus {
            current_period_start: Some("2026-08-18T00:00:00Z".into()),
            next_reset_timestamp_utc: Some("2026-08-25T00:00:00Z".into()),
            usage_percent: Some(37.5),
            has_non_zero_included_limit: Some(false),
            included_limit_zero: Some(false),
            sand_trial_expires_at: None,
        };
        let row = status
            .to_window("2026-08-20T00:00:00Z".parse().unwrap())
            .expect("paid Grok Bot window");
        assert_eq!(row.id, "cursor-grok-bot");
        assert_eq!(row.title, "Grok Bot");
        assert_eq!(row.window.window_minutes, Some(10080));
        assert_eq!(
            row.window.resets_at,
            Some("2026-08-25T00:00:00Z".parse().unwrap())
        );
    }

    #[test]
    fn sand_usage_maps_an_active_trial_without_a_recurring_reset() {
        let status = SandUsageStatus {
            current_period_start: Some("2026-08-18T00:00:00Z".into()),
            next_reset_timestamp_utc: Some("2026-08-25T00:00:00Z".into()),
            usage_percent: Some(12.5),
            has_non_zero_included_limit: Some(false),
            included_limit_zero: Some(true),
            sand_trial_expires_at: Some("2026-08-28T00:00:00Z".into()),
        };
        let row = status
            .to_window("2026-08-20T00:00:00Z".parse().unwrap())
            .expect("active trial window");
        assert_eq!(row.window.resets_at, None);
        assert_eq!(row.window.window_minutes, None);
        assert!((row.window.used_percent - 12.5).abs() < 0.001);
    }

    #[test]
    fn sand_usage_hides_expired_trial() {
        let status = SandUsageStatus {
            current_period_start: None,
            next_reset_timestamp_utc: None,
            usage_percent: Some(12.5),
            has_non_zero_included_limit: Some(false),
            included_limit_zero: Some(true),
            sand_trial_expires_at: Some("2026-08-19T00:00:00Z".into()),
        };
        assert!(
            status
                .to_window("2026-08-20T00:00:00Z".parse().unwrap())
                .is_none()
        );
    }
}
