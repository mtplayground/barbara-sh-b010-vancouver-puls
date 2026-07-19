CREATE TABLE user_invites (
    token_hash TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    role user_role NOT NULL DEFAULT 'editor',
    invited_by_sub TEXT NOT NULL REFERENCES users (sub) ON DELETE RESTRICT,
    accepted_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ
);

CREATE INDEX idx_user_invites_email ON user_invites (lower(email));
CREATE INDEX idx_user_invites_pending ON user_invites (expires_at) WHERE accepted_at IS NULL;
