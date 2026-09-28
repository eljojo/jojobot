-- A caller's own declaration, kept after a shipped write took the name over.
--
-- **A shipped write landing on a declared row is not a refusal** — a caller
-- cannot write over a type the software ships, but the software's own write
-- may still replace a caller's. Nothing else remembers what that replaced,
-- so a caller meeting the name again finds it answering the software's
-- keys with no trace of their own. This table is what a redeclaration is
-- met with, and what a later read of the name carries beside it.
--
-- Same shape as `type_field`, minus `origin` (always the caller's, by
-- definition of what gets displaced) and `owner` (a kind is never
-- displaced this way). `replaced_on` is the day the software's write took
-- the name over, and it is the same for every row of one name: a type is
-- replaced whole.
--
-- **One row per name, ever.** Once a name is shipped, a caller's write over
-- it is refused, so the name never returns to being a caller's for this to
-- capture a second time.
CREATE TABLE displaced_type_field (
    type_name   VARCHAR(191) NOT NULL,
    key_name    VARCHAR(191) NOT NULL,
    ordinal     INT          NOT NULL,
    holds       VARCHAR(32)  NOT NULL,
    folds       VARCHAR(16)  NOT NULL DEFAULT 'newest',
    required    BOOLEAN      NOT NULL DEFAULT TRUE,
    one_of      TEXT NULL,
    -- Text for the reason every stamp in this store is text (see fact.date):
    -- the domain's date is the domain's, and a column that reformatted it
    -- would rewrite a record on its way through.
    replaced_on VARCHAR(16)  NOT NULL,
    PRIMARY KEY (type_name, key_name)
);
