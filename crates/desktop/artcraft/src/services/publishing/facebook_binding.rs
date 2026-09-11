//! Durable Facebook destination configuration and ephemeral browser identity.
//!
//! The CDP target id is deliberately kept in the runtime portion of the
//! binding.  It is never treated as a durable browser identifier: a PID or
//! launch generation change invalidates it and requires a fresh `/pages`
//! reconciliation.

use serde::{Deserialize, Serialize};
use sqlite_tasks::connection::TaskDbConnection;
use sqlite_tasks::queries::floword_settings::get_floword_setting::get_floword_setting;

const SETTINGS_PREFIX: &str = "facebook_publishing";

fn setting_key(kind: &str, id: &str) -> String {
  format!("{SETTINGS_PREFIX}.{kind}.{}", id.trim())
}

/// Read the durable binding from the existing Floword settings table. A
/// malformed row is treated as absent so callers fail closed rather than
/// dispatching against an untrusted target.
pub async fn load_runtime_binding(db: &TaskDbConnection, binding_id: &str) -> Result<Option<FacebookRuntimeBinding>, String> {
  let Some(row) = get_floword_setting(db, &setting_key("runtime_binding", binding_id)).await.map_err(|e| e.to_string())? else {
    return Ok(None);
  };
  serde_json::from_str(&row.value_json).map(Some).map_err(|e| e.to_string())
}

pub async fn load_page_snapshot(db: &TaskDbConnection, binding_id: &str) -> Result<Option<FacebookPageSnapshot>, String> {
  let Some(row) = get_floword_setting(db, &setting_key("page_snapshot", binding_id)).await.map_err(|e| e.to_string())? else {
    return Ok(None);
  };
  serde_json::from_str(&row.value_json).map(Some).map_err(|e| e.to_string())
}

