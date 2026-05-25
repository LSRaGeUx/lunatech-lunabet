ALTER TABLE matches ADD COLUMN reminded_at TIMESTAMPTZ;
CREATE INDEX matches_reminded_idx ON matches (kickoff_at) WHERE reminded_at IS NULL;
