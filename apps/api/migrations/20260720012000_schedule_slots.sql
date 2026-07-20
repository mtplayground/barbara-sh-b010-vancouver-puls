CREATE TABLE schedule_slots (
    id BIGSERIAL PRIMARY KEY,
    slot_date DATE NOT NULL,
    slot_time TIME NOT NULL DEFAULT TIME '09:00:00',
    draft_id BIGINT UNIQUE REFERENCES post_drafts (id) ON DELETE SET NULL,
    created_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    updated_by_sub TEXT REFERENCES users (sub) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT schedule_slots_one_slot_per_day UNIQUE (slot_date),
    CONSTRAINT schedule_slots_draft_id_positive CHECK (draft_id IS NULL OR draft_id > 0)
);

CREATE INDEX idx_schedule_slots_slot_date ON schedule_slots (slot_date);
CREATE INDEX idx_schedule_slots_draft_id ON schedule_slots (draft_id);

CREATE TRIGGER schedule_slots_set_updated_at
BEFORE UPDATE ON schedule_slots
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();
