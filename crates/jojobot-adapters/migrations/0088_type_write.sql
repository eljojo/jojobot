-- Every declaration a caller made of a type or a kind, in the order it was made.
--
-- `type_field` holds the keys a name declares NOW: declaring deletes the rows
-- sharing the name and writes the new set, so an earlier declaration, the run
-- that sent it and the moment it was sent are gone. This keeps each declaration
-- as one row, so they can be read back. `type_field` stays the projection and
-- answers every current read as before.
--
-- `ordinal` is the declaration's place among the declarations of that name,
-- counted from 1. `written_at` is the moment on the server's own clock.
-- `by_session` is the public name of the run that sent the declaration, NULL
-- when no run was known. `fields` is a JSON list with one object for each key,
-- in the order declared: key, holds, folds, required and one_of, as `type_field`
-- spells them.
--
-- Only a caller's declaration is kept. What the software ships is declared again
-- at every start and writes no row here. A declaration made before this existed
-- has no row. Absence says nothing was recorded, never that nothing happened,
-- and nothing is backfilled.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE type_write (
    name       VARCHAR(191) NOT NULL,
    ordinal    INT          NOT NULL,
    written_at VARCHAR(40)  NOT NULL,
    by_session VARCHAR(64)  NULL,
    fields     LONGTEXT     NOT NULL,
    PRIMARY KEY (name, ordinal)
);
