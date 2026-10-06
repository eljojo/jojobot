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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','fv94yt',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','q59h7r',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','hznc49',NULL,NULL,NULL), ('thing:recorded-twin','thing','A Spare Copy','test',NULL,NULL,'on-demand','','ge36mz','thing:upgrade-fixture-thing',NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','03kc14',NULL,NULL,NULL);
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
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-06T08:24:58.361301627Z'), ('person:upgrade-fixture-person',1,'2026-10-06T08:24:58.659445441Z'), ('place:upgrade-fixture-place',1,'2026-10-06T08:24:58.900229493Z'), ('thing:recorded-twin',1,'2026-10-06T08:24:59.819719567Z'), ('thing:recorded-twin',2,'2026-10-06T08:25:00.346667038Z'), ('thing:upgrade-fixture-thing',1,'2026-10-06T08:24:59.048375808Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('03kc14','f1','the twin carried a note',NULL,'testimony','settled','active','2026-10-06',NULL,NULL,NULL,NULL,NULL,'2026-10-06T08:25:00.070615353Z',NULL,NULL,NULL), ('03kc14','f2','recorded as one thing',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,NULL,'2026-10-06T08:25:00.343266059Z',NULL,NULL,NULL), ('fv94yt','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,NULL,NULL), ('fv94yt','f2','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-06','connection','03kc14',NULL,NULL,NULL,'2026-10-06T08:25:00.594728275Z',NULL,NULL,NULL), ('q59h7r','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-06','location','hznc49',NULL,NULL,NULL,'2026-10-06T08:24:59.186492424Z',NULL,NULL,NULL), ('q59h7r','f2','visited the recorded place',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,'q59h7r','f1','2026-10-06T08:24:59.524508339Z',NULL,NULL,NULL);
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
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`) VALUES ('03kc14','f1',1,'the twin carried a note',NULL,'testimony','settled','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:25:00.070615353Z',NULL,'2026-10-06T08:25:00.074609264Z',NULL,NULL), ('03kc14','f2',1,'recorded as one thing',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:25:00.343266059Z',NULL,'2026-10-06T08:25:00.344805688Z',NULL,NULL), ('fv94yt','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:24:58.570417297Z',NULL,NULL), ('fv94yt','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:24:58.713755287Z',NULL,NULL), ('fv94yt','f1',3,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:24:58.967924994Z',NULL,NULL), ('fv94yt','f1',4,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:24:59.100103303Z',NULL,NULL), ('fv94yt','f1',5,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:24:59.263791465Z',NULL,NULL), ('fv94yt','f1',6,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:24:59.703702114Z',NULL,NULL), ('fv94yt','f1',7,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:24:59.871883674Z',NULL,NULL), ('fv94yt','f1',8,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:00.142521769Z',NULL,NULL), ('fv94yt','f1',9,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:00.443336363Z',NULL,NULL), ('fv94yt','f1',10,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:00.6768924Z',NULL,NULL), ('fv94yt','f1',11,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:00.829807601Z',NULL,NULL), ('fv94yt','f1',12,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:00.971511524Z',NULL,NULL), ('fv94yt','f1',13,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:01.174434793Z',NULL,NULL), ('fv94yt','f1',14,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:01.342467032Z',NULL,NULL), ('fv94yt','f1',15,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:01.456413405Z',NULL,NULL), ('fv94yt','f1',16,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T08:24:58.564932401Z',NULL,'2026-10-06T08:25:01.571617549Z',NULL,NULL), ('fv94yt','f2',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-06','connection','03kc14',NULL,NULL,'2026-10-06T08:25:00.594728275Z',NULL,'2026-10-06T08:25:00.599477075Z',NULL,NULL), ('q59h7r','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-06','location','hznc49',NULL,NULL,'2026-10-06T08:24:59.186492424Z',NULL,'2026-10-06T08:24:59.190616058Z',NULL,NULL), ('q59h7r','f2',1,'visited the recorded place',NULL,'inference','open','active','2026-10-06',NULL,NULL,'q59h7r','f1','2026-10-06T08:24:59.524508339Z',NULL,'2026-10-06T08:24:59.528565203Z',NULL,NULL);
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
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('03kc14','merged_from',1,'thing:recorded-twin','f2'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',1,'2026-10-06T08:24:58.558269234Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',2,'2026-10-06T08:24:58.692189305Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',3,'2026-10-06T08:24:58.944315499Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',4,'2026-10-06T08:24:59.078482701Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',5,'2026-10-06T08:24:59.243155413Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',6,'2026-10-06T08:24:59.682573255Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',7,'2026-10-06T08:24:59.850474285Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',8,'2026-10-06T08:25:00.12020297Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',9,'2026-10-06T08:25:00.422675584Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',10,'2026-10-06T08:25:00.657292611Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',11,'2026-10-06T08:25:00.808999163Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',12,'2026-10-06T08:25:00.948409115Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',13,'2026-10-06T08:25:01.154651254Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',14,'2026-10-06T08:25:01.322060155Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',15,'2026-10-06T08:25:01.426524012Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/claimed_at',16,'1970-01-01T00:00:00Z','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',1,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',2,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',3,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',4,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',5,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',6,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',7,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',8,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',9,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',10,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',11,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',12,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',13,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',14,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',15,'6x12','f1'), ('fv94yt','role/upgrade-fixture-recorder/holder',16,'6x12','f1'), ('q59h7r','since',1,'2026-01-01','f1'), ('q59h7r','venue',1,'hznc49','f2');
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
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`) VALUES ('rtx65c','86bjez',6,'2026-10-06T08:25:01.426524012Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL), ('rtx65c','btdcpa',2,'2026-10-06T08:24:59.426098495Z','captured facts about: person:upgrade-fixture-person, thing:recorded-twin, bot:assistant, … (4)','2026-10-06T08:25:00.742741156Z','capture',NULL,NULL), ('rtx65c','c2qneg',3,'2026-10-06T08:25:00.50523232Z','merged entities: thing:recorded-twin (1)',NULL,'merge_entities',NULL,NULL), ('rtx65c','nbvcmw',5,'2026-10-06T08:25:01.409169248Z','retired messages: ygk8h6 (1)',NULL,'mark_processed',NULL,NULL), ('rtx65c','qv093s',4,'2026-10-06T08:25:00.894033108Z','posted to mailboxes: assistant, … (3)','2026-10-06T08:25:01.239235969Z','post_message',NULL,NULL), ('rtx65c','vbsp6y',1,'2026-10-06T08:24:58.871584013Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing, thing:recorded-twin (4)','2026-10-06T08:25:00.047268405Z','add_entity',NULL,NULL);
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
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('2pxf0z','assistant',1,'left new',NULL,'fv94yt','2026-10-06T08:25:00.780688534Z','new',NULL,NULL,0,'rtx65c',NULL,NULL,NULL), ('v44g8n','assistant',2,'will be read',NULL,'fv94yt','2026-10-06T08:25:00.924242417Z','read',NULL,NULL,1,'rtx65c',NULL,NULL,NULL), ('ygk8h6','assistant',3,'will be processed',NULL,'fv94yt','2026-10-06T08:25:01.128634795Z','processed',NULL,NULL,2,'rtx65c',NULL,NULL,NULL);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('v44g8n','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-06T08:24:57.634008644Z'), ('0002_journal_entry','2026-10-06T08:24:57.644183784Z'), ('0003_minted','2026-10-06T08:24:57.653041784Z'), ('0004_mailbox','2026-10-06T08:24:57.662012364Z'), ('0005_message','2026-10-06T08:24:57.671060042Z'), ('0006_handover','2026-10-06T08:24:57.679333162Z'), ('0007_drop_minted','2026-10-06T08:24:57.686908809Z'), ('0008_entity','2026-10-06T08:24:57.696254619Z'), ('0009_entity_alias','2026-10-06T08:24:57.70730567Z'), ('0010_fact','2026-10-06T08:24:57.718362773Z'), ('0011_fact_event_metadata','2026-10-06T08:24:57.728849704Z'), ('0012_fact_event_ref','2026-10-06T08:24:57.738756304Z'), ('0013_type_field','2026-10-06T08:24:57.751024517Z'), ('0014_message_delivery','2026-10-06T08:24:57.759459235Z'), ('0015_type_field_origin','2026-10-06T08:24:57.767746515Z'), ('0016_field_write','2026-10-06T08:24:57.776332873Z'), ('0017_type_field_folds','2026-10-06T08:24:57.784749502Z'), ('0018_type_field_holds_wider','2026-10-06T08:24:57.79289461Z'), ('0019_entity_source_wider','2026-10-06T08:24:57.80126922Z'), ('0020_entity_crm_wider','2026-10-06T08:24:57.808741677Z'), ('0021_kind','2026-10-06T08:24:57.816643446Z'), ('0022_type_field_owner','2026-10-06T08:24:57.825691425Z'), ('0023_type_field_required','2026-10-06T08:24:57.834735055Z'), ('0024_rhythm_kind_takes_its_name','2026-10-06T08:24:57.841835232Z'), ('0025_type_field_one_of','2026-10-06T08:24:57.851285882Z'), ('0026_session_timezone','2026-10-06T08:24:57.860416692Z'), ('0027_fact_inserted_at','2026-10-06T08:24:57.870569322Z'), ('0028_fact_stale_after','2026-10-06T08:24:57.879875701Z'), ('0029_fact_by_source','2026-10-06T08:24:57.889064502Z'), ('0030_session_stated_day','2026-10-06T08:24:57.898105801Z'), ('0031_journal_entry_day','2026-10-06T08:24:57.907032811Z'), ('0032_entity_badge','2026-10-06T08:24:57.916817171Z'), ('0033_entity_merged_into','2026-10-06T08:24:57.92591547Z'), ('0034_fact_write','2026-10-06T08:24:57.935818331Z'), ('0035_fact_write_backfill','2026-10-06T08:24:57.946426532Z'), ('0036_fact_write_moment','2026-10-06T08:24:57.955321181Z'), ('0037_fact_happened_at','2026-10-06T08:24:57.964532081Z'), ('0038_fact_write_happened_at','2026-10-06T08:24:57.974075741Z'), ('0039_fact_recorded_at','2026-10-06T08:24:57.98326739Z'), ('0040_fact_write_recorded_at','2026-10-06T08:24:57.993909101Z'), ('0041_session_teaching','2026-10-06T08:24:58.005257144Z'), ('0042_entity_former_handle','2026-10-06T08:24:58.014268143Z'), ('0043_message_sender_mail_waiting','2026-10-06T08:24:58.023274172Z'), ('0044_fact_stands_for','2026-10-06T08:24:58.032438432Z'), ('0045_entity_former_handle_key_add','2026-10-06T08:24:58.066169837Z'), ('0045_entity_former_handle_key_drop','2026-10-06T08:24:58.057718448Z'), ('0045_entity_former_handle_ordinal','2026-10-06T08:24:58.04936094Z'), ('0045_entity_former_handle_width','2026-10-06T08:24:58.04071145Z'), ('0046_fact_status_archived','2026-10-06T08:24:58.073462575Z'), ('0047_fact_write_status_archived','2026-10-06T08:24:58.080399372Z'), ('0048_session_served_chars','2026-10-06T08:24:58.089450522Z'), ('0049_entity_archived','2026-10-06T08:24:58.099070852Z'), ('0050_entity_archived_at','2026-10-06T08:24:58.108074262Z'), ('0051_entity_write','2026-10-06T08:24:58.11668679Z'), ('0052_fact_happened_through','2026-10-06T08:24:58.12591562Z'), ('0053_fact_write_happened_through','2026-10-06T08:24:58.13532265Z'), ('0054_session_write','2026-10-06T08:24:58.143519518Z'), ('0055_message_posted_by_session','2026-10-06T08:24:58.153152699Z'), ('0056_session_stated_day','2026-10-06T08:24:58.161620578Z'), ('0057_message_quarantined_by','2026-10-06T08:24:58.170366897Z'), ('0058_message_quarantined_at','2026-10-06T08:24:58.179213306Z'), ('0059_message_quarantine_reason','2026-10-06T08:24:58.188533696Z'), ('0060_journal_entry_closing_focus','2026-10-06T08:24:58.197263625Z'), ('0061_displaced_type_field','2026-10-06T08:24:58.206143035Z');
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
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`) VALUES ('rtx65c','6x12','fv94yt','claimed the upgrade-fixture-recorder role','2026-10-06T08:24:58.627261296Z','wrapped',NULL,NULL,13449,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('6x12','claim-direction','2026-10-06T08:24:59.453441354Z'), ('6x12','claim-subject','2026-10-06T08:24:59.44929292Z'), ('6x12','claims','2026-10-06T08:24:59.444858655Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('rtx65c',1), ('rtx65c',2), ('rtx65c',3), ('rtx65c',4), ('rtx65c',5), ('rtx65c',6), ('rtx65c',7), ('rtx65c',8), ('rtx65c',9), ('rtx65c',10), ('rtx65c',11), ('rtx65c',12), ('rtx65c',13), ('rtx65c',14), ('rtx65c',15);
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
INSERT INTO `type_field` (`type_name`,`key_name`,`ordinal`,`holds`,`origin`,`folds`,`owner`,`required`,`one_of`) VALUES ('decide-by','decide_by',1,'date','shipped','newest','type',0,NULL), ('entitlement','admits',1,'reference','shipped','newest','type',1,NULL), ('entitlement','tier',4,'text','shipped','newest','type',0,NULL), ('entitlement','valid_from',2,'date','shipped','newest','type',0,NULL), ('entitlement','valid_until',3,'date','shipped','newest','type',0,NULL), ('rhythm','advances_from',5,'text','shipped','newest','kind',0,'due_date,check_in_date'), ('rhythm','cadence_days',4,'number','shipped','newest','kind',0,NULL), ('rhythm','counts_from',6,'date','shipped','newest','kind',0,NULL), ('rhythm','last_check_in',2,'date','shipped','newest','kind',1,NULL), ('rhythm','name',1,'text','shipped','newest','kind',1,NULL), ('rhythm','note',3,'text','shipped','newest','kind',0,NULL), ('rhythm','outcome',7,'text','shipped','newest','kind',0,'ran,skipped,snoozed'), ('runs-out','runs_out',1,'date','shipped','newest','type',0,NULL), ('trip','arrives_at',2,'reference','shipped','newest','type',1,NULL), ('trip','departs_from',1,'reference','shipped','newest','type',1,NULL), ('trip','leaves_on',3,'date','shipped','newest','type',1,NULL), ('trip','returns_on',4,'date','shipped','newest','type',1,NULL), ('upgrade-fixture-type','status',1,'text','declared','newest','type',0,'open,closed'), ('upgrade-fixture-visit','venue',1,'reference:place','declared','newest','type',0,NULL), ('view','asks',3,'text','shipped','newest','kind',0,'overdue'), ('view','selects',1,'text','shipped','newest','kind',1,NULL), ('view','shows',2,'text','shipped','newest','kind',0,NULL);
