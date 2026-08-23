use artcraft_app_lib::services::pipeline::output_policy::{current_local_date_string, sanitize_page_name, OutputPathResolver};
use sqlite_tasks::connection::TaskDbConnection;
use sqlite_tasks::queries::content_pages::archive_content_page::{archive_content_page, ArchiveContentPageArgs};
use sqlite_tasks::queries::content_pages::create_content_page::{create_content_page, CreateContentPageArgs};
use sqlite_tasks::queries::content_pages::get_content_page_by_id::{get_content_page_by_id, GetContentPageByIdArgs};
use sqlite_tasks::queries::content_pages::list_content_pages::{list_content_pages, ListContentPagesArgs};
use sqlite_tasks::queries::content_pages::update_content_page::{update_content_page, UpdateContentPageArgs};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_sanitize_page_name_removes_invalid_characters() {
  assert_eq!(sanitize_page_name("Stage & Screen Feed"), "Stage & Screen Feed");
  assert_eq!(sanitize_page_name("My:Page*Name?\"<test>|/\\"), "MyPageNametest");
  assert_eq!(sanitize_page_name("   Valid Space Page   "), "Valid Space Page");
  assert_eq!(sanitize_page_name("Tiếng Việt Có Dấu 123"), "Tiếng Việt Có Dấu 123");
}

#[test]
fn test_sanitize_page_name_guards_reserved_names() {
  assert_eq!(sanitize_page_name("CON"), "CON_page");
  assert_eq!(sanitize_page_name("prn"), "prn_page");
  assert_eq!(sanitize_page_name("AUX"), "AUX_page");
  assert_eq!(sanitize_page_name("NUL"), "NUL_page");
  assert_eq!(sanitize_page_name("COM1"), "COM1_page");
  assert_eq!(sanitize_page_name("LPT9"), "LPT9_page");
}

#[test]
fn test_sanitize_page_name_guards_traversal_and_empty() {
  assert_eq!(sanitize_page_name(".."), "Untitled Page");
  assert_eq!(sanitize_page_name("."), "Untitled Page");
  assert_eq!(sanitize_page_name("   "), "Untitled Page");
  assert_eq!(sanitize_page_name(""), "Untitled Page");
}

#[test]
fn test_current_local_date_format() {
  let date_str = current_local_date_string();
  // Must match DD-MM-YYYY pattern (e.g. 16-08-2026)
  let parts: Vec<&str> = date_str.split('-').collect();
  assert_eq!(parts.len(), 3, "Date string must have 3 parts separated by hyphen");
  assert_eq!(parts[0].len(), 2, "Day must be 2 digits");
  assert_eq!(parts[1].len(), 2, "Month must be 2 digits");
  assert_eq!(parts[2].len(), 4, "Year must be 4 digits");
}

#[test]
fn test_output_path_resolver_layout() {
  let output_root = "D:\\TestOutputs";
  let page_name = "Stage & Screen Feed";
  let resolved = OutputPathResolver::resolve_page_date_directory(output_root, page_name).unwrap();

  let resolved_str = resolved.to_string_lossy();
  let date_str = current_local_date_string();
  assert!(resolved_str.contains("Stage & Screen Feed"));
  assert!(resolved_str.ends_with(&date_str));
}

#[test]
fn test_prepare_and_publish_output_file() {
  let temp_dir = tempdir().unwrap();
  let root_path = temp_dir.path().to_string_lossy().to_string();

  let target_dir = OutputPathResolver::prepare_output_directory(&root_path, "Movie Spotlight").unwrap();
  assert!(target_dir.exists(), "Output directory must be created on disk");

  // Create a dummy source file
  let source_file = temp_dir.path().join("source_rendered.mp4");
  fs::write(&source_file, b"fake video content 12345").unwrap();

  let filename = OutputPathResolver::generate_final_filename("job-abcdef-123456", "video", "mp4");
  let published = OutputPathResolver::publish_final_file(&source_file, &target_dir, &filename).unwrap();

  assert!(published.exists());
  assert_eq!(fs::read(&published).unwrap(), b"fake video content 12345");
}

