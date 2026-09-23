CREATE DATABASE IF NOT EXISTS `jojobot`; USE `jojobot`; 
SET FOREIGN_KEY_CHECKS=0;
SET UNIQUE_CHECKS=0;
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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','qx4paw',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','bfv2nk',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','kjdb3f',NULL,NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','j8qabx',NULL,NULL,NULL);
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
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-09-23T12:28:52.150195803Z'), ('person:upgrade-fixture-person',1,'2026-09-23T12:28:52.320555486Z'), ('place:upgrade-fixture-place',1,'2026-09-23T12:28:52.398927089Z'), ('thing:upgrade-fixture-thing',1,'2026-09-23T12:28:52.560319211Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('bfv2nk','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-09-23','location','kjdb3f',NULL,NULL,NULL,'2026-09-23T12:28:52.723760807Z',NULL,NULL,NULL), ('qx4paw','f1','a recorded thought, live in the room',NULL,'inference','open','active','2026-09-23','connection','j8qabx',NULL,NULL,NULL,'2026-09-23T12:28:52.878228101Z',NULL,NULL,NULL), ('qx4paw','f2','claims the upgrade-fixture-recorder role',NULL,'observation','settled','active','2026-09-23',NULL,NULL,NULL,NULL,NULL,'2026-09-23T12:28:52.985429008Z',NULL,NULL,NULL);
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
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`) VALUES ('bfv2nk','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-09-23','location','kjdb3f',NULL,NULL,'2026-09-23T12:28:52.723760807Z',NULL,'2026-09-23T12:28:52.730047664Z',NULL,NULL), ('qx4paw','f1',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-09-23','connection','j8qabx',NULL,NULL,'2026-09-23T12:28:52.878228101Z',NULL,'2026-09-23T12:28:52.883514186Z',NULL,NULL), ('qx4paw','f2',1,'claims the upgrade-fixture-recorder role',NULL,'observation','settled','active','2026-09-23',NULL,NULL,NULL,NULL,'2026-09-23T12:28:52.985429008Z',NULL,'2026-09-23T12:28:52.988999912Z',NULL,NULL);
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
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('bfv2nk','since',1,'2026-01-01','f1'), ('qx4paw','read_from',1,'this recording run','f2'), ('qx4paw','role/upgrade-fixture-recorder/claimed_at',1,'2026-09-23T12:28:52.979584441Z','f2'), ('qx4paw','role/upgrade-fixture-recorder/holder',1,'bot:assistant','f2');
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
  PRIMARY KEY (`session`,`id`),
  KEY `in_order` (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`) VALUES ('cjenxv','177rwd',1,'2026-09-23T12:28:52.377079154Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing (3)','2026-09-23T12:28:52.702490431Z','add_entity',NULL), ('cjenxv','9dyarj',5,'2026-09-23T12:28:53.507207889Z','recorded the upgrade fixture',NULL,NULL,NULL), ('cjenxv','eefckc',6,'2026-09-23T12:28:53.530898367Z','working\n\nrecorded the upgrade fixture',NULL,NULL,NULL), ('cjenxv','h7g968',2,'2026-09-23T12:28:52.801427438Z','captured facts about: person:upgrade-fixture-person, bot:assistant, … (3)','2026-09-23T12:28:53.072818072Z','capture',NULL), ('cjenxv','vyyt2s',4,'2026-09-23T12:28:53.478342284Z','retired messages: xrpwjx (1)',NULL,'mark_processed',NULL), ('cjenxv','z4n6bz',3,'2026-09-23T12:28:53.13908476Z','posted to mailboxes: assistant, … (3)','2026-09-23T12:28:53.240168671Z','post_message',NULL);
