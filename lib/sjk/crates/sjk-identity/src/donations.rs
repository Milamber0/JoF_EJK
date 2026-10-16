//! The donation goal (`PROTOCOL.md`, "Donations"): how much of the goal came in this
//! period and where to donate, as the hub answers `GET /v1/donations/goal`. It needs no
//! identity and sends nothing about the player.

use serde_json::Value;

/// Where to donate when the hub does not say (SJK's Ko-fi page).
pub const DEFAULT_URL: &str = "https://ko-fi.com/soljk";

/// The goal as the hub answered it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationGoal {
    /// Where to donate: an https address.
    pub url: String,
    /// The goal's currency, `EUR`.
    pub currency: String,
    /// The goal, in cents.
    pub goal_cents: i64,
    /// What came in this period, in cents.
    pub raised_cents: i64,
    /// Whether the goal is monthly (the count starts again on the 1st); otherwise a total.
    pub monthly: bool,
}

impl DonationGoal {
    /// The goal in `answer`, or `None` when it is not one: no positive goal, or a
    /// donation address that is not https (it is opened in the player's browser).
    pub fn parse(answer: &Value) -> Option<Self> {
        let goal_cents = answer["goal_cents"].as_i64().filter(|cents| *cents > 0)?;
        let raised_cents = answer["raised_cents"].as_i64().unwrap_or(0).max(0);
        let url = answer["url"]
            .as_str()
            .filter(|url| url.starts_with("https://") && !url.contains(char::is_whitespace))
            .unwrap_or(DEFAULT_URL)
            .to_owned();
        let currency = answer["currency"]
            .as_str()
            .filter(|code| code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_uppercase()))
            .unwrap_or("EUR")
            .to_owned();
        Some(Self {
            url,
            currency,
            goal_cents,
            raised_cents,
            monthly: answer["period"].as_str() != Some("total"),
        })
    }

    /// How full the bar is, 0 to 1.
    pub fn fill(&self) -> f32 {
        (self.raised_cents as f64 / self.goal_cents as f64).clamp(0.0, 1.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_hubs_answer_is_read_and_checked() {
        let goal = DonationGoal::parse(&json!({
            "url": "https://ko-fi.com/soljk", "currency": "EUR", "goal_cents": 20000,
            "raised_cents": 5000, "period": "month", "period_start": 0
        }))
        .unwrap();
        assert_eq!(goal.goal_cents, 20_000);
        assert!(goal.monthly);
        assert!((goal.fill() - 0.25).abs() < 1e-6);
        let over = DonationGoal::parse(&json!({"goal_cents": 100, "raised_cents": 900,
            "period": "total", "url": "http://evil.example"}))
        .unwrap();
        assert_eq!(over.fill(), 1.0);
        assert!(!over.monthly);
        assert_eq!(over.url, DEFAULT_URL);
        assert_eq!(over.currency, "EUR");
        assert!(DonationGoal::parse(&json!({"goal_cents": 0})).is_none());
        assert!(DonationGoal::parse(&json!({"error": "not_found"})).is_none());
    }
}
