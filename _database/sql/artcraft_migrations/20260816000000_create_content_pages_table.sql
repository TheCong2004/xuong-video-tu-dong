-- ContentPage domain migration for multi-page production in Floword Studio

CREATE TABLE IF NOT EXISTS content_pages (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    output_root TEXT NOT NULL,
    target_platform TEXT,
    default_model_id TEXT,
    default_workflow_id TEXT,
    default_language TEXT,
    default_tone TEXT,
    default_aspect_ratio TEXT,
    browser_profile_id TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch('now')),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch('now'))
);

CREATE INDEX IF NOT EXISTS idx_content_pages_on_is_archived ON content_pages(is_archived);
CREATE INDEX IF NOT EXISTS idx_content_pages_on_slug ON content_pages(slug);

-- Add page_id ownership column to pipeline_jobs
ALTER TABLE pipeline_jobs ADD COLUMN page_id TEXT;
CREATE INDEX IF NOT EXISTS idx_pipeline_jobs_on_page_id ON pipeline_jobs(page_id);
