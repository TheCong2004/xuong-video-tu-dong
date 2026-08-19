use enums::tauri::pipeline::pipeline_stage::PipelineStage;
use enums::tauri::tasks::task_status::TaskStatus;
use log::{error, info};
use serde::{Deserialize, Serialize};
use sqlite_tasks::connection::TaskDbConnection;
use sqlite_tasks::queries::content_pages::list_content_pages::{list_content_pages, ListContentPagesArgs};
use sqlite_tasks::queries::pipeline::create_pipeline_job::{create_pipeline_job, CreatePipelineJobArgs};
use sqlite_tasks::queries::pipeline_job_events::insert_pipeline_job_event::{insert_pipeline_job_event, InsertPipelineJobEventArgs};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkImportRow {
  pub row_index: usize,
  pub page_id: String,
  pub source_image: Option<String>,
  pub image_prompt: Option<String>,
  pub expand_916_prompt: Option<String>,
  pub video_prompt: Option<String>,
  pub title: Option<String>,
  pub caption: Option<String>,
  pub hashtags: Vec<String>,
  pub platforms: Vec<String>,
  pub post_mode: Option<String>,
  pub post_time: Option<String>,
  pub output_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkValidationError {
  pub row_index: usize,
  pub field: String,
  pub code: String,
  pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkValidationSummary {
  pub total_rows: usize,
  pub valid_count: usize,
  pub invalid_count: usize,
  pub valid_rows: Vec<BulkImportRow>,
  pub errors: Vec<BulkValidationError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkCommitResponse {
  pub batch_id: String,
  pub total_created: usize,
  pub created_job_ids: Vec<String>,
}

pub struct BulkImportService;

impl BulkImportService {
  /// Parses raw CSV string or file content into raw string matrix.
  pub fn parse_csv_content(content: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in content.lines() {
      let trimmed = line.trim();
      if trimmed.is_empty() || trimmed.starts_with('#') {
        continue;
      }

      let mut fields = Vec::new();
      let mut current = String::new();
      let mut in_quotes = false;
      let chars: Vec<char> = trimmed.chars().collect();
      let mut i = 0;

      while i < chars.len() {
        let c = chars[i];
        if c == '"' {
          if in_quotes && i + 1 < chars.len() && chars[i + 1] == '"' {
            current.push('"');
            i += 1;
          } else {
            in_quotes = !in_quotes;
          }
        } else if (c == ',' || c == '\t' || c == ';') && !in_quotes {
          fields.push(current.trim().to_string());
          current = String::new();
        } else {
          current.push(c);
        }
        i += 1;
      }
      fields.push(current.trim().to_string());
      if !fields.is_empty() {
        rows.push(fields);
      }
    }
    rows
  }

  /// Parses rows according to column header mapping.
  pub fn map_raw_rows_to_import_rows(matrix: &[Vec<String>]) -> Vec<BulkImportRow> {
    if matrix.is_empty() {
      return Vec::new();
    }

    let header = &matrix[0];
    let header_lower: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();

    // Map column names
    let find_col = |names: &[&str]| -> Option<usize> {
      for name in names {
        if let Some(pos) = header_lower.iter().position(|h| h == *name || h.contains(name)) {
          return Some(pos);
        }
      }
      None
    };

    let col_page = find_col(&["page", "page_id", "pageid", "page name"]);
    let col_source = find_col(&["source_image", "sourceimage", "image_source", "source", "image_url", "image_path"]);
    let col_img_prompt = find_col(&["image_prompt", "imageprompt", "prompt_image", "prompt"]);
    let col_exp_prompt = find_col(&["expand916prompt", "expand_prompt", "expand", "9:16", "expand_916_prompt"]);
    let col_vid_prompt = find_col(&["video_prompt", "videoprompt", "prompt_video", "motion_prompt"]);
    let col_title = find_col(&["title", "headline", "post_title"]);
    let col_caption = find_col(&["caption", "description", "content"]);
    let col_hashtags = find_col(&["hashtags", "tags", "hashtag"]);
    let col_platforms = find_col(&["platforms", "platform", "social"]);
    let col_post_mode = find_col(&["postmode", "post_mode", "mode"]);
    let col_post_time = find_col(&["posttime", "post_time", "slot", "schedule"]);
    let col_output = find_col(&["output_override", "output", "destination"]);

    let mut import_rows = Vec::new();

    for (idx, row) in matrix.iter().skip(1).enumerate() {
      let get_val = |opt_col: Option<usize>| -> Option<String> {
        if let Some(col) = opt_col {
          if let Some(v) = row.get(col) {
            let s = v.trim().to_string();
            if !s.is_empty() {
              return Some(s);
            }
          }
        }
        None
      };

      let page_id = get_val(col_page).unwrap_or_default();
      let source_image = get_val(col_source);
      let image_prompt = get_val(col_img_prompt);
      let expand_916_prompt = get_val(col_exp_prompt);
      let video_prompt = get_val(col_vid_prompt);
      let title = get_val(col_title);
      let caption = get_val(col_caption);

      let hashtags = get_val(col_hashtags)
        .map(|s| {
          s.split(&[',', ' ', ';'][..])
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
        })
        .unwrap_or_default();

      let platforms = get_val(col_platforms)
        .map(|s| {
          s.split(&[',', ';', '|'][..])
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
        })
        .unwrap_or_default();

      let post_mode = get_val(col_post_mode);
      let post_time = get_val(col_post_time);
      let output_override = get_val(col_output);

      import_rows.push(BulkImportRow {
        row_index: idx + 2, // 1-indexed including header
        page_id,
        source_image,
        image_prompt,
        expand_916_prompt,
        video_prompt,
        title,
        caption,
        hashtags,
        platforms,
        post_mode,
        post_time,
        output_override,
      });
    }

    import_rows
  }

  /// Validates a batch of BulkImportRows against real SQLite Pages and schema rules.
  pub async fn validate_rows(
    db: &TaskDbConnection,
    rows: Vec<BulkImportRow>,
  ) -> BulkValidationSummary {
    let page_list = list_content_pages(ListContentPagesArgs {
      db,
      include_archived: false,
    })
    .await
    .unwrap_or_else(|_| sqlite_tasks::queries::content_pages::list_content_pages::ContentPageList { pages: Vec::new() });

    let valid_page_ids: HashSet<String> = page_list.pages.iter().map(|p| p.id.to_string()).collect();

    let mut valid_rows = Vec::new();
    let mut errors = Vec::new();

    for row in rows {
      let mut row_errors = Vec::new();

      // 1. Validate Page ID
      if row.page_id.trim().is_empty() {
        row_errors.push(BulkValidationError {
          row_index: row.row_index,
          field: "page_id".to_string(),
          code: "REQUIRED_FIELD".to_string(),
          message: "Page ID không được để trống".to_string(),
        });
      } else if !valid_page_ids.contains(&row.page_id) {
        row_errors.push(BulkValidationError {
          row_index: row.row_index,
          field: "page_id".to_string(),
          code: "PAGE_NOT_FOUND".to_string(),
          message: format!("Page ID '{}' không tồn tại trong hệ thống", row.page_id),
        });
      }

      // 2. Validate Prompts or Source
      let has_image_prompt = row.image_prompt.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);
      let has_video_prompt = row.video_prompt.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);
      let has_source_image = row.source_image.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);

      if !has_image_prompt && !has_video_prompt && !has_source_image {
        row_errors.push(BulkValidationError {
          row_index: row.row_index,
          field: "prompt".to_string(),
          code: "MISSING_INPUT".to_string(),
          message: "Phải cung cấp ít nhất Image Prompt, Video Prompt hoặc Source Image".to_string(),
        });
      }

      // 3. Validate Platforms
      for p in &row.platforms {
        let p_clean = p.to_lowercase();
        if p_clean != "facebook" && p_clean != "tiktok" && p_clean != "youtube" {
          row_errors.push(BulkValidationError {
            row_index: row.row_index,
            field: "platforms".to_string(),
            code: "INVALID_PLATFORM".to_string(),
            message: format!("Platform '{}' không được hỗ trợ (chỉ hỗ trợ facebook, tiktok, youtube)", p),
          });
        }
      }

      // 4. Validate Post Mode
      if let Some(ref mode) = row.post_mode {
        let mode_clean = mode.to_lowercase();
        if mode_clean != "auto" && mode_clean != "review" {
          row_errors.push(BulkValidationError {
            row_index: row.row_index,
            field: "post_mode".to_string(),
            code: "INVALID_POST_MODE".to_string(),
            message: format!("Post mode '{}' không hợp lệ (chỉ chấp nhận 'auto' hoặc 'review')", mode),
          });
        }
      }

      if row_errors.is_empty() {
        valid_rows.push(row);
      } else {
        errors.extend(row_errors);
      }
    }

    let valid_count = valid_rows.len();
    let invalid_count = errors.iter().map(|e| e.row_index).collect::<HashSet<_>>().len();
    let total_rows = valid_count + invalid_count;

    BulkValidationSummary {
      total_rows,
      valid_count,
      invalid_count,
      valid_rows,
      errors,
    }
  }

  /// Commits valid rows into `pipeline_jobs` table in chunked transactions.
  pub async fn commit_import(
    db: &TaskDbConnection,
    rows: Vec<BulkImportRow>,
  ) -> Result<BulkCommitResponse, String> {
    let batch_id = format!("batch_{}", uuid::Uuid::new_v4().simple());
    info!("[BulkImport] Committing batch_id={} with {} valid rows", batch_id, rows.len());

    let mut created_job_ids = Vec::with_capacity(rows.len());

    // Chunk in batches of 25 to prevent lock storm
    for chunk in rows.chunks(25) {
      for row in chunk {
        let payload = serde_json::json!({
          "workflow_mode": "grok_content_pipeline",
          "batch_id": batch_id,
          "row_index": row.row_index,
          "source_image": row.source_image,
          "image_prompt": row.image_prompt,
          "expand_916_prompt": row.expand_916_prompt,
          "video_prompt": row.video_prompt,
          "title": row.title,
          "caption": row.caption,
          "hashtags": row.hashtags,
          "platforms": row.platforms,
          "post_mode": row.post_mode,
          "post_time": row.post_time,
          "output_override": row.output_override,
        });

        let payload_str = payload.to_string();

        let args = CreatePipelineJobArgs {
          db,
          status: TaskStatus::Pending,
          current_stage: PipelineStage::Queued,
          maybe_page_id: Some(&row.page_id),
          maybe_input_payload: Some(&payload_str),
          maybe_page_snapshot: None,
          maybe_business_status: Some("QUEUED"),
        };

        match create_pipeline_job(args).await {
          Ok(pipeline_job_id) => {
            let job_id_str = pipeline_job_id.as_str().to_string();
            let meta = serde_json::json!({
              "batch_id": batch_id,
              "row_index": row.row_index,
              "page_id": row.page_id
            }).to_string();

            let _ = insert_pipeline_job_event(
              InsertPipelineJobEventArgs {
                db,
                id: None,
                job_id: &job_id_str,
                sequence: 1,
                stage_id: Some("INGEST_SOURCE_IMAGE"),
                business_status: Some("QUEUED"),
                event_type: "JOB_ENQUEUED_BULK",
                level: "INFO",
                message: "Job được khởi tạo từ Bulk Import",
                error_code: None,
                metadata_json: Some(&meta),
              },
            ).await;

            created_job_ids.push(job_id_str);
          }
          Err(err) => {
            error!("[BulkImport] Failed to insert job for row {}: {err}", row.row_index);
          }
        }
      }
    }

    Ok(BulkCommitResponse {
      batch_id,
      total_created: created_job_ids.len(),
      created_job_ids,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_csv_parser_comma_and_quotes() {
    let csv = r#"page_id,image_prompt,video_prompt,platforms
page_demo,"Cyberpunk girl, neon city",Zoom in fast,"facebook,tiktok"
page_2,Simple prompt,Simple video,youtube
"#;
    let matrix = BulkImportService::parse_csv_content(csv);
    assert_eq!(matrix.len(), 3);
    assert_eq!(matrix[0], vec!["page_id", "image_prompt", "video_prompt", "platforms"]);
    assert_eq!(matrix[1][0], "page_demo");
    assert_eq!(matrix[1][1], "Cyberpunk girl, neon city");
    assert_eq!(matrix[1][3], "facebook,tiktok");
  }

  #[test]
  fn test_csv_mapping_to_import_rows() {
    let csv = r#"page,image_prompt,video_prompt,platforms,post_mode
page_1,Hero image,Hero action,"facebook|tiktok",auto
"#;
    let matrix = BulkImportService::parse_csv_content(csv);
    let rows = BulkImportService::map_raw_rows_to_import_rows(&matrix);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].row_index, 2);
    assert_eq!(rows[0].page_id, "page_1");
    assert_eq!(rows[0].image_prompt, Some("Hero image".to_string()));
    assert_eq!(rows[0].video_prompt, Some("Hero action".to_string()));
    assert_eq!(rows[0].platforms, vec!["facebook", "tiktok"]);
    assert_eq!(rows[0].post_mode, Some("auto".to_string()));
  }
}