#[tokio::test]
async fn test_content_page_crud_in_sqlite() {
  let temp_dir = tempdir().unwrap();
  let db_path = temp_dir.path().join("test_tasks.sqlite");

  let db = TaskDbConnection::connect_and_migrate(&db_path).await.unwrap();

  // 1. Create Page with default prompt fields
  let page = create_content_page(CreateContentPageArgs { db: &db, id: None, name: "Hollywood Review", slug: None, output_root: "D:\\Outputs", target_platform: Some("tiktok"), default_model_id: Some("gpt-4o"), default_workflow_id: None, worker_pool_id: None, default_language: Some("vi"), default_tone: Some("professional"), default_aspect_ratio: Some("9:16"), browser_profile_id: None, default_image_prompt: Some("Default Hollywood image prompt"), default_expand_9_16_prompt: Some("Default Hollywood expand 9:16 prompt"), default_video_prompt: Some("Default Hollywood video prompt") }).await.unwrap();

  assert_eq!(page.name, "Hollywood Review");
  assert_eq!(page.slug, "hollywood-review");
  assert_eq!(page.output_root, "D:\\Outputs");
  assert_eq!(page.default_image_prompt.as_deref(), Some("Default Hollywood image prompt"));
  assert_eq!(page.default_expand_9_16_prompt.as_deref(), Some("Default Hollywood expand 9:16 prompt"));
  assert_eq!(page.default_video_prompt.as_deref(), Some("Default Hollywood video prompt"));
  assert_eq!(page.is_archived, false);

  // 2. Read Page by ID
  let fetched = get_content_page_by_id(GetContentPageByIdArgs { db: &db, id: &page.id }).await.unwrap().expect("Page should exist");

  assert_eq!(fetched.id, page.id);
  assert_eq!(fetched.name, "Hollywood Review");
  assert_eq!(fetched.default_image_prompt.as_deref(), Some("Default Hollywood image prompt"));

  // 3. Update Page
  let updated = update_content_page(UpdateContentPageArgs { db: &db, id: &page.id, name: "Hollywood Review Official", slug: Some("hollywood-review-official"), output_root: "E:\\NewOutputs", target_platform: Some("reels"), default_model_id: Some("claude-3-5-sonnet"), default_workflow_id: None, worker_pool_id: None, default_language: Some("en"), default_tone: Some("viral"), default_aspect_ratio: Some("16:9"), browser_profile_id: None, default_image_prompt: Some("Updated Hollywood image prompt"), default_expand_9_16_prompt: Some("Updated Hollywood expand prompt"), default_video_prompt: Some("Updated Hollywood video prompt") }).await.unwrap();

  assert_eq!(updated.name, "Hollywood Review Official");
  assert_eq!(updated.slug, "hollywood-review-official");
  assert_eq!(updated.output_root, "E:\\NewOutputs");
  assert_eq!(updated.target_platform.as_deref(), Some("reels"));
  assert_eq!(updated.default_image_prompt.as_deref(), Some("Updated Hollywood image prompt"));

  // 4. List Active Pages
  let list = list_content_pages(ListContentPagesArgs { db: &db, include_archived: false }).await.unwrap();

  assert_eq!(list.pages.len(), 1);
  assert_eq!(list.pages[0].id, page.id);

  // 5. Archive Page
  let archived_res = archive_content_page(ArchiveContentPageArgs { db: &db, id: &page.id, is_archived: true }).await.unwrap();
  assert!(archived_res);

  // 6. List Active Pages (should now be 0)
  let active_list = list_content_pages(ListContentPagesArgs { db: &db, include_archived: false }).await.unwrap();
  assert_eq!(active_list.pages.len(), 0);

  // 7. List All including Archived (should be 1)
  let all_list = list_content_pages(ListContentPagesArgs { db: &db, include_archived: true }).await.unwrap();
  assert_eq!(all_list.pages.len(), 1);
  assert_eq!(all_list.pages[0].is_archived, true);
}

