DO $$
BEGIN
    CREATE TYPE backup_content_kind AS ENUM ('past_recap', 'did_you_know');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

CREATE TABLE backup_content_items (
    id BIGSERIAL PRIMARY KEY,
    kind backup_content_kind NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    source_url TEXT,
    media_ref TEXT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    updated_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT backup_content_items_title_not_blank CHECK (length(trim(title)) > 0),
    CONSTRAINT backup_content_items_body_not_blank CHECK (length(trim(body)) > 0),
    CONSTRAINT backup_content_items_source_url_not_blank CHECK (
        source_url IS NULL OR length(trim(source_url)) > 0
    ),
    CONSTRAINT backup_content_items_media_ref_not_blank CHECK (
        media_ref IS NULL OR length(trim(media_ref)) > 0
    )
);

CREATE INDEX idx_backup_content_items_kind ON backup_content_items (kind);
CREATE INDEX idx_backup_content_items_active ON backup_content_items (active);
CREATE INDEX idx_backup_content_items_updated_at ON backup_content_items (updated_at DESC);

CREATE TRIGGER backup_content_items_set_updated_at
BEFORE UPDATE ON backup_content_items
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();
