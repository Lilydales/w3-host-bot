//! Direct Strike player lookups against ds-report.com.
//!
//! `lookup` blocks, so it only ever runs on a short-lived background thread
//! (see `hosting::ds_check`). Network failures are never treated as a verdict.
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

pub const URL: &str = "https://www.ds-report.com/api/w3c-player-details";
pub const ORIGIN: &str = "https://www.ds-report.com";
pub const REFERER: &str = "https://www.ds-report.com/";
pub const DEFAULT_PATCH: &str = "5Direct Strike 6_5_13f";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DsStats {
    pub level: u32,
    pub matches: u32,
}

impl DsStats {
    /// The compact label shown beside a player in the launcher.
    pub fn badge(stats: Option<Self>) -> String {
        match stats {
            Some(s) => format!("[Lv. {} | {}m]", s.level, s.matches),
            None => "[Lv. N/A]".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Found(DsStats),
    /// HTTP 404 or 500: the player is not on W3Champions.
    NotFound,
    /// Timeout, connection error or an unreadable answer. Never grounds for a kick.
    Failed,
}

impl Lookup {
    pub fn stats(self) -> Option<DsStats> {
        match self {
            Self::Found(stats) => Some(stats),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct Response {
    level: u32,
    #[serde(rename = "filteredTotal")]
    filtered_total: Total,
}

#[derive(Deserialize)]
struct Total {
    matches: u32,
}

pub fn parse(body: &str) -> Option<DsStats> {
    let response: Response = serde_json::from_str(body).ok()?;
    Some(DsStats {
        level: response.level,
        matches: response.filtered_total.matches,
    })
}

pub fn request_body(tag: &str, patch: &str) -> String {
    json!({"BattleNetTag": tag, "PatchVersion": patch, "PickOrder": 0}).to_string()
}

pub fn validate_patch(patch: &str) -> Result<(), String> {
    if patch.trim().is_empty() || patch.len() > 64 || patch.chars().any(char::is_control) {
        return Err("Direct Strike patch version must be 1 to 64 characters on one line.".into());
    }
    Ok(())
}

/// Blocking. Call from a background thread only.
pub fn lookup(tag: &str, patch: &str) -> Lookup {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    let sent = agent
        .post(URL)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .set("Origin", ORIGIN)
        .set("Referer", REFERER)
        .send_string(&request_body(tag, patch));
    match sent {
        Ok(response) => response
            .into_string()
            .ok()
            .and_then(|text| parse(&text))
            .map_or(Lookup::Failed, Lookup::Found),
        Err(ureq::Error::Status(404 | 500, _)) => Lookup::NotFound,
        Err(_) => Lookup::Failed,
    }
}

/// Whether the filters (0 = off) say this player should be removed.
pub fn rejects(min_level: u32, min_matches: u32, result: Lookup) -> bool {
    if min_level == 0 && min_matches == 0 {
        return false;
    }
    match result {
        Lookup::NotFound => true,
        Lookup::Failed => false,
        Lookup::Found(s) => {
            (min_level > 0 && s.level < min_level) || (min_matches > 0 && s.matches < min_matches)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn found(level: u32, matches: u32) -> Lookup {
        Lookup::Found(DsStats { level, matches })
    }

    #[test]
    fn level_and_filtered_matches_are_read_and_other_fields_ignored() {
        let body = r#"{"level":57,"name":"x","filteredTotal":{"matches":312,"wins":1},"extra":[1]}"#;
        assert_eq!(parse(body), Some(DsStats { level: 57, matches: 312 }));
        assert_eq!(parse(r#"{"level":5}"#), None);
        assert_eq!(parse("<html>"), None);
        assert_eq!(parse(r#"{"level":-1,"filteredTotal":{"matches":1}}"#), None);
    }

    #[test]
    fn the_request_body_uses_the_documented_field_names() {
        let body: serde_json::Value =
            serde_json::from_str(&request_body("Name#1234", DEFAULT_PATCH)).unwrap();
        assert_eq!(body["BattleNetTag"], "Name#1234");
        assert_eq!(body["PatchVersion"], "5Direct Strike 6_5_13f");
        assert_eq!(body["PickOrder"], 0);
    }

    #[test]
    fn disabled_filters_never_reject() {
        assert!(!rejects(0, 0, Lookup::NotFound));
        assert!(!rejects(0, 0, found(1, 0)));
    }

    #[test]
    fn missing_players_and_low_numbers_are_rejected_but_network_errors_are_not() {
        assert!(rejects(50, 0, Lookup::NotFound));
        assert!(rejects(0, 50, Lookup::NotFound));
        assert!(!rejects(50, 50, Lookup::Failed));
        assert!(rejects(50, 0, found(49, 999)));
        assert!(!rejects(50, 0, found(50, 0)));
        assert!(rejects(0, 50, found(99, 49)));
        assert!(!rejects(0, 50, found(1, 50)));
        assert!(rejects(50, 50, found(60, 10)));
        assert!(!rejects(50, 50, found(60, 60)));
    }

    #[test]
    fn badges_show_level_and_matches_or_na() {
        assert_eq!(DsStats::badge(Some(DsStats { level: 12, matches: 340 })), "[Lv. 12 | 340m]");
        assert_eq!(DsStats::badge(None), "[Lv. N/A]");
    }
}
