use crate::core::state::app_preferences::app_preferences::{AppPreferences, LocalRole};
use crate::core::state::app_preferences::preferred_download_directory::PreferredDownloadDirectory;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use errors::AnyhowResult;
use serde_derive::{Deserialize, Serialize};

/// Vector clock versioning string rather than semver.
/// - Version 1 - initial version.
/// - Version 2 - added "delete_file_sound", marked optionals "skip_serializing_if"
const CURRENT_VERSION: &str = "3";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPreferencesSerializable {
  /// Versioning string.
  pub version: String,

  /// Optional for backward compatibility with legacy profiles.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub local_role: Option<LocalRole>,

  /// The downloads directory to use when a user downloads a file.
  pub preferred_download_directory: Option<PreferredDownloadDirectory>,

  /// Play sounds on events.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub play_sounds: Option<bool>,

  /// Key pointing to file; defined in the frontend code.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub delete_file_sound: Option<String>,

  /// Key pointing to file; defined in the frontend code.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub generation_success_sound: Option<String>,

  /// Key pointing to file; defined in the frontend code.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub generation_failure_sound: Option<String>,

  /// Key pointing to file; defined in the frontend code.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub generation_enqueue_sound: Option<String>,
}

impl AppPreferencesSerializable {
  pub fn load_from_file(app_data_root: &AppDataRoot) -> AnyhowResult<Option<Self>> {
    let filename = app_data_root.settings_dir().get_app_preferences_path();
    if !filename.exists() {
      return Ok(None);
    }

    let contents = std::fs::read_to_string(filename)?;
    let data: Self = serde_json::from_str(&contents)?;
    Ok(Some(data))
  }

  pub fn from_preferences(preferences: &AppPreferences) -> Self {
    Self { version: CURRENT_VERSION.to_string(), local_role: Some(preferences.local_role), preferred_download_directory: Some(preferences.preferred_download_directory.clone()), play_sounds: Some(preferences.play_sounds), delete_file_sound: preferences.delete_file_sound.clone(), generation_success_sound: preferences.generation_success_sound.clone(), generation_failure_sound: preferences.generation_failure_sound.clone(), generation_enqueue_sound: preferences.generation_enqueue_sound.clone() }
  }

  pub fn to_preferences(&self) -> AppPreferences {
    let mut preferences = AppPreferences::default();

    if let Some(local_role) = self.local_role {
      preferences.local_role = local_role;
    }

    if let Some(preferred_download_directory) = &self.preferred_download_directory {
      preferences.preferred_download_directory = preferred_download_directory.clone();
    }

    if let Some(play_sounds) = self.play_sounds {
      preferences.play_sounds = play_sounds;
    }

    preferences.delete_file_sound = self.delete_file_sound.clone();
    preferences.generation_success_sound = self.generation_success_sound.clone();
    preferences.generation_failure_sound = self.generation_failure_sound.clone();
    preferences.generation_enqueue_sound = self.generation_enqueue_sound.clone();

    preferences
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn legacy_profile_without_local_role_loads_with_local_owner_default() {
    let legacy = r#"{"version":"2","preferred_download_directory":null,"play_sounds":true}"#;
    let serialized: AppPreferencesSerializable = serde_json::from_str(legacy).unwrap();
    assert_eq!(serialized.local_role, None);
    assert_eq!(serialized.to_preferences().local_role, LocalRole::Admin);
  }
}
