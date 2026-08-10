-- A declared type: one row per key it names.
--
-- **A type IS the set of rows sharing a name**, and there is no table of names
-- beside this one. That makes a type with no keys unrepresentable rather than
-- merely refused: a type arrives complete with its fields, so an empty shell
-- somebody has to configure into usefulness cannot exist here at all.
--
-- **Nothing in this table admits a record to a type.** Matching is structural:
-- a record carrying these keys answers this type whether or not anybody
-- declared it. A declaration says which keys a writer should fill, and a query
-- reads it to know which keys to look for. A row here describes; it does not
-- gate.
--
-- `ordinal` keeps the order the type was declared in. The answer names the keys
-- a record holds and the keys it lacks in the type's own order, so a reader
-- sorting by key name would replace an order the declaration fixed with one
-- nobody chose.
--
-- The key name is part of the primary key, so one type cannot name one key
-- twice. Two DIFFERENT types may name the same key and mean their own thing by
-- it: keys are scoped by the type that names them and are registered nowhere.
CREATE TABLE type_field (
    type_name VARCHAR(191) NOT NULL,
    key_name  VARCHAR(191) NOT NULL,
    ordinal   INT          NOT NULL,
    holds     VARCHAR(16)  NOT NULL,
    PRIMARY KEY (type_name, key_name)
);
