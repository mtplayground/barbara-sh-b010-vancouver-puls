DO $$
BEGIN
    CREATE TYPE content_source_kind AS ENUM ('rss', 'website', 'instagram', 'manual');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

CREATE TABLE content_sources (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    kind content_source_kind NOT NULL,
    url TEXT,
    external_id TEXT,
    created_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT content_sources_name_not_blank CHECK (length(trim(name)) > 0),
    CONSTRAINT content_sources_url_not_blank CHECK (url IS NULL OR length(trim(url)) > 0),
    CONSTRAINT content_sources_external_id_not_blank CHECK (
        external_id IS NULL OR length(trim(external_id)) > 0
    )
);

CREATE INDEX idx_content_sources_kind ON content_sources (kind);
CREATE INDEX idx_content_sources_enabled ON content_sources (enabled);
CREATE UNIQUE INDEX idx_content_sources_external_identity
    ON content_sources (kind, lower(external_id))
    WHERE external_id IS NOT NULL;

CREATE TRIGGER content_sources_set_updated_at
BEFORE UPDATE ON content_sources
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();

CREATE TABLE ingested_items (
    id BIGSERIAL PRIMARY KEY,
    source_id BIGINT NOT NULL REFERENCES content_sources (id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    summary TEXT,
    link TEXT NOT NULL,
    media_ref TEXT,
    dedup_key TEXT NOT NULL,
    source_published_at TIMESTAMPTZ,
    discovered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT ingested_items_title_not_blank CHECK (length(trim(title)) > 0),
    CONSTRAINT ingested_items_link_not_blank CHECK (length(trim(link)) > 0),
    CONSTRAINT ingested_items_dedup_key_not_blank CHECK (length(trim(dedup_key)) > 0),
    CONSTRAINT ingested_items_summary_not_blank CHECK (
        summary IS NULL OR length(trim(summary)) > 0
    ),
    CONSTRAINT ingested_items_media_ref_not_blank CHECK (
        media_ref IS NULL OR length(trim(media_ref)) > 0
    )
);

CREATE UNIQUE INDEX idx_ingested_items_source_dedup
    ON ingested_items (source_id, dedup_key);
CREATE INDEX idx_ingested_items_source_published_at
    ON ingested_items (source_id, source_published_at DESC NULLS LAST);
CREATE INDEX idx_ingested_items_ingested_at
    ON ingested_items (ingested_at DESC);

CREATE TRIGGER ingested_items_set_updated_at
BEFORE UPDATE ON ingested_items
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();
