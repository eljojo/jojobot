CREATE TABLE event_log (
    slot VARCHAR(66) NOT NULL PRIMARY KEY,
    first_position BIGINT UNSIGNED NULL UNIQUE,
    last_position BIGINT UNSIGNED NOT NULL,
    payload LONGBLOB NOT NULL,
    INDEX event_log_tail (last_position)
);
