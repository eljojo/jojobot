-- The handles a build has supplied as records, one row each, kept across builds.
--
-- A record the build supplies has no row, so the operator's writes on it are
-- filed under the handle's own text. When a later build stops supplying the
-- handle, those writes have no record to answer for them, and the boot gives
-- the handle a row (rule 508). That pass must adopt only a handle a build once
-- supplied: a colon key with no row can also be a row removed by hand or a
-- leftover of a rename that predates the former-handle table, and neither is
-- the build's to adopt.
--
-- Every boot writes the handles it supplies, plus a list the build carries of
-- handles earlier builds supplied before this table existed. Rows are never
-- removed: a handle a build stopped supplying must stay listed, because that is
-- what makes its writes adoptable.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE supplied_handle (
    handle VARCHAR(191) NOT NULL PRIMARY KEY
);
