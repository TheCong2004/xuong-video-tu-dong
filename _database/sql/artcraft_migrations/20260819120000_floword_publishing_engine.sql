-- Floword Publishing Engine Migration

-- 1. Table: content_page_publish_targets
CREATE TABLE IF NOT EXISTS content_page_publish_targets (
    id TEXT NOT NULL PRIMARY KEY,
    page_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    account_label TEXT,
    destination_id TEXT NOT NULL,
    destination_handle TEXT,
    browser_profile_id TEXT NOT NULL,
    post_mode TEXT NOT NULL DEFAULT 'review',
    default_slots_json TEXT NOT NULL DEFAULT '["08:30", "10:00", "17:00", "22:00"]',
    created_at INTEGER NOT NULL DEFAULT (unixepoch('now')),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch('now')),
    UNIQUE(page_id, platform, destination_id)
);

CREATE INDEX IF NOT EXISTS idx_publish_targets_on_page_id ON content_page_publish_targets(page_id);

-- 2. Table: job_publications
CREATE TABLE IF NOT EXISTS job_publications (
    id TEXT NOT NULL PRIMARY KEY,
    job_id TEXT NOT NULL,
    page_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    target_config_id TEXT,
    browser_profile_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'WAITING_APPROVAL',
    scheduled_at INTEGER DEFAULT NULL,
    approved_at INTEGER DEFAULT NULL,
    started_at INTEGER DEFAULT NULL,
    posted_at INTEGER DEFAULT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    idempotency_key TEXT NOT NULL UNIQUE,
    platform_post_id TEXT,
    post_url TEXT,
    title TEXT,
    caption TEXT,
    hashtags_json TEXT,
    description TEXT,
    video_path TEXT,
    last_error_code TEXT,
    last_error_message TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch('now')),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch('now'))
);

CREATE INDEX IF NOT EXISTS idx_job_publications_on_job_id ON job_publications(job_id);
CREATE INDEX IF NOT EXISTS idx_job_publications_on_status ON job_publications(status);
CREATE INDEX IF NOT EXISTS idx_job_publications_on_page_id ON job_publications(page_id);
