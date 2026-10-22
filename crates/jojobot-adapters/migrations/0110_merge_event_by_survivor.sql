-- The index answers "which merges folded something into this thing" without
-- reading every merge event. A read of a thing's history asks that once for each
-- thing it reads, by `survivor`, over the rows that hold the event itself
-- (`ordinal` 0). 0080 indexed the table by `folded` only, so each ask read the
-- whole table.
ALTER TABLE merge_event ADD INDEX merge_event_by_survivor (survivor, ordinal);
