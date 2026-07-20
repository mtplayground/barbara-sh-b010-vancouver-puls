CREATE TABLE instagram_insight_snapshots (
    id BIGSERIAL PRIMARY KEY,
    instagram_account_id TEXT NOT NULL,
    followers_count BIGINT NOT NULL DEFAULT 0,
    reach BIGINT NOT NULL DEFAULT 0,
    saves BIGINT NOT NULL DEFAULT 0,
    shares BIGINT NOT NULL DEFAULT 0,
    raw_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT instagram_insight_snapshots_account_id_not_blank CHECK (
        length(trim(instagram_account_id)) > 0
    ),
    CONSTRAINT instagram_insight_snapshots_followers_non_negative CHECK (
        followers_count >= 0
    ),
    CONSTRAINT instagram_insight_snapshots_reach_non_negative CHECK (reach >= 0),
    CONSTRAINT instagram_insight_snapshots_saves_non_negative CHECK (saves >= 0),
    CONSTRAINT instagram_insight_snapshots_shares_non_negative CHECK (shares >= 0)
);

CREATE INDEX idx_instagram_insight_snapshots_account_captured_at
ON instagram_insight_snapshots (instagram_account_id, captured_at DESC);

CREATE INDEX idx_instagram_insight_snapshots_captured_at
ON instagram_insight_snapshots (captured_at DESC);
