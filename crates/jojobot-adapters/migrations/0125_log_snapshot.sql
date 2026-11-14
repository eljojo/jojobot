-- Where the log's snapshots are kept.
--
-- **A snapshot is derived from the log, and the log is its history.** It used
-- to be a row of the event log, and every commit mark pinned the copy it found
-- there, so each changed snapshot left a whole copy in the store's history for
-- good. It is now a table of its own, named in `dolt_ignore` by the runner
-- before this statement runs, so no commit ever holds it (rule 687: nothing is
-- removed from history, because derived data never enters it).
--
-- **The blob is stored as bytes.** The event log wrote the whole snapshot as
-- one JSON array of numbers, which the store cannot share between two copies.
-- The header is columns, and `body` is the blob itself.
--
-- **One row for each slot.** A slot is the fingerprint of the State's shape the
-- snapshot was written for, as it was in the event log.
CREATE TABLE log_snapshot (
    slot                    VARCHAR(32)     NOT NULL PRIMARY KEY,
    covered_position        BIGINT UNSIGNED NOT NULL,
    snapshot_format_version INT UNSIGNED    NOT NULL,
    projection_version      INT UNSIGNED    NOT NULL,
    log_identity            VARCHAR(191)    NOT NULL,
    representation_epoch    BIGINT UNSIGNED NOT NULL,
    erasure_epoch           BIGINT UNSIGNED NOT NULL,
    batch_checksum          BINARY(32)      NOT NULL,
    created_at              VARCHAR(40)     NOT NULL,
    checksum                BINARY(32)      NOT NULL,
    serialized_length       BIGINT UNSIGNED NOT NULL,
    body                    LONGBLOB        NOT NULL
);
