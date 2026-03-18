-- Group profile: description and avatar
ALTER TABLE groups ADD COLUMN description TEXT;
ALTER TABLE groups ADD COLUMN avatar_path TEXT;