pub async fn load_confirmation(db: &TaskDbConnection, publication_id: &str) -> Result<Option<LivePublishConfirmation>, String> {
  let Some(row) = get_floword_setting(db, &setting_key("confirmation", publication_id)).await.map_err(|e| e.to_string())? else {
    return Ok(None);
  };
  serde_json::from_str(&row.value_json).map(Some).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FacebookRuntimeIdentity {
  pub browser_pid: u32,
  pub launch_generation: u64,
  pub cdp_endpoint: String,
  pub managed_target_id: String,
  pub managed_page_url: String,
  pub claimed_at_utc: i64,
  pub last_validated_at_utc: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FacebookRuntimeBinding {
  pub binding_id: String,
  pub donut_profile_id: String,
  pub target_kind: String,
  pub enabled: bool,
  pub auto_start_managed_donut_profile: bool,
  pub created_at_utc: i64,
  pub updated_at_utc: i64,
  #[serde(default)]
  pub runtime: Option<FacebookRuntimeIdentity>,
}

impl FacebookRuntimeBinding {
  pub fn runtime_is_current(&self, profile_id: &str, identity: &FacebookRuntimeIdentity, page_is_live: bool) -> bool {
    let Some(runtime) = self.runtime.as_ref() else {
      return false;
    };
    self.enabled && self.target_kind == "FACEBOOK" && self.donut_profile_id == profile_id && runtime == identity && page_is_live
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FacebookPageSnapshot {
  pub facebook_page_binding_id: String,
  pub donut_profile_id: String,
  pub facebook_page_id: Option<String>,
  pub facebook_page_canonical_url: Option<String>,
  pub facebook_page_display_name: String,
  pub identity_evidence: String,
  pub identity_captured_at_utc: i64,
  pub identity_last_verified_at_utc: i64,
}

impl FacebookPageSnapshot {
  pub fn matches_current(&self, page_id: Option<&str>, canonical_url: Option<&str>, display_name: &str, evidence: bool) -> bool {
    if !evidence || self.facebook_page_display_name.trim() != display_name.trim() {
      return false;
    }
    match (self.facebook_page_id.as_deref(), page_id, self.facebook_page_canonical_url.as_deref(), canonical_url) {
      (Some(expected), Some(actual), _, _) => expected == actual,
      (None, _, Some(expected), Some(actual)) => expected == actual,
      _ => false,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LivePublishConfirmation {
  pub publication_id: String,
  pub scheduled_occurrence_id: String,
  pub donut_profile_id: String,
  pub facebook_page_id: Option<String>,
  pub video_sha256: String,
  pub caption_sha256: String,
  pub visible_link: Option<String>,
  pub confirmation_created_at_utc: i64,
  pub confirmation_expires_at_utc: i64,
  pub confirmed_by_user: bool,
}

impl LivePublishConfirmation {
  pub fn is_valid_for(&self, publication_id: &str, occurrence_id: &str, profile_id: &str, page_id: Option<&str>, video_sha256: &str, caption_sha256: &str, visible_link: Option<&str>, now_utc: i64) -> bool {
    self.confirmed_by_user && now_utc <= self.confirmation_expires_at_utc && self.publication_id == publication_id && self.scheduled_occurrence_id == occurrence_id && self.donut_profile_id == profile_id && self.facebook_page_id.as_deref() == page_id && self.video_sha256 == video_sha256 && self.caption_sha256 == caption_sha256 && self.visible_link.as_deref() == visible_link
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn binding() -> FacebookRuntimeBinding {
    FacebookRuntimeBinding { binding_id: "binding-1".into(), donut_profile_id: "profile-1".into(), target_kind: "FACEBOOK".into(), enabled: true, auto_start_managed_donut_profile: false, created_at_utc: 1, updated_at_utc: 2, runtime: Some(FacebookRuntimeIdentity { browser_pid: 10, launch_generation: 20, cdp_endpoint: "http://127.0.0.1:9222".into(), managed_target_id: "target-1".into(), managed_page_url: "https://www.facebook.com/me".into(), claimed_at_utc: 3, last_validated_at_utc: 4 }) }
  }

  #[test]
  fn runtime_binding_round_trips_and_pid_change_is_stale() {
    let original = binding();
    let encoded = serde_json::to_string(&original).unwrap();
    let decoded: FacebookRuntimeBinding = serde_json::from_str(&encoded).unwrap();
    let identity = decoded.runtime.clone().unwrap();
    assert!(decoded.runtime_is_current("profile-1", &identity, true));
    let mut changed = identity;
    changed.browser_pid += 1;
    assert!(!decoded.runtime_is_current("profile-1", &changed, true));
  }

  #[test]
  fn generation_change_and_dead_target_are_stale() {
    let original = binding();
    let identity = original.runtime.clone().unwrap();
    let mut changed = identity.clone();
    changed.launch_generation += 1;
    assert!(!original.runtime_is_current("profile-1", &changed, true));
    assert!(!original.runtime_is_current("profile-1", &identity, false));
  }

  #[test]
  fn page_snapshot_round_trip_requires_id_or_url_and_name() {
    let snapshot = FacebookPageSnapshot { facebook_page_binding_id: "page-binding".into(), donut_profile_id: "profile-1".into(), facebook_page_id: Some("page-1".into()), facebook_page_canonical_url: Some("https://www.facebook.com/example".into()), facebook_page_display_name: "Example Page".into(), identity_evidence: "publishing_surface".into(), identity_captured_at_utc: 1, identity_last_verified_at_utc: 2 };
    let decoded: FacebookPageSnapshot = serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap();
    assert!(decoded.matches_current(Some("page-1"), Some("https://www.facebook.com/example"), "Example Page", true));
    assert!(!decoded.matches_current(Some("other-page"), Some("https://www.facebook.com/example"), "Example Page", true));
  }

  #[test]
  fn page_snapshot_can_use_canonical_url_but_name_or_evidence_must_match() {
    let snapshot = FacebookPageSnapshot { facebook_page_binding_id: "page-binding".into(), donut_profile_id: "profile-1".into(), facebook_page_id: None, facebook_page_canonical_url: Some("https://www.facebook.com/example".into()), facebook_page_display_name: "Example Page".into(), identity_evidence: "publishing_surface".into(), identity_captured_at_utc: 1, identity_last_verified_at_utc: 2 };
    assert!(snapshot.matches_current(None, Some("https://www.facebook.com/example"), "Example Page", true));
    assert!(!snapshot.matches_current(None, Some("https://www.facebook.com/example"), "Other Page", true));
    assert!(!snapshot.matches_current(None, Some("https://www.facebook.com/example"), "Example Page", false));
  }

  #[test]
  fn confirmation_is_bound_to_exact_publication_snapshot() {
    let confirmation = LivePublishConfirmation { publication_id: "pub-1".into(), scheduled_occurrence_id: "occ-1".into(), donut_profile_id: "profile-1".into(), facebook_page_id: Some("page-1".into()), video_sha256: "video".into(), caption_sha256: "caption".into(), visible_link: None, confirmation_created_at_utc: 10, confirmation_expires_at_utc: 20, confirmed_by_user: true };
    assert!(confirmation.is_valid_for("pub-1", "occ-1", "profile-1", Some("page-1"), "video", "caption", None, 15));
    assert!(!confirmation.is_valid_for("pub-1", "occ-1", "profile-1", Some("page-1"), "other-video", "caption", None, 15));
    assert!(!confirmation.is_valid_for("pub-1", "occ-1", "profile-1", Some("page-1"), "video", "caption", None, 21));
    assert!(!confirmation.is_valid_for("pub-1", "other-occ", "profile-1", Some("page-1"), "video", "caption", None, 15));
    assert!(!confirmation.is_valid_for("pub-1", "occ-1", "profile-1", Some("other-page"), "video", "caption", None, 15));
    assert!(!confirmation.is_valid_for("pub-1", "occ-1", "profile-1", Some("page-1"), "video", "caption", Some("https://example.test"), 15));
  }

  #[test]
  fn persistence_models_do_not_contain_transport_secrets() {
    let encoded = serde_json::to_string(&binding()).unwrap();
    assert!(!encoded.to_ascii_lowercase().contains("bearer"));
    assert!(!encoded.to_ascii_lowercase().contains("token="));
    assert!(!encoded.contains("ws://"));
    assert!(!encoded.contains("wss://"));
  }
}
