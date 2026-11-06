CREATE TABLE role_lease_heartbeat (
    role       VARCHAR(191) NOT NULL PRIMARY KEY,
    moment     VARCHAR(64)  NOT NULL,
    written_at VARCHAR(64)  NOT NULL
);
