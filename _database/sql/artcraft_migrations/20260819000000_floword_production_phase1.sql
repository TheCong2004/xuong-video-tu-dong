-- Floword Studio Phase 1 Production Migration

-- Add authoritative fields to pipeline_jobs
ALTER TABLE pipeline_jobs ADD COLUMN page_snapshot TEXT;
ALTER TABLE pipeline_jobs ADD COLUMN business_status TEXT;
ALTER TABLE pipeline_jobs ADD COLUMN started_at INTEGER DEFAULT NULL;
ALTER TABLE pipeline_jobs ADD COLUMN failure_code TEXT;
ALTER TABLE pipeline_jobs ADD COLUMN failure_stage TEXT;

-- Add worker pool to content_pages
ALTER TABLE content_pages ADD COLUMN worker_pool_id TEXT;

-- Create pipeline_job_events table for durable audit history
CREATE TABLE IF NOT EXISTS pipeline_job_events (
    id TEXT NOT NULL PRIMARY KEY,
    job_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    stage_id TEXT,
    business_status TEXT,
    event_type TEXT NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    error_code TEXT,
    metadata_json TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch('now'))
);

CREATE INDEX IF NOT EXISTS idx_pipeline_job_events_on_job_id ON pipeline_job_events(job_id, sequence);

-- Create app_settings table for persisted runtime settings (e.g. floword.max_concurrent_jobs)
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT NOT NULL PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch('now'))
);

-- Seed default max concurrency to 3
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('floword.max_concurrent_jobs', '3');
