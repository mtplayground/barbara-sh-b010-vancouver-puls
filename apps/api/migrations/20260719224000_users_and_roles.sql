DO $$
BEGIN
    CREATE TYPE user_role AS ENUM ('admin', 'editor');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

CREATE TABLE users (
    sub TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    name TEXT,
    picture_url TEXT,
    role user_role NOT NULL DEFAULT 'editor',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT users_email_not_blank CHECK (length(trim(email)) > 0)
);

CREATE INDEX users_email_idx ON users (lower(email));
CREATE INDEX users_role_idx ON users (role);

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_set_updated_at
BEFORE UPDATE ON users
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();
