-- The snapshots the event log held.
--
-- **They moved to `log_snapshot`.** A row left here is a snapshot in the format
-- of the build before this one, which no build reads now. The first boot over
-- a store that held one finds no snapshot in the new table and replays the log
-- once; its stop publishes the first snapshot there. Nothing is removed from
-- the store's history (rule 687): the commits that held these rows still do.
DELETE FROM event_log WHERE slot LIKE 'snapshot:%';
