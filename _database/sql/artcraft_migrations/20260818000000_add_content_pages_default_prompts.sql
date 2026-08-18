-- Migration: Add default prompt columns for Grok production workflows to content_pages
ALTER TABLE content_pages ADD COLUMN default_image_prompt TEXT;
ALTER TABLE content_pages ADD COLUMN default_expand_9_16_prompt TEXT;
ALTER TABLE content_pages ADD COLUMN default_video_prompt TEXT;
