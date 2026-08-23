use crate::core::lifecycle::startup::tasks::bootstrap_task_database::bootstrap_task_database;
use crate::core::lifecycle::startup::tasks::initially_size_and_position_windows::initially_size_and_position_windows;
use crate::core::lifecycle::startup::tasks::load_provider_priority_state::load_provider_priority_state;
use crate::core::lifecycle::startup::tasks::set_app_log_level::set_app_log_level;
use crate::core::lifecycle::startup::tasks::spawn_auxiliary_backends::spawn_auxiliary_backends;
use crate::core::lifecycle::startup::tasks::spawn_capcut_mate_backend::{health_ready as unified_health_ready, spawn_capcut_mate_backend};
use crate::core::lifecycle::startup::tasks::spawn_discord_presence_thread::spawn_discord_presence_thread;
use crate::core::lifecycle::startup::tasks::spawn_main_window_thread::spawn_main_window_thread;
use crate::core::lifecycle::startup::tasks::spawn_omniroute_backend::{health_ready as omniroute_health_ready, spawn_omniroute_backend};
use crate::core::lifecycle::startup::tasks::runtime_supervisor::{runtime_manifest_ready, start_playwright_runtime, start_runtime_supervisor};
use crate::core::lifecycle::startup::tasks::spawn_sora_task_polling_thread::spawn_sora_task_polling_thread;
use crate::core::lifecycle::startup::tasks::spawn_storyteller_threads::spawn_storyteller_threads;
use crate::core::providers::credentials::provider_credential_loading_cache::ProviderCredentialLoadingCache;
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use crate::core::state::artcraft_platform_info::ArtcraftPlatformInfo;
use crate::core::state::artcraft_usage_tracker::artcraft_usage_tracker::ArtcraftUsageTracker;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::core::threads::third_party_task_polling_thread::third_party_task_polling_thread::third_party_task_polling_thread;
use crate::services::pipeline::state::command_dispatcher::CommandDispatcher;
use crate::services::pipeline::threads::pipeline_worker_thread::pipeline_worker_thread;
use crate::services::grok::state::grok_credential_manager::GrokCredentialManager;
use crate::services::grok::state::grok_image_prompt_queue::GrokImagePromptQueue;
use crate::services::grok::threads::grok_image_websocket_thread::grok_image_websocket_thread::grok_image_websocket_thread;
use crate::services::grok::threads::grok_video_task_polling::grok_video_task_polling_thread::grok_video_task_polling_thread;
use crate::services::midjourney::state::midjourney_credential_manager::MidjourneyCredentialManager;
use crate::services::midjourney::threads::midjourney_long_polling_thread::midjourney_long_polling_thread;
use crate::services::sora::state::sora_credential_manager::SoraCredentialManager;
use crate::services::sora::state::sora_task_queue::SoraTaskQueue;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use crate::services::worldlabs::state::worldlabs_bearer_bridge::WorldlabsBearerBridge;
use crate::services::worldlabs::state::worldlabs_credential_manager::WorldlabsCredentialManager;
use crate::services::worldlabs::threads::worldlabs_marble_task_polling::worldlabs_marble_task_polling;
use errors::AnyhowResult;
use log::warn;
use tauri::{AppHandle, Manager};

