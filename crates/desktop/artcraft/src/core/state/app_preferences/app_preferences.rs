use crate::core::state::app_preferences::app_preferences_serializable::AppPreferencesSerializable;
use crate::core::state::app_preferences::preferred_download_directory::{PreferredDownloadDirectory, SystemDownloadDirectory};
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use serde_derive::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalRole {
  Admin,
  User,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalEntitlements {
  pub role: LocalRole,
  pub unlimited_local_usage: bool,
}

impl LocalEntitlements {
  pub fn for_role(role: LocalRole) -> Self {
    Self { role, unlimited_local_usage: role == LocalRole::Admin }
  }

  /// Floword has no local credit quota. External providers retain their own
  /// authentication, billing, and quota enforcement.
  pub fn allows_floword_run(self) -> bool {
    true
  }
}

#[derive(Clone)]
pub struct AppPreferences {
  /// Explicit local application role. This never changes provider billing.
  pub local_role: LocalRole,
  /// The downloads directory to use when a user downloads a file.
  pub preferred_download_directory: PreferredDownloadDirectory,

  /// Play sounds on events.
  pub play_sounds: bool,

  /// Key pointing to file; defined in the frontend code.
  pub delete_file_sound: Option<String>,

  /// Key pointing to file; defined in the frontend code.
  /// Defined for enqueue since image enqueue can be async
  pub enqueue_success_sound: Option<String>,

  /// Key pointing to file; defined in the frontend code.
  /// Defined for enqueue since image enqueue can be async
  pub enqueue_failure_sound: Option<String>,

  /// Key pointing to file; defined in the frontend code.
  pub generation_success_sound: Option<String>,

  /// Key pointing to file; defined in the frontend code.
  pub generation_failure_sound: Option<String>,

  /// Key pointing to file; defined in the frontend code.
  #[deprecated]
  pub generation_enqueue_sound: Option<String>,
}

impl Default for AppPreferences {
  fn default() -> Self {
    Self {
      local_role: LocalRole::Admin,
      preferred_download_directory: PreferredDownloadDirectory::System(SystemDownloadDirectory::Downloads),
      play_sounds: true,
      // NB: These are defined in the frontend.
      enqueue_success_sound: Some("done".to_string()),
      enqueue_failure_sound: Some("spike_throw".to_string()),
      generation_success_sound: Some("special_flower".to_string()),
      generation_failure_sound: Some("crumble".to_string()),
      generation_enqueue_sound: Some("done".to_string()),
      delete_file_sound: Some("trash".to_string()),
    }
  }
}

impl AppPreferences {
  pub fn local_entitlements(&self) -> LocalEntitlements {
    LocalEntitlements::for_role(self.local_role)
  }
  pub fn load_from_file_or_default(data_root: &AppDataRoot) -> Self {
    let filename = data_root.settings_dir().get_app_preferences_path();
    if !filename.exists() {
      return Self::default();
    }

    match AppPreferencesSerializable::load_from_file(data_root) {
      Ok(Some(serializable)) => serializable.to_preferences(),
      Ok(None) => Self::default(),
      Err(err) => {
        println!("Error loading app preferences: {}", err);
        Self::default()
      },
    }
  }

  pub fn to_serializable(&self) -> AppPreferencesSerializable {
    AppPreferencesSerializable::from_preferences(self)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn admin_has_explicit_unlimited_local_usage_without_fake_credits() {
    let entitlements = LocalEntitlements::for_role(LocalRole::Admin);
    assert!(entitlements.unlimited_local_usage);
    assert!(entitlements.allows_floword_run());
  }

  #[test]
  fn ordinary_user_is_not_marked_unlimited_but_floword_has_no_local_credit_gate() {
    let entitlements = LocalEntitlements::for_role(LocalRole::User);
    assert!(!entitlements.unlimited_local_usage);
    assert!(entitlements.allows_floword_run());
  }
}
