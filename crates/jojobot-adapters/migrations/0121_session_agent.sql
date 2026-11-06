ALTER TABLE session ADD COLUMN agent VARCHAR(191) NULL, ADD INDEX by_agent (agent);
