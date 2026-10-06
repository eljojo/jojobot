CREATE DATABASE IF NOT EXISTS `jojobot`; USE `jojobot`; 
SET FOREIGN_KEY_CHECKS=0;
SET UNIQUE_CHECKS=0;
DROP TABLE IF EXISTS `displaced_type_field`;
CREATE TABLE `displaced_type_field` (
  `type_name` varchar(191) NOT NULL,
  `key_name` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `holds` varchar(32) NOT NULL,
  `folds` varchar(16) NOT NULL DEFAULT 'newest',
  `required` tinyint(1) NOT NULL DEFAULT '1',
  `one_of` text,
  `replaced_on` varchar(16) NOT NULL,
  PRIMARY KEY (`type_name`,`key_name`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
DROP TABLE IF EXISTS `entity`;
CREATE TABLE `entity` (
  `id` varchar(191) NOT NULL,
  `kind` varchar(32) NOT NULL,
  `name` text NOT NULL,
  `source` varchar(255) NOT NULL,
  `crm` varchar(255),
  `parent` varchar(191),
  `boot` varchar(16) NOT NULL,
  `prose` longtext NOT NULL,
  `badge` varchar(16),
  `merged_into` varchar(191),
  `archived_reason` text,
  `archived_at` varchar(40),
  PRIMARY KEY (`id`),
  KEY `by_kind` (`kind`),
  KEY `by_parent` (`parent`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','evj1rt',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','9cf1z9',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','w83637',NULL,NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','ezz4w7',NULL,NULL,NULL);
DROP TABLE IF EXISTS `entity_alias`;
CREATE TABLE `entity_alias` (
  `entity` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `alias` text NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
DROP TABLE IF EXISTS `entity_former_handle`;
CREATE TABLE `entity_former_handle` (
  `former_handle` varchar(191) NOT NULL,
  `badge` varchar(16) NOT NULL,
  `changed_at` varchar(16) NOT NULL,
  `ordinal` int NOT NULL DEFAULT '1',
  PRIMARY KEY (`former_handle`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
DROP TABLE IF EXISTS `entity_write`;
CREATE TABLE `entity_write` (
  `entity` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `written_at` varchar(40) NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-06T04:16:52.867239955Z'), ('person:upgrade-fixture-person',1,'2026-10-06T04:16:53.188774776Z'), ('place:upgrade-fixture-place',1,'2026-10-06T04:16:53.317823074Z'), ('thing:upgrade-fixture-thing',1,'2026-10-06T04:16:53.454599612Z');
DROP TABLE IF EXISTS `fact`;
CREATE TABLE `fact` (
  `entity` varchar(191) NOT NULL,
  `id` varchar(64) NOT NULL,
  `content` longtext NOT NULL,
  `details` longtext,
  `provenance` varchar(16) NOT NULL,
  `standing` varchar(16),
  `status` varchar(16) NOT NULL,
  `recorded_at` varchar(16) NOT NULL,
  `edge_shape` varchar(32),
  `edge_object` varchar(191),
  `event_kind` text,
  `derived_from` varchar(191),
  `derived_from_id` varchar(64),
  `inserted_at` varchar(40),
  `stale_after` varchar(16),
  `happened_at` varchar(16),
  `happened_through` varchar(16),
  PRIMARY KEY (`entity`,`id`),
  KEY `by_edge` (`edge_object`),
  KEY `by_source` (`derived_from`,`derived_from_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('9cf1z9','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-06','location','w83637',NULL,NULL,NULL,'2026-10-06T04:16:53.588006706Z',NULL,NULL,NULL), ('evj1rt','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,NULL,NULL), ('evj1rt','f2','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-06','connection','ezz4w7',NULL,NULL,NULL,'2026-10-06T04:16:53.919005483Z',NULL,NULL,NULL);
DROP TABLE IF EXISTS `fact_event_metadata`;
CREATE TABLE `fact_event_metadata` (
  `fact_home` varchar(191) NOT NULL,
  `fact_id` varchar(64) NOT NULL,
  `key` varchar(191) NOT NULL,
  `value` longtext NOT NULL,
  PRIMARY KEY (`fact_home`,`fact_id`,`key`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
DROP TABLE IF EXISTS `fact_event_ref`;
CREATE TABLE `fact_event_ref` (
  `fact_home` varchar(191) NOT NULL,
  `fact_id` varchar(64) NOT NULL,
  `ordinal` int NOT NULL,
  `entity` varchar(191) NOT NULL,
  PRIMARY KEY (`fact_home`,`fact_id`,`ordinal`),
  KEY `by_entity` (`entity`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
DROP TABLE IF EXISTS `fact_stands_for`;
CREATE TABLE `fact_stands_for` (
  `fact_home` varchar(191) NOT NULL,
  `fact_id` varchar(64) NOT NULL,
  `ordinal` int NOT NULL,
  `source_home` varchar(191) NOT NULL,
  `source_id` varchar(64) NOT NULL,
  PRIMARY KEY (`fact_home`,`fact_id`,`ordinal`),
  KEY `by_source` (`source_home`,`source_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
DROP TABLE IF EXISTS `fact_write`;
CREATE TABLE `fact_write` (
  `entity` varchar(191) NOT NULL,
  `fact_id` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  `content` longtext NOT NULL,
  `details` longtext,
  `provenance` varchar(16) NOT NULL,
  `standing` varchar(16),
  `status` varchar(16) NOT NULL,
  `recorded_at` varchar(16) NOT NULL,
  `edge_shape` varchar(32),
  `edge_object` varchar(191),
  `derived_from` varchar(191),
  `derived_from_id` varchar(64),
  `inserted_at` varchar(40),
  `stale_after` varchar(16),
  `written_at` varchar(40),
  `happened_at` varchar(16),
  `happened_through` varchar(16),
  PRIMARY KEY (`entity`,`fact_id`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`) VALUES ('9cf1z9','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-06','location','w83637',NULL,NULL,'2026-10-06T04:16:53.588006706Z',NULL,'2026-10-06T04:16:53.592429873Z',NULL,NULL), ('evj1rt','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:53.098385658Z',NULL,NULL), ('evj1rt','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:53.235696108Z',NULL,NULL), ('evj1rt','f1',3,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:53.371859226Z',NULL,NULL), ('evj1rt','f1',4,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:53.50524872Z',NULL,NULL), ('evj1rt','f1',5,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:53.764208106Z',NULL,NULL), ('evj1rt','f1',6,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:53.999338735Z',NULL,NULL), ('evj1rt','f1',7,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:54.142542245Z',NULL,NULL), ('evj1rt','f1',8,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:54.379937577Z',NULL,NULL), ('evj1rt','f1',9,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:54.51289984Z',NULL,NULL), ('evj1rt','f1',10,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:54.850365957Z',NULL,NULL), ('evj1rt','f1',11,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:54.959757764Z',NULL,NULL), ('evj1rt','f1',12,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T04:16:53.092790509Z',NULL,'2026-10-06T04:16:55.064524135Z',NULL,NULL), ('evj1rt','f2',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-06','connection','ezz4w7',NULL,NULL,'2026-10-06T04:16:53.919005483Z',NULL,'2026-10-06T04:16:53.922674558Z',NULL,NULL);
DROP TABLE IF EXISTS `field_write`;
CREATE TABLE `field_write` (
  `entity` varchar(191) NOT NULL,
  `key` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `value` longtext,
  `fact_id` varchar(64) NOT NULL,
  PRIMARY KEY (`entity`,`key`,`ordinal`),
  KEY `by_record` (`entity`,`fact_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('9cf1z9','since',1,'2026-01-01','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',1,'2026-10-06T04:16:53.085315868Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',2,'2026-10-06T04:16:53.218465401Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',3,'2026-10-06T04:16:53.347259809Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',4,'2026-10-06T04:16:53.4854354Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',5,'2026-10-06T04:16:53.744977186Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',6,'2026-10-06T04:16:53.980195356Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',7,'2026-10-06T04:16:54.122725634Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',8,'2026-10-06T04:16:54.355314859Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',9,'2026-10-06T04:16:54.49317744Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',10,'2026-10-06T04:16:54.830730216Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',11,'2026-10-06T04:16:54.933499904Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/claimed_at',12,'1970-01-01T00:00:00Z','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',1,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',2,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',3,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',4,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',5,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',6,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',7,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',8,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',9,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',10,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',11,'h65k','f1'), ('evj1rt','role/upgrade-fixture-recorder/holder',12,'h65k','f1');
DROP TABLE IF EXISTS `handover`;
CREATE TABLE `handover` (
  `what` varchar(64) NOT NULL,
  `state` varchar(16) NOT NULL,
  PRIMARY KEY (`what`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
DROP TABLE IF EXISTS `journal_entry`;
CREATE TABLE `journal_entry` (
  `session` varchar(64) NOT NULL,
  `id` varchar(64) NOT NULL,
  `ordinal` int NOT NULL,
  `at` varchar(48) NOT NULL,
  `text` longtext NOT NULL,
  `touched` varchar(48),
  `beat` varchar(191),
  `happened_on` varchar(10),
  `closing_focus` text,
  PRIMARY KEY (`session`,`id`),
  KEY `in_order` (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`) VALUES ('ffeqz5','4tqgxz',4,'2026-10-06T04:16:54.916042637Z','retired messages: 2pzpt0 (1)',NULL,'mark_processed',NULL,NULL), ('ffeqz5','73b13h',5,'2026-10-06T04:16:54.933499904Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL), ('ffeqz5','fhhbd0',1,'2026-10-06T04:16:53.292376615Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing (3)','2026-10-06T04:16:53.562816098Z','add_entity',NULL,NULL), ('ffeqz5','w0vhxf',2,'2026-10-06T04:16:53.824093547Z','captured facts about: person:upgrade-fixture-person, bot:assistant (2)','2026-10-06T04:16:54.058664396Z','capture',NULL,NULL), ('ffeqz5','y686tw',3,'2026-10-06T04:16:54.306972555Z','posted to mailboxes: assistant, … (3)','2026-10-06T04:16:54.752216906Z','post_message',NULL,NULL);
DROP TABLE IF EXISTS `kind`;
CREATE TABLE `kind` (
  `token` varchar(64) NOT NULL,
  `origin` varchar(16) NOT NULL DEFAULT 'declared',
  PRIMARY KEY (`token`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `kind` (`token`,`origin`) VALUES ('bot','shipped'), ('event','shipped'), ('machine','shipped'), ('org','shipped'), ('person','shipped'), ('pet','shipped'), ('place','shipped'), ('project','shipped'), ('rhythm','shipped'), ('session','shipped'), ('thing','shipped'), ('thread','shipped'), ('topic','shipped'), ('view','shipped'), ('work','shipped');
DROP TABLE IF EXISTS `mailbox`;
CREATE TABLE `mailbox` (
  `name` varchar(191) NOT NULL,
  `owner` varchar(191) NOT NULL,
  PRIMARY KEY (`name`),
  KEY `by_owner` (`owner`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `mailbox` (`name`,`owner`) VALUES ('assistant','bot:assistant');
DROP TABLE IF EXISTS `message`;
CREATE TABLE `message` (
  `id` varchar(64) NOT NULL,
  `mailbox` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `body` longtext NOT NULL,
  `subject` text,
  `sender` varchar(191) NOT NULL,
  `sent_at` varchar(48) NOT NULL,
  `state` varchar(16) NOT NULL,
  `notes` text,
  `in_reply_to` varchar(64),
  `sender_mail_waiting_at_send` bigint,
  `posted_by_session` varchar(64),
  `quarantined_by` varchar(191),
  `quarantined_at` varchar(48),
  `quarantine_reason` text,
  PRIMARY KEY (`id`),
  KEY `in_order` (`mailbox`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('2pzpt0','assistant',3,'will be processed',NULL,'evj1rt','2026-10-06T04:16:54.468906053Z','processed',NULL,NULL,2,'ffeqz5',NULL,NULL,NULL), ('6vg6jy','assistant',1,'left new',NULL,'evj1rt','2026-10-06T04:16:54.095799702Z','new',NULL,NULL,0,'ffeqz5',NULL,NULL,NULL), ('fp1ze7','assistant',2,'will be read',NULL,'evj1rt','2026-10-06T04:16:54.331876144Z','read',NULL,NULL,1,'ffeqz5',NULL,NULL,NULL);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('fp1ze7','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-06T04:16:52.020850011Z'), ('0002_journal_entry','2026-10-06T04:16:52.032427458Z'), ('0003_minted','2026-10-06T04:16:52.040384271Z'), ('0004_mailbox','2026-10-06T04:16:52.047696332Z'), ('0005_message','2026-10-06T04:16:52.055522484Z'), ('0006_handover','2026-10-06T04:16:52.063182566Z'), ('0007_drop_minted','2026-10-06T04:16:52.073399112Z'), ('0008_entity','2026-10-06T04:16:52.084803549Z'), ('0009_entity_alias','2026-10-06T04:16:52.093255922Z'), ('0010_fact','2026-10-06T04:16:52.102488635Z'), ('0011_fact_event_metadata','2026-10-06T04:16:52.111353989Z'), ('0012_fact_event_ref','2026-10-06T04:16:52.119847113Z'), ('0013_type_field','2026-10-06T04:16:52.127624994Z'), ('0014_message_delivery','2026-10-06T04:16:52.135687277Z'), ('0015_type_field_origin','2026-10-06T04:16:52.143981089Z'), ('0016_field_write','2026-10-06T04:16:52.152609203Z'), ('0017_type_field_folds','2026-10-06T04:16:52.160678865Z'), ('0018_type_field_holds_wider','2026-10-06T04:16:52.168425327Z'), ('0019_entity_source_wider','2026-10-06T04:16:52.176417269Z'), ('0020_entity_crm_wider','2026-10-06T04:16:52.18391267Z'), ('0021_kind','2026-10-06T04:16:52.191991072Z'), ('0022_type_field_owner','2026-10-06T04:16:52.200808666Z'), ('0023_type_field_required','2026-10-06T04:16:52.208887959Z'), ('0024_rhythm_kind_takes_its_name','2026-10-06T04:16:52.215654689Z'), ('0025_type_field_one_of','2026-10-06T04:16:52.224272622Z'), ('0026_session_timezone','2026-10-06T04:16:52.232186414Z'), ('0027_fact_inserted_at','2026-10-06T04:16:52.245271874Z'), ('0028_fact_stale_after','2026-10-06T04:16:52.253266886Z'), ('0029_fact_by_source','2026-10-06T04:16:52.26201733Z'), ('0030_session_stated_day','2026-10-06T04:16:52.270446763Z'), ('0031_journal_entry_day','2026-10-06T04:16:52.279041726Z'), ('0032_entity_badge','2026-10-06T04:16:52.28820478Z'), ('0033_entity_merged_into','2026-10-06T04:16:52.298571486Z'), ('0034_fact_write','2026-10-06T04:16:52.30742716Z'), ('0035_fact_write_backfill','2026-10-06T04:16:52.314838681Z'), ('0036_fact_write_moment','2026-10-06T04:16:52.323560433Z'), ('0037_fact_happened_at','2026-10-06T04:16:52.335521842Z'), ('0038_fact_write_happened_at','2026-10-06T04:16:52.344513966Z'), ('0039_fact_recorded_at','2026-10-06T04:16:52.353522739Z'), ('0040_fact_write_recorded_at','2026-10-06T04:16:52.365064637Z'), ('0041_session_teaching','2026-10-06T04:16:52.373744581Z'), ('0042_entity_former_handle','2026-10-06T04:16:52.382795224Z'), ('0043_message_sender_mail_waiting','2026-10-06T04:16:52.391703758Z'), ('0044_fact_stands_for','2026-10-06T04:16:52.401054102Z'), ('0045_entity_former_handle_key_add','2026-10-06T04:16:52.436622407Z'), ('0045_entity_former_handle_key_drop','2026-10-06T04:16:52.428788834Z'), ('0045_entity_former_handle_ordinal','2026-10-06T04:16:52.418779839Z'), ('0045_entity_former_handle_width','2026-10-06T04:16:52.409847496Z'), ('0046_fact_status_archived','2026-10-06T04:16:52.443057147Z'), ('0047_fact_write_status_archived','2026-10-06T04:16:52.449255116Z'), ('0048_session_served_chars','2026-10-06T04:16:52.457906569Z'), ('0049_entity_archived','2026-10-06T04:16:52.466663963Z'), ('0050_entity_archived_at','2026-10-06T04:16:52.474684474Z'), ('0051_entity_write','2026-10-06T04:16:52.482394967Z'), ('0052_fact_happened_through','2026-10-06T04:16:52.49144506Z'), ('0053_fact_write_happened_through','2026-10-06T04:16:52.500348804Z'), ('0054_session_write','2026-10-06T04:16:52.507846815Z'), ('0055_message_posted_by_session','2026-10-06T04:16:52.516199848Z'), ('0056_session_stated_day','2026-10-06T04:16:52.524466241Z'), ('0057_message_quarantined_by','2026-10-06T04:16:52.532147653Z'), ('0058_message_quarantined_at','2026-10-06T04:16:52.540747535Z'), ('0059_message_quarantine_reason','2026-10-06T04:16:52.550983392Z'), ('0060_journal_entry_closing_focus','2026-10-06T04:16:52.559103584Z'), ('0061_displaced_type_field','2026-10-06T04:16:52.567336656Z');
DROP TABLE IF EXISTS `schema_migration_begun`;
CREATE TABLE `schema_migration_begun` (
  `version` varchar(64) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
DROP TABLE IF EXISTS `session`;
CREATE TABLE `session` (
  `id` varchar(64) NOT NULL,
  `sid` varchar(64),
  `bot` varchar(191) NOT NULL,
  `focus` text NOT NULL,
  `started_at` varchar(48) NOT NULL,
  `state` varchar(16) NOT NULL,
  `timezone` varchar(64),
  `started_on` varchar(10),
  `served_chars` bigint NOT NULL DEFAULT '0',
  `stated_day` varchar(10),
  PRIMARY KEY (`id`),
  KEY `by_bot` (`bot`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`) VALUES ('ffeqz5','h65k','evj1rt','claimed the upgrade-fixture-recorder role','2026-10-06T04:16:53.157200518Z','wrapped',NULL,NULL,10176,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('h65k','claim-direction','2026-10-06T04:16:53.848864136Z'), ('h65k','claim-subject','2026-10-06T04:16:53.844576288Z'), ('h65k','claims','2026-10-06T04:16:53.840742303Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('ffeqz5',1), ('ffeqz5',2), ('ffeqz5',3), ('ffeqz5',4), ('ffeqz5',5), ('ffeqz5',6), ('ffeqz5',7), ('ffeqz5',8), ('ffeqz5',9), ('ffeqz5',10), ('ffeqz5',11);
DROP TABLE IF EXISTS `type_field`;
CREATE TABLE `type_field` (
  `type_name` varchar(191) NOT NULL,
  `key_name` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `holds` varchar(32) NOT NULL,
  `origin` varchar(16) NOT NULL DEFAULT 'declared',
  `folds` varchar(16) NOT NULL DEFAULT 'newest',
  `owner` varchar(16) NOT NULL DEFAULT 'type',
  `required` tinyint(1) NOT NULL DEFAULT '1',
  `one_of` text,
  PRIMARY KEY (`type_name`,`key_name`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `type_field` (`type_name`,`key_name`,`ordinal`,`holds`,`origin`,`folds`,`owner`,`required`,`one_of`) VALUES ('decide-by','decide_by',1,'date','shipped','newest','type',0,NULL), ('entitlement','admits',1,'reference','shipped','newest','type',1,NULL), ('entitlement','tier',4,'text','shipped','newest','type',0,NULL), ('entitlement','valid_from',2,'date','shipped','newest','type',0,NULL), ('entitlement','valid_until',3,'date','shipped','newest','type',0,NULL), ('rhythm','advances_from',5,'text','shipped','newest','kind',0,'due_date,check_in_date'), ('rhythm','cadence_days',4,'number','shipped','newest','kind',0,NULL), ('rhythm','counts_from',6,'date','shipped','newest','kind',0,NULL), ('rhythm','last_check_in',2,'date','shipped','newest','kind',1,NULL), ('rhythm','name',1,'text','shipped','newest','kind',1,NULL), ('rhythm','note',3,'text','shipped','newest','kind',0,NULL), ('rhythm','outcome',7,'text','shipped','newest','kind',0,'ran,skipped,snoozed'), ('runs-out','runs_out',1,'date','shipped','newest','type',0,NULL), ('trip','arrives_at',2,'reference','shipped','newest','type',1,NULL), ('trip','departs_from',1,'reference','shipped','newest','type',1,NULL), ('trip','leaves_on',3,'date','shipped','newest','type',1,NULL), ('trip','returns_on',4,'date','shipped','newest','type',1,NULL), ('upgrade-fixture-type','status',1,'text','declared','newest','type',0,'open,closed'), ('view','asks',3,'text','shipped','newest','kind',0,'overdue'), ('view','selects',1,'text','shipped','newest','kind',1,NULL), ('view','shows',2,'text','shipped','newest','kind',0,NULL);
