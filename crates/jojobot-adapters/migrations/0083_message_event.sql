-- Every change of state a message went through.
--
-- `message` keeps the state a message holds NOW and the notes of the call that
-- retired it last: a second retirement overwrites the first's notes, a
-- quarantine overwrites the state it set aside, and a read leaves only a token
-- for how it was taken (`message_delivery`, 0014). This keeps the changes
-- themselves, one row each, in the order they happened, so an earlier state,
-- note or run can be read back. `message` stays as the projection and answers
-- every current read as before.
--
-- `kind` is read, processed or quarantined. `at` is the moment on the server's
-- own clock. `by_session` is the public name of the run that made the change,
-- NULL when no run was known. `by_box` is the box the change was made through:
-- the box a delivery read, the box the message was filed in for a read of one
-- and for a retirement, and the box that decided for a quarantine. `prior_state`
-- is the state the message held before the change. `notes` holds only what that
-- call was given, so a retirement that sent none is NULL even when the message
-- carries an earlier call's notes. `reason` is a quarantine's reason.
--
-- A message changed before this existed has no row here. Absence says nothing
-- was recorded, never that nothing happened, and nothing is backfilled.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE message_event (
    message_id  VARCHAR(64)  NOT NULL,
    ordinal     INT          NOT NULL,
    kind        VARCHAR(16)  NOT NULL,
    at          VARCHAR(40)  NOT NULL,
    by_session  VARCHAR(64)  NULL,
    by_box      VARCHAR(191) NULL,
    prior_state VARCHAR(16)  NOT NULL,
    notes       TEXT         NULL,
    reason      TEXT         NULL,
    PRIMARY KEY (message_id, ordinal)
);
