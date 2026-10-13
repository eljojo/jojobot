-- What an entity looked like at each write, and the session that wrote it.
--
-- `entity_write` (0051) logged that a write happened and when. This keeps the
-- rest: the whole row as it stood after the write, the way `fact_write` (0034)
-- keeps the whole claim on every write. A rename, an alias change, a prose edit,
-- an archive and a restore each leave a row, so an earlier value can be read
-- back and told from one the entity always held. The `entity` row stays as the
-- projection and answers every current read as before.
--
-- `aliases` is the alias set as one text: a JSON array, in order, because an
-- alias is free text and no separator is safe in it. `badge` is the permanent id
-- of the thing, so a history read is exact after a rename and after a freed
-- handle is taken by a new thing. `session` is the run that made the write.
--
-- **Every column is nullable and nothing is backfilled.** A row an older build
-- appended holds none of this, the earlier values are gone, and it stays NULL.
-- A reader treats a NULL snapshot as "not recorded", never as an empty entity.
--
-- One statement, because this store commits DDL as it goes and a second
-- statement could be left unapplied.
ALTER TABLE entity_write
    ADD COLUMN kind            VARCHAR(32)  NULL,
    ADD COLUMN name            TEXT         NULL,
    ADD COLUMN source          VARCHAR(255) NULL,
    ADD COLUMN crm             VARCHAR(255) NULL,
    ADD COLUMN parent          VARCHAR(191) NULL,
    ADD COLUMN boot            VARCHAR(16)  NULL,
    ADD COLUMN archived_reason TEXT         NULL,
    ADD COLUMN archived_at     VARCHAR(40)  NULL,
    ADD COLUMN merged_into     VARCHAR(191) NULL,
    ADD COLUMN aliases         TEXT         NULL,
    ADD COLUMN prose           LONGTEXT     NULL,
    ADD COLUMN badge           VARCHAR(16)  NULL,
    ADD COLUMN session         VARCHAR(64)  NULL;
