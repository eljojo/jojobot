-- A merge as an appended event, and where each of the folded thing's claims
-- went. Created only: nothing that exists is altered, and a merge made before
-- this table has no row in it and reads as it always did.
--
-- One table keeps both, so one statement makes both. A row is either the event
-- itself (`ordinal` 0: who was folded into whom, when, and in which run) or one
-- claim it moved (`ordinal` 1 and up: the claim's local id under the folded
-- thing and its local id under the survivor). Every row carries the two
-- storage keys, so the map is read by `folded` and `folded_id` without a join.
--
-- `folded` and `survivor` are the permanent ids the two things wear, never
-- handles (rule 268), so a rename after the merge changes what they are served
-- as and nothing stored. `merge_no` counts the merges made, from one.
CREATE TABLE merge_event (
    merge_no    BIGINT       NOT NULL,
    ordinal     BIGINT       NOT NULL,
    folded      VARCHAR(191) NOT NULL,
    survivor    VARCHAR(191) NOT NULL,
    merged_at   VARCHAR(40)  NOT NULL,
    session     VARCHAR(64)  NULL,
    folded_id   VARCHAR(64)  NULL,
    survivor_id VARCHAR(64)  NULL,
    PRIMARY KEY (merge_no, ordinal),
    INDEX merge_event_by_folded (folded, folded_id)
);
