-- Live score push (spec 13). Remembers the last (score, status) we pushed for
-- each match so the scoring tick can detect goals and the final whistle, and
-- never push the same scoreline twice (even across a restart). NULL means
-- "never observed": the first tick baselines every match without pushing, so
-- enabling this never blasts the backlog of already-played matches.
ALTER TABLE matches
    ADD COLUMN last_pushed_home   INT,
    ADD COLUMN last_pushed_away   INT,
    ADD COLUMN last_pushed_status TEXT;
