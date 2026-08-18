-- A kind: the namespace a handle carries, and the schema of what it names.
--
-- The set of kinds was a closed list in the code, which made the nouns one
-- person's life contains a fact about the software. It is data here, so an
-- instance can hold a noun this build never heard of and a handle carrying it
-- still resolves.
--
-- `token` is the identity and the primary key. It is what a handle's prefix is
-- compared against, byte for byte, so nothing here normalizes it.
--
-- `origin` says who declared it, and it is what closes the shipped ten to a
-- caller. It is the same mechanism the type declarations use rather than a
-- second one: `declared` is the default because a row written before this
-- column existed can only have come from a caller.
--
-- The kinds have no keys yet. A kind IS a schema, so keys are coming, and they
-- are a table of their own when they arrive rather than a column here.
CREATE TABLE kind (
    token  VARCHAR(64) NOT NULL PRIMARY KEY,
    origin VARCHAR(16) NOT NULL DEFAULT 'declared'
);
