-- Floword Scale + Operations Migration

-- 1. Prompt Templates Table
CREATE TABLE IF NOT EXISTS floword_prompt_templates (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL UNIQUE,
  image_prompt TEXT NOT NULL,
  expand_prompt TEXT,
  video_prompt TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch('now')),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch('now'))
);

-- 2. System Settings Table (Key-Value JSON store for concurrency, notifications, storage config)
CREATE TABLE IF NOT EXISTS floword_system_settings (
  key TEXT PRIMARY KEY NOT NULL,
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL DEFAULT (unixepoch('now'))
);

-- Insert default system settings if not present
INSERT OR IGNORE INTO floword_system_settings (key, value_json) VALUES
('concurrency', '{"global_concurrency": 10, "grok_concurrency": 5}'),
('notifications', '{"enabled": true, "on_job_failed": true, "on_auth_required": true, "on_post_failed": true, "on_batch_completed": true}');

-- 3. Optimization Indexes for Scale Operations
CREATE INDEX IF NOT EXISTS idx_pipeline_jobs_page_status_created ON pipeline_jobs (page_id, status, created_at);
CREATE INDEX IF NOT EXISTS idx_pipeline_jobs_created_status ON pipeline_jobs (created_at DESC, status);
CREATE INDEX IF NOT EXISTS idx_job_publications_page_platform_status ON job_publications (page_id, platform, status);
CREATE INDEX IF NOT EXISTS idx_job_publications_scheduled_status ON job_publications (status, scheduled_at);
