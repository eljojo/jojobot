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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','pxqxbr',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','kgxkbc',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','tk5rxy',NULL,NULL,NULL), ('thing:recorded-twin','thing','A Spare Copy','test',NULL,NULL,'on-demand','','fy5fjh','thing:upgrade-fixture-thing',NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','eet0r1',NULL,NULL,NULL);
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
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-06T12:09:56.249770547Z'), ('person:upgrade-fixture-person',1,'2026-10-06T12:09:56.542168414Z'), ('place:upgrade-fixture-place',1,'2026-10-06T12:09:56.86770396Z'), ('thing:recorded-twin',1,'2026-10-06T12:09:57.49584681Z'), ('thing:recorded-twin',2,'2026-10-06T12:09:57.775240226Z'), ('thing:upgrade-fixture-thing',1,'2026-10-06T12:09:56.984663226Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('eet0r1','f1','the twin carried a note',NULL,'testimony','settled','active','2026-10-06',NULL,NULL,NULL,NULL,NULL,'2026-10-06T12:09:57.614318806Z',NULL,NULL,NULL), ('eet0r1','f2','recorded as one thing',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,NULL,'2026-10-06T12:09:57.77087425Z',NULL,NULL,NULL), ('kgxkbc','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-06','location','tk5rxy',NULL,NULL,NULL,'2026-10-06T12:09:57.104971155Z',NULL,NULL,NULL), ('kgxkbc','f2','visited the recorded place',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,'kgxkbc','f1','2026-10-06T12:09:57.3091885Z',NULL,NULL,NULL), ('pxqxbr','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,NULL,NULL), ('pxqxbr','f2','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-06','connection','eet0r1',NULL,NULL,NULL,'2026-10-06T12:09:58.13981406Z',NULL,NULL,NULL);
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
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`) VALUES ('eet0r1','f1',1,'the twin carried a note',NULL,'testimony','settled','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:57.614318806Z',NULL,'2026-10-06T12:09:57.618086421Z',NULL,NULL), ('eet0r1','f2',1,'recorded as one thing',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:57.77087425Z',NULL,'2026-10-06T12:09:57.772655072Z',NULL,NULL), ('kgxkbc','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-06','location','tk5rxy',NULL,NULL,'2026-10-06T12:09:57.104971155Z',NULL,'2026-10-06T12:09:57.10853249Z',NULL,NULL), ('kgxkbc','f2',1,'visited the recorded place',NULL,'inference','open','active','2026-10-06',NULL,NULL,'kgxkbc','f1','2026-10-06T12:09:57.3091885Z',NULL,'2026-10-06T12:09:57.312541945Z',NULL,NULL), ('pxqxbr','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:56.460624295Z',NULL,NULL), ('pxqxbr','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:56.691643118Z',NULL,NULL), ('pxqxbr','f1',3,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:56.915714903Z',NULL,NULL), ('pxqxbr','f1',4,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:57.028691711Z',NULL,NULL), ('pxqxbr','f1',5,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:57.175701751Z',NULL,NULL), ('pxqxbr','f1',6,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:57.387680137Z',NULL,NULL), ('pxqxbr','f1',7,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:57.541604537Z',NULL,NULL), ('pxqxbr','f1',8,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:57.673881395Z',NULL,NULL), ('pxqxbr','f1',9,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:57.974737685Z',NULL,NULL), ('pxqxbr','f1',10,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:58.211883978Z',NULL,NULL), ('pxqxbr','f1',11,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:58.35399478Z',NULL,NULL), ('pxqxbr','f1',12,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:58.486706266Z',NULL,NULL), ('pxqxbr','f1',13,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:58.781215436Z',NULL,NULL), ('pxqxbr','f1',14,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:59.163756336Z',NULL,NULL), ('pxqxbr','f1',15,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:59.263989453Z',NULL,NULL), ('pxqxbr','f1',16,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-06',NULL,NULL,NULL,NULL,'2026-10-06T12:09:56.455506553Z',NULL,'2026-10-06T12:09:59.366746817Z',NULL,NULL), ('pxqxbr','f2',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-06','connection','eet0r1',NULL,NULL,'2026-10-06T12:09:58.13981406Z',NULL,'2026-10-06T12:09:58.143082516Z',NULL,NULL);
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
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('eet0r1','merged_from',1,'thing:recorded-twin','f2'), ('kgxkbc','since',1,'2026-01-01','f1'), ('kgxkbc','venue',1,'tk5rxy','f2'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',1,'2026-10-06T12:09:56.448905097Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',2,'2026-10-06T12:09:56.672256869Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',3,'2026-10-06T12:09:56.895670442Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',4,'2026-10-06T12:09:57.011839066Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',5,'2026-10-06T12:09:57.158472365Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',6,'2026-10-06T12:09:57.366277605Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',7,'2026-10-06T12:09:57.524223381Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',8,'2026-10-06T12:09:57.656976569Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',9,'2026-10-06T12:09:57.952542541Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',10,'2026-10-06T12:09:58.193913552Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',11,'2026-10-06T12:09:58.335032532Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',12,'2026-10-06T12:09:58.467435002Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',13,'2026-10-06T12:09:58.762301112Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',14,'2026-10-06T12:09:59.140485513Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',15,'2026-10-06T12:09:59.238165334Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/claimed_at',16,'1970-01-01T00:00:00Z','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',1,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',2,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',3,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',4,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',5,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',6,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',7,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',8,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',9,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',10,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',11,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',12,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',13,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',14,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',15,'y47m','f1'), ('pxqxbr','role/upgrade-fixture-recorder/holder',16,'y47m','f1');
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
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`) VALUES ('jegjmd','6he58v',6,'2026-10-06T12:09:59.238165334Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL), ('jegjmd','aajc4y',4,'2026-10-06T12:09:58.416080583Z','posted to mailboxes: assistant, … (3)','2026-10-06T12:09:58.844222159Z','post_message',NULL,NULL), ('jegjmd','cnr2na',1,'2026-10-06T12:09:56.843899475Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing, thing:recorded-twin (4)','2026-10-06T12:09:57.593559055Z','add_entity',NULL,NULL), ('jegjmd','j9r4pe',5,'2026-10-06T12:09:59.223181171Z','retired messages: khjya3 (1)',NULL,'mark_processed',NULL,NULL), ('jegjmd','jwdmrw',3,'2026-10-06T12:09:58.035443965Z','merged entities: thing:recorded-twin (1)',NULL,'merge_entities',NULL,NULL), ('jegjmd','zxy2q8',2,'2026-10-06T12:09:57.228412999Z','captured facts about: person:upgrade-fixture-person, thing:recorded-twin, bot:assistant, … (4)','2026-10-06T12:09:58.273982131Z','capture',NULL,NULL);
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
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('5v758k','assistant',2,'will be read',NULL,'pxqxbr','2026-10-06T12:09:58.443385974Z','read',NULL,NULL,1,'jegjmd',NULL,NULL,NULL), ('hqv84n','assistant',1,'left new',NULL,'pxqxbr','2026-10-06T12:09:58.309684824Z','new',NULL,NULL,0,'jegjmd',NULL,NULL,NULL), ('khjya3','assistant',3,'will be processed',NULL,'pxqxbr','2026-10-06T12:09:58.639573653Z','processed',NULL,NULL,2,'jegjmd',NULL,NULL,NULL);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('5v758k','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-06T12:09:55.423450152Z'), ('0002_journal_entry','2026-10-06T12:09:55.432519713Z'), ('0003_minted','2026-10-06T12:09:55.450290796Z'), ('0004_mailbox','2026-10-06T12:09:55.457691523Z'), ('0005_message','2026-10-06T12:09:55.465364891Z'), ('0006_handover','2026-10-06T12:09:55.472098547Z'), ('0007_drop_minted','2026-10-06T12:09:55.479030294Z'), ('0008_entity','2026-10-06T12:09:55.487462054Z'), ('0009_entity_alias','2026-10-06T12:09:55.495724474Z'), ('0010_fact','2026-10-06T12:09:55.503620622Z'), ('0011_fact_event_metadata','2026-10-06T12:09:55.510805379Z'), ('0012_fact_event_ref','2026-10-06T12:09:55.518061166Z'), ('0013_type_field','2026-10-06T12:09:55.525058643Z'), ('0014_message_delivery','2026-10-06T12:09:55.53254957Z'), ('0015_type_field_origin','2026-10-06T12:09:55.539942168Z'), ('0016_field_write','2026-10-06T12:09:55.546924634Z'), ('0017_type_field_folds','2026-10-06T12:09:55.554149511Z'), ('0018_type_field_holds_wider','2026-10-06T12:09:55.561098418Z'), ('0019_entity_source_wider','2026-10-06T12:09:55.568580246Z'), ('0020_entity_crm_wider','2026-10-06T12:09:55.575882503Z'), ('0021_kind','2026-10-06T12:09:55.58318641Z'), ('0022_type_field_owner','2026-10-06T12:09:55.591076359Z'), ('0023_type_field_required','2026-10-06T12:09:55.599721279Z'), ('0024_rhythm_kind_takes_its_name','2026-10-06T12:09:55.606154824Z'), ('0025_type_field_one_of','2026-10-06T12:09:55.614220614Z'), ('0026_session_timezone','2026-10-06T12:09:55.622260243Z'), ('0027_fact_inserted_at','2026-10-06T12:09:55.630801362Z'), ('0028_fact_stale_after','2026-10-06T12:09:55.638430321Z'), ('0029_fact_by_source','2026-10-06T12:09:55.646028579Z'), ('0030_session_stated_day','2026-10-06T12:09:55.653898657Z'), ('0031_journal_entry_day','2026-10-06T12:09:55.662009387Z'), ('0032_entity_badge','2026-10-06T12:09:55.687467276Z'), ('0033_entity_merged_into','2026-10-06T12:09:55.696171597Z'), ('0034_fact_write','2026-10-06T12:09:55.705315909Z'), ('0035_fact_write_backfill','2026-10-06T12:09:55.712013955Z'), ('0036_fact_write_moment','2026-10-06T12:09:55.719716663Z'), ('0037_fact_happened_at','2026-10-06T12:09:55.751308748Z'), ('0038_fact_write_happened_at','2026-10-06T12:09:55.778178161Z'), ('0039_fact_recorded_at','2026-10-06T12:09:55.810609548Z'), ('0040_fact_write_recorded_at','2026-10-06T12:09:55.833282672Z'), ('0041_session_teaching','2026-10-06T12:09:55.912521619Z'), ('0042_entity_former_handle','2026-10-06T12:09:55.928348206Z'), ('0043_message_sender_mail_waiting','2026-10-06T12:09:55.936787826Z'), ('0044_fact_stands_for','2026-10-06T12:09:55.945077925Z'), ('0045_entity_former_handle_key_add','2026-10-06T12:09:55.975746529Z'), ('0045_entity_former_handle_key_drop','2026-10-06T12:09:55.96829166Z'), ('0045_entity_former_handle_ordinal','2026-10-06T12:09:55.960898623Z'), ('0045_entity_former_handle_width','2026-10-06T12:09:55.952875375Z'), ('0046_fact_status_archived','2026-10-06T12:09:55.982424964Z'), ('0047_fact_write_status_archived','2026-10-06T12:09:55.989243181Z'), ('0048_session_served_chars','2026-10-06T12:09:55.997297Z'), ('0049_entity_archived','2026-10-06T12:09:56.00605473Z'), ('0050_entity_archived_at','2026-10-06T12:09:56.015814193Z'), ('0051_entity_write','2026-10-06T12:09:56.025115915Z'), ('0052_fact_happened_through','2026-10-06T12:09:56.037930516Z'), ('0053_fact_write_happened_through','2026-10-06T12:09:56.047776329Z'), ('0054_session_write','2026-10-06T12:09:56.055837268Z'), ('0055_message_posted_by_session','2026-10-06T12:09:56.064575969Z'), ('0056_session_stated_day','2026-10-06T12:09:56.073288469Z'), ('0057_message_quarantined_by','2026-10-06T12:09:56.081765729Z'), ('0058_message_quarantined_at','2026-10-06T12:09:56.089907888Z'), ('0059_message_quarantine_reason','2026-10-06T12:09:56.097941748Z'), ('0060_journal_entry_closing_focus','2026-10-06T12:09:56.106322767Z'), ('0061_displaced_type_field','2026-10-06T12:09:56.114620807Z');
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
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`) VALUES ('jegjmd','y47m','pxqxbr','claimed the upgrade-fixture-recorder role','2026-10-06T12:09:56.512962521Z','wrapped',NULL,NULL,13972,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('y47m','claim-direction','2026-10-06T12:09:57.249858872Z'), ('y47m','claim-subject','2026-10-06T12:09:57.246469907Z'), ('y47m','claims','2026-10-06T12:09:57.242924122Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('jegjmd',1), ('jegjmd',2), ('jegjmd',3), ('jegjmd',4), ('jegjmd',5), ('jegjmd',6), ('jegjmd',7), ('jegjmd',8), ('jegjmd',9), ('jegjmd',10), ('jegjmd',11), ('jegjmd',12), ('jegjmd',13), ('jegjmd',14), ('jegjmd',15);
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
INSERT INTO `type_field` (`type_name`,`key_name`,`ordinal`,`holds`,`origin`,`folds`,`owner`,`required`,`one_of`) VALUES ('decide-by','decide_by',1,'date','shipped','newest','type',0,NULL), ('entitlement','admits',1,'reference','shipped','newest','type',1,NULL), ('entitlement','tier',4,'text','shipped','newest','type',0,NULL), ('entitlement','valid_from',2,'date','shipped','newest','type',0,NULL), ('entitlement','valid_until',3,'date','shipped','newest','type',0,NULL), ('promise','promised_for',1,'date','declared','newest','type',0,NULL), ('rhythm','advances_from',5,'text','shipped','newest','kind',0,'due_date,check_in_date'), ('rhythm','cadence_days',4,'number','shipped','newest','kind',0,NULL), ('rhythm','counts_from',6,'date','shipped','newest','kind',0,NULL), ('rhythm','last_check_in',2,'date','shipped','newest','kind',1,NULL), ('rhythm','name',1,'text','shipped','newest','kind',1,NULL), ('rhythm','note',3,'text','shipped','newest','kind',0,NULL), ('rhythm','outcome',7,'text','shipped','newest','kind',0,'ran,skipped,snoozed'), ('runs-out','runs_out',1,'date','shipped','newest','type',0,NULL), ('trip','arrives_at',2,'reference','shipped','newest','type',1,NULL), ('trip','departs_from',1,'reference','shipped','newest','type',1,NULL), ('trip','leaves_on',3,'date','shipped','newest','type',1,NULL), ('trip','returns_on',4,'date','shipped','newest','type',1,NULL), ('upgrade-fixture-type','status',1,'text','declared','newest','type',0,'open,closed'), ('upgrade-fixture-visit','venue',1,'reference:place','declared','newest','type',0,NULL), ('view','asks',3,'text','shipped','newest','kind',0,'overdue'), ('view','selects',1,'text','shipped','newest','kind',1,NULL), ('view','shows',2,'text','shipped','newest','kind',0,NULL);
