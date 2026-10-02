-- Indexes for schema version 1 (spike 3's set, promoted to product: docs/spikes/03-catalogue-and-grid.md).

CREATE INDEX photo_time ON photo(capture_time);
CREATE INDEX photo_rating_time ON photo(rating, capture_time);
CREATE INDEX photo_eff_rating_time ON photo(effective_rating, capture_time);
CREATE INDEX photo_camera_time ON photo(camera_id, capture_time);
CREATE INDEX photo_series ON photo(series_id);
-- (The two indexes of the place filter are in `open.rs`, `PLACE_INDEXES`: one text for a new catalogue, the migration and
-- the refill when the keys change.)
CREATE INDEX photo_fingerprint ON photo(fingerprint);
CREATE INDEX photo_source ON photo(source_id);
CREATE INDEX version_photo ON version(photo_id);
CREATE INDEX keyword_path ON keyword(path);
CREATE INDEX keyword_parent ON keyword(parent_id);
CREATE INDEX photo_keyword_kw ON photo_keyword(keyword_id, photo_id);
CREATE INDEX version_keyword_kw ON version_keyword(keyword_id, version_id);
CREATE INDEX collection_member_photo ON collection_member(photo_id);
CREATE INDEX series_cover ON series(cover_photo_id);