DROP TABLE IF EXISTS `kind`;
CREATE TABLE `kind` (
  `token` varchar(64) NOT NULL,
  `origin` varchar(16) NOT NULL DEFAULT 'declared',
  PRIMARY KEY (`token`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `kind` (`token`,`origin`) VALUES ('bot','shipped'), ('event','shipped'), ('machine','shipped'), ('org','shipped'), ('person','shipped'), ('pet','shipped'), ('place','shipped'), ('project','shipped'), ('rhythm','shipped'), ('session','shipped'), ('thing','shipped'), ('topic','shipped'), ('view','shipped'), ('work','shipped');
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
  PRIMARY KEY (`id`),
  KEY `in_order` (`mailbox`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`) VALUES ('85p2jy','assistant',1,'left new',NULL,'qx4paw','2026-09-23T12:28:53.107322173Z','new',NULL,NULL,0), ('vajnnz','assistant',2,'will be read',NULL,'qx4paw','2026-09-23T12:28:53.160926707Z','read',NULL,NULL,1), ('xrpwjx','assistant',3,'will be processed',NULL,'qx4paw','2026-09-23T12:28:53.211462386Z','processed',NULL,NULL,2);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('vajnnz','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-09-23T12:28:51.412722015Z'), ('0002_journal_entry','2026-09-23T12:28:51.421211814Z'), ('0003_minted','2026-09-23T12:28:51.428838654Z'), ('0004_mailbox','2026-09-23T12:28:51.436114172Z'), ('0005_message','2026-09-23T12:28:51.443881411Z'), ('0006_handover','2026-09-23T12:28:51.45138239Z'), ('0007_drop_minted','2026-09-23T12:28:51.458332118Z'), ('0008_entity','2026-09-23T12:28:51.466819189Z'), ('0009_entity_alias','2026-09-23T12:28:51.47554661Z'), ('0010_fact','2026-09-23T12:28:51.487310203Z'), ('0011_fact_event_metadata','2026-09-23T12:28:51.497886755Z'), ('0012_fact_event_ref','2026-09-23T12:28:51.50953379Z'), ('0013_type_field','2026-09-23T12:28:51.519611072Z'), ('0014_message_delivery','2026-09-23T12:28:51.529302023Z'), ('0015_type_field_origin','2026-09-23T12:28:51.540821257Z'), ('0016_field_write','2026-09-23T12:28:51.552005221Z'), ('0017_type_field_folds','2026-09-23T12:28:51.560986812Z'), ('0018_type_field_holds_wider','2026-09-23T12:28:51.56846737Z'), ('0019_entity_source_wider','2026-09-23T12:28:51.57711284Z'), ('0020_entity_crm_wider','2026-09-23T12:28:51.58487812Z'), ('0021_kind','2026-09-23T12:28:51.592517559Z'), ('0022_type_field_owner','2026-09-23T12:28:51.600820068Z'), ('0023_type_field_required','2026-09-23T12:28:51.609980179Z'), ('0024_rhythm_kind_takes_its_name','2026-09-23T12:28:51.617316819Z'), ('0025_type_field_one_of','2026-09-23T12:28:51.625176208Z'), ('0026_session_timezone','2026-09-23T12:28:51.633388218Z'), ('0027_fact_inserted_at','2026-09-23T12:28:51.641901698Z'), ('0028_fact_stale_after','2026-09-23T12:28:51.649826777Z'), ('0029_fact_by_source','2026-09-23T12:28:51.657632457Z'), ('0030_session_stated_day','2026-09-23T12:28:51.665765596Z'), ('0031_journal_entry_day','2026-09-23T12:28:51.674719617Z'), ('0032_entity_badge','2026-09-23T12:28:51.683299866Z'), ('0033_entity_merged_into','2026-09-23T12:28:51.692263307Z'), ('0034_fact_write','2026-09-23T12:28:51.701885139Z'), ('0035_fact_write_backfill','2026-09-23T12:28:51.709165947Z'), ('0036_fact_write_moment','2026-09-23T12:28:51.718592229Z'), ('0037_fact_happened_at','2026-09-23T12:28:51.729617702Z'), ('0038_fact_write_happened_at','2026-09-23T12:28:51.740544335Z'), ('0039_fact_recorded_at','2026-09-23T12:28:51.751048148Z'), ('0040_fact_write_recorded_at','2026-09-23T12:28:51.762233132Z'), ('0041_session_teaching','2026-09-23T12:28:51.775028056Z'), ('0042_entity_former_handle','2026-09-23T12:28:51.785199048Z'), ('0043_message_sender_mail_waiting','2026-09-23T12:28:51.794646899Z'), ('0044_fact_stands_for','2026-09-23T12:28:51.805244312Z'), ('0045_entity_former_handle_key_add','2026-09-23T12:28:51.843304608Z'), ('0045_entity_former_handle_key_drop','2026-09-23T12:28:51.834890278Z'), ('0045_entity_former_handle_ordinal','2026-09-23T12:28:51.826144717Z'), ('0045_entity_former_handle_width','2026-09-23T12:28:51.816216156Z'), ('0046_fact_status_archived','2026-09-23T12:28:51.849723545Z'), ('0047_fact_write_status_archived','2026-09-23T12:28:51.856091673Z'), ('0048_session_served_chars','2026-09-23T12:28:51.866510435Z'), ('0049_entity_archived','2026-09-23T12:28:51.875174615Z'), ('0050_entity_archived_at','2026-09-23T12:28:51.883862386Z'), ('0051_entity_write','2026-09-23T12:28:51.891545455Z'), ('0052_fact_happened_through','2026-09-23T12:28:51.899898875Z'), ('0053_fact_write_happened_through','2026-09-23T12:28:51.907896214Z'), ('0054_session_write','2026-09-23T12:28:51.915761644Z');
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
  PRIMARY KEY (`id`),
  KEY `by_bot` (`bot`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`) VALUES ('cjenxv','thh2','qx4paw','working','2026-09-23T12:28:52.355294567Z','wrapped',NULL,NULL,10883);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('thh2','claim-direction','2026-09-23T12:28:52.825944547Z'), ('thh2','claim-subject','2026-09-23T12:28:52.822027363Z'), ('thh2','claims','2026-09-23T12:28:52.818236388Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('cjenxv',1), ('cjenxv',2), ('cjenxv',3), ('cjenxv',4), ('cjenxv',5), ('cjenxv',6), ('cjenxv',7), ('cjenxv',8), ('cjenxv',9), ('cjenxv',10), ('cjenxv',11), ('cjenxv',12), ('cjenxv',13), ('cjenxv',14), ('cjenxv',15), ('cjenxv',16), ('cjenxv',17), ('cjenxv',18), ('cjenxv',19), ('cjenxv',20), ('cjenxv',21), ('cjenxv',22), ('cjenxv',23), ('cjenxv',24), ('cjenxv',25), ('cjenxv',26), ('cjenxv',27), ('cjenxv',28);
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