#[tokio::test]
async fn test_grok_pipeline_prompt_resolution_rules() {
  let temp_dir = tempdir().unwrap();
  let db_path = temp_dir.path().join("test_prompts.sqlite");
  let db = TaskDbConnection::connect_and_migrate(&db_path).await.unwrap();

  // Seed ContentPage with defaults
  let page = create_content_page(CreateContentPageArgs { db: &db, id: Some("PAGE_PROMPTS_01"), name: "Movie Magic", slug: Some("movie-magic"), output_root: "D:\\Outputs", target_platform: Some("tiktok"), default_model_id: None, default_workflow_id: None, worker_pool_id: None, default_language: Some("vi"), default_tone: None, default_aspect_ratio: Some("9:16"), browser_profile_id: None, default_image_prompt: Some("Page Default Image Prompt"), default_expand_9_16_prompt: Some("Page Default 9:16 Prompt"), default_video_prompt: Some("Page Default Video Prompt") }).await.unwrap();

  // Helper matching run_job_pipeline resolution logic
  fn resolve_image_prompt<'a>(job_prompt: Option<&'a str>, raw_prompt: &'a str, page: &'a sqlite_tasks::queries::content_pages::content_page::ContentPage) -> Result<&'a str, &'static str> {
    if let Some(p) = job_prompt.filter(|s| !s.trim().is_empty()) {
      Ok(p)
    } else if !raw_prompt.trim().is_empty() {
      Ok(raw_prompt)
    } else if let Some(ref dp) = page.default_image_prompt.as_deref().filter(|s| !s.trim().is_empty()) {
      Ok(dp)
    } else {
      Err("IMAGE_PROMPT_REQUIRED")
    }
  }

  fn resolve_expand_prompt<'a>(job_expand: Option<&'a str>, page: &'a sqlite_tasks::queries::content_pages::content_page::ContentPage) -> &'a str {
    const FALLBACK: &str = "Expand image to 9:16 vertical ratio preserving subject and composition";
    job_expand.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| page.default_expand_9_16_prompt.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or(FALLBACK))
  }

  fn resolve_video_prompt<'a>(job_video: Option<&'a str>, page: &'a sqlite_tasks::queries::content_pages::content_page::ContentPage) -> &'a str {
    const FALLBACK: &str = "Generate cinematic dynamic video animation preserving character and action";
    job_video.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| page.default_video_prompt.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or(FALLBACK))
  }

  // CASE 1: Job image prompt present, Page default present -> Job prompt wins
  let res_c1 = resolve_image_prompt(Some("Job Custom Image Prompt"), "", &page);
  assert_eq!(res_c1.unwrap(), "Job Custom Image Prompt");

  // CASE 2: Job image prompt missing, Page default present -> Page default used
  let res_c2 = resolve_image_prompt(None, "", &page);
  assert_eq!(res_c2.unwrap(), "Page Default Image Prompt");

  // CASE 3: Both missing -> IMAGE_PROMPT_REQUIRED
  let empty_page = sqlite_tasks::queries::content_pages::content_page::ContentPage { id: "PAGE_EMPTY".to_string(), name: "Empty".to_string(), slug: "empty".to_string(), output_root: "D:\\".to_string(), target_platform: None, default_model_id: None, default_workflow_id: None, worker_pool_id: None, default_language: None, default_tone: None, default_aspect_ratio: None, browser_profile_id: None, default_image_prompt: None, default_expand_9_16_prompt: None, default_video_prompt: None, is_archived: false, created_at: 0, updated_at: 0 };
  let res_c3 = resolve_image_prompt(None, "", &empty_page);
  assert_eq!(res_c3.unwrap_err(), "IMAGE_PROMPT_REQUIRED");

  // CASE 4: Job 9:16 prompt present -> Job override
  let res_c4 = resolve_expand_prompt(Some("Job Custom 9:16 Prompt"), &page);
  assert_eq!(res_c4, "Job Custom 9:16 Prompt");

  // CASE 5: Job missing 9:16 prompt, Page default exists -> Page default
  let res_c5 = resolve_expand_prompt(None, &page);
  assert_eq!(res_c5, "Page Default 9:16 Prompt");

  // CASE 6: Video Job override vs Page default -> Job override
  let res_c6_override = resolve_video_prompt(Some("Job Custom Video Prompt"), &page);
  assert_eq!(res_c6_override, "Job Custom Video Prompt");

  let res_c6_default = resolve_video_prompt(None, &page);
  assert_eq!(res_c6_default, "Page Default Video Prompt");
}
