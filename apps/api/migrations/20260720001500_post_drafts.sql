DO $$
BEGIN
    CREATE TYPE post_draft_status AS ENUM (
        'draft',
        'in_review',
        'approved',
        'scheduled',
        'published',
        'archived'
    );
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

CREATE TABLE post_drafts (
    id BIGSERIAL PRIMARY KEY,
    source_item_id BIGINT REFERENCES ingested_items (id) ON DELETE SET NULL,
    caption_en TEXT NOT NULL DEFAULT '',
    caption_zh TEXT NOT NULL DEFAULT '',
    status post_draft_status NOT NULL DEFAULT 'draft',
    rendered_post_asset_ref TEXT,
    rendered_reel_asset_ref TEXT,
    created_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    updated_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT post_drafts_rendered_post_asset_ref_not_blank CHECK (
        rendered_post_asset_ref IS NULL OR length(trim(rendered_post_asset_ref)) > 0
    ),
    CONSTRAINT post_drafts_rendered_reel_asset_ref_not_blank CHECK (
        rendered_reel_asset_ref IS NULL OR length(trim(rendered_reel_asset_ref)) > 0
    ),
    CONSTRAINT post_drafts_source_item_id_positive CHECK (
        source_item_id IS NULL OR source_item_id > 0
    )
);

CREATE INDEX idx_post_drafts_source_item_id ON post_drafts (source_item_id);
CREATE INDEX idx_post_drafts_status ON post_drafts (status);
CREATE INDEX idx_post_drafts_updated_at ON post_drafts (updated_at DESC);

CREATE TRIGGER post_drafts_set_updated_at
BEFORE UPDATE ON post_drafts
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();
