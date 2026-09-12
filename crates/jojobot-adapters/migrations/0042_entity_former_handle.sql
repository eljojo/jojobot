-- One row per rename event: a handle a thing used to answer to, and the
-- badge it still answers to under whatever handle it wears now.
--
-- `rename_entity` is the one verb that writes here, one row per call.
--
-- Kept forever, one row per event, never only the newest: a reference
-- written under the FIRST handle a thing ever wore has to keep resolving,
-- and keeping only the last rename would break exactly that case, silently
-- — a miss on it would read no differently from a handle that never existed.
CREATE TABLE entity_former_handle (
    former_handle VARCHAR(64) NOT NULL,
    badge         VARCHAR(16) NOT NULL,
    changed_at    VARCHAR(16) NOT NULL,
    PRIMARY KEY (former_handle)
);
