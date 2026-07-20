DO $$
BEGIN
    CREATE TYPE instagram_account_type AS ENUM ('business', 'creator');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

CREATE TABLE instagram_connections (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE,
    instagram_account_id TEXT NOT NULL,
    username TEXT,
    account_type instagram_account_type NOT NULL,
    graph_api_version TEXT NOT NULL,
    app_id TEXT NOT NULL,
    access_token TEXT NOT NULL,
    token_source TEXT NOT NULL DEFAULT 'environment',
    connected_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    disconnected_at TIMESTAMPTZ,
    connected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT instagram_connections_singleton CHECK (id),
    CONSTRAINT instagram_connections_account_id_not_blank CHECK (
        length(trim(instagram_account_id)) > 0
    ),
    CONSTRAINT instagram_connections_graph_api_version_not_blank CHECK (
        length(trim(graph_api_version)) > 0
    ),
    CONSTRAINT instagram_connections_app_id_not_blank CHECK (length(trim(app_id)) > 0),
    CONSTRAINT instagram_connections_access_token_not_blank CHECK (
        length(trim(access_token)) > 0
    ),
    CONSTRAINT instagram_connections_token_source_not_blank CHECK (
        length(trim(token_source)) > 0
    ),
    CONSTRAINT instagram_connections_username_not_blank CHECK (
        username IS NULL OR length(trim(username)) > 0
    )
);

CREATE TRIGGER instagram_connections_set_updated_at
BEFORE UPDATE ON instagram_connections
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();
