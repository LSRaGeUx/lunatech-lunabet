-- Per-user language preference, used to localise background emails (match
-- reminders) which are sent outside any request and so can't read the
-- `lb_lang` cookie. Defaults to English; set to 'fr' when a user explicitly
-- switches the UI to French.
ALTER TABLE users ADD COLUMN lang TEXT NOT NULL DEFAULT 'en';
