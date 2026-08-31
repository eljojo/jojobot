-- One row per (sid, domain): whether this session's handle has already been
-- taught how that domain behaves.
--
-- `sid` rather than the session's own store id, because the sessions this
-- exists for are the ones that only ever read — they never write, so they
-- never get a card. The handle exists from the moment a session boots; the
-- card does not.
CREATE TABLE session_teaching (
    sid        VARCHAR(4)  NOT NULL,
    domain     VARCHAR(64) NOT NULL,
    taught_at  VARCHAR(48) NOT NULL,
    PRIMARY KEY (sid, domain)
);