pub async fn handle_tauri_startup(app: AppHandle, root: AppDataRoot, app_env_configs: AppEnvConfigs, artcraft_platform_info: ArtcraftPlatformInfo, artcraft_usage_tracker: ArtcraftUsageTracker, storyteller_creds_manager: StorytellerCredentialManager, sora_credential_manager: SoraCredentialManager, sora_task_queue: SoraTaskQueue, mj_creds_manager: MidjourneyCredentialManager, grok_creds_manager: GrokCredentialManager, grok_image_prompt_queue: GrokImagePromptQueue, worldlabs_bearer_bridge: WorldlabsBearerBridge, worldlabs_creds_manager: WorldlabsCredentialManager, credential_cache: ProviderCredentialLoadingCache, command_dispatcher: CommandDispatcher) -> AnyhowResult<()> {
  set_app_log_level(&app, &root)?;

  // Floword owns the headless Donut runtime. It is intentionally started in a
  // background thread so the Studio can render while runtime health is pending.
  let app_for_donut = app.clone();
  std::thread::spawn(move || {
    if runtime_manifest_ready(&app_for_donut) {
      start_playwright_runtime(&app_for_donut);
      start_runtime_supervisor(&app_for_donut);
    } else {
      warn!("Runtime manifest verification failed; Donut and Playwright runtimes were not spawned");
    }
  });

  // Python backend: capcut-mate owns :30000 (the single always-on Python port).
  // (artcraft-server.exe / spawn_unified_backend removed — it fought capcut-mate
  //  for :30000 and its 15s blocking wait was the startup black-screen cause.)
  //
  // Spawning the sidecars blocks (sidecar startup sleep) — run it off the setup
  // thread so the WebView can render immediately instead of showing a black screen.
  // Each launcher performs its own readiness wait. Run them independently so a
  // slow or failed service cannot delay the others (especially OmniRoute
  // blocking the unified backend for its full readiness deadline).
  {
    let app_for_omniroute = app.clone();
    std::thread::spawn(move || spawn_omniroute_backend(&app_for_omniroute));

    let app_for_unified = app.clone();
    std::thread::spawn(move || spawn_capcut_mate_backend(&app_for_unified));

    let app_for_auxiliary = app.clone();
    std::thread::spawn(move || {
      // The frozen Python sidecars are CPU/I/O-heavy during extraction. Starting
      // all three at once can starve OmniRoute's instrumentation hook past its
      // readiness deadline. Core services own the startup critical path;
      // auxiliary services begin only after both core identities are healthy.
      for _ in 0..240 {
        if app_for_auxiliary.webview_windows().is_empty() {
          return;
        }
        if unified_health_ready(30000) && omniroute_health_ready(20128) {
          spawn_auxiliary_backends(&app_for_auxiliary);
          return;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
      }
      warn!("Auxiliary backends skipped: core services were not ready within 240s");
    });
  }

  let task_database = bootstrap_task_database(&app, &root).await?;

  load_provider_priority_state(&app, &root)?;

  spawn_main_window_thread(&app, &root, &storyteller_creds_manager)?;

  spawn_storyteller_threads(&app, &app_env_configs, &artcraft_usage_tracker, &artcraft_platform_info, &task_database, &storyteller_creds_manager)?;

  spawn_sora_task_polling_thread(&app, &root, &app_env_configs, &task_database, &sora_credential_manager, &storyteller_creds_manager, &sora_task_queue)?;

  tauri::async_runtime::spawn(grok_video_task_polling_thread(app.clone(), app_env_configs.clone(), root.clone(), task_database.clone(), grok_creds_manager.clone(), storyteller_creds_manager.clone()));

  tauri::async_runtime::spawn(grok_image_websocket_thread(app.clone(), app_env_configs.clone(), root.clone(), task_database.clone(), grok_creds_manager.clone(), grok_image_prompt_queue.clone(), storyteller_creds_manager.clone()));

  tauri::async_runtime::spawn(midjourney_long_polling_thread(app.clone(), app_env_configs.clone(), root.clone(), task_database.clone(), mj_creds_manager.clone(), storyteller_creds_manager.clone()));

  tauri::async_runtime::spawn(worldlabs_marble_task_polling(app.clone(), app_env_configs.clone(), root.clone(), task_database.clone(), worldlabs_creds_manager.clone(), storyteller_creds_manager.clone()));

  tauri::async_runtime::spawn(third_party_task_polling_thread(app.clone(), app_env_configs.clone(), root.clone(), task_database.clone(), storyteller_creds_manager.clone(), credential_cache));

  // Pipeline orchestrator worker: drives multi-stage pipeline jobs
  // (script generation -> video assembly) gated by the CommandDispatcher.
  tauri::async_runtime::spawn(pipeline_worker_thread(app.clone(), root.clone(), task_database.clone(), command_dispatcher));

  // Publishing worker: handles automated/approved social posting (FB, TikTok, YT Shorts)
  crate::services::publishing::threads::publishing_worker_thread::PublishingWorkerThread::start(app.clone(), task_database.get_connection().clone());

  spawn_discord_presence_thread()?;

  initially_size_and_position_windows(&app, &root);

  Ok(())
}
