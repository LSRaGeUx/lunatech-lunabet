-- Rank-change push alerts (spec 08). Remembers the leaderboard rank we last
-- pushed to each user so the scoring tick can detect when they climb and notify
-- them once. NULL means "never recorded": the first tick baselines every user
-- without pushing, so enabling this never blasts the whole leaderboard.
ALTER TABLE users ADD COLUMN last_notified_rank INT;
