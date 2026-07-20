DO $$
BEGIN
    CREATE TYPE instagram_publish_target AS ENUM ('post', 'reel');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

DO $$
BEGIN
    CREATE TYPE publish_log_status AS ENUM ('success', 'failure');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

CREATE TABLE publish_logs (
    id BIGSERIAL PRIMARY KEY,
    draft_id BIGINT NOT NULL REFERENCES post_drafts (id) ON DELETE CASCADE,
    target instagram_publish_target NOT NULL,
    status publish_log_status NOT NULL,
    instagram_account_id TEXT NOT NULL,
    asset_ref TEXT NOT NULL,
    graph_container_id TEXT,
    graph_media_id TEXT,
    error_message TEXT,
    requested_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT publish_logs_instagram_account_id_not_blank CHECK (
        length(trim(instagram_account_id)) > 0
    ),
    CONSTRAINT publish_logs_asset_ref_not_blank CHECK (length(trim(asset_ref)) > 0),
    CONSTRAINT publish_logs_graph_container_id_not_blank CHECK (
        graph_container_id IS NULL OR length(trim(graph_container_id)) > 0
    ),
    CONSTRAINT publish_logs_graph_media_id_not_blank CHECK (
        graph_media_id IS NULL OR length(trim(graph_media_id)) > 0
    ),
    CONSTRAINT publish_logs_error_message_not_blank CHECK (
        error_message IS NULL OR length(trim(error_message)) > 0
    ),
    CONSTRAINT publish_logs_success_has_media_id CHECK (
        status <> 'success' OR graph_media_id IS NOT NULL
    ),
    CONSTRAINT publish_logs_failure_has_error CHECK (
        status <> 'failure' OR error_message IS NOT NULL
    )
);

CREATE INDEX idx_publish_logs_draft_id_created_at
ON publish_logs (draft_id, created_at DESC);

CREATE INDEX idx_publish_logs_status_created_at
ON publish_logs (status, created_at DESC);
