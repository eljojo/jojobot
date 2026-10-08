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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','17t9nq',NULL,NULL,NULL), ('bot:upgrade-fixture-heavy','bot','A Recorded Heavy Bot','test',NULL,NULL,'on-demand','xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx','jyhc7b',NULL,NULL,NULL), ('bot:upgrade-fixture-lead','bot','A Recorded Lead','test',NULL,NULL,'on-demand','','zxttg0',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','8vzbr1',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','6z62bt',NULL,NULL,NULL), ('project:upgrade-fixture-project','project','A Recorded Project','test',NULL,NULL,'on-demand','','3vgp04',NULL,NULL,NULL), ('thing:blue-kite','thing','A Recorded Spare Kite','test',NULL,NULL,'on-demand','','9924zm',NULL,NULL,NULL), ('thing:recorded-twin','thing','A Spare Copy','test',NULL,NULL,'on-demand','','vx4c6k','thing:upgrade-fixture-thing',NULL,NULL), ('thing:red-kite','thing','A Recorded Kite','test',NULL,NULL,'on-demand','','y6gddq',NULL,NULL,NULL), ('thing:upgrade-fixture-archived','thing','A Recorded Archive','test',NULL,NULL,'on-demand','','4phs1m',NULL,'recorded as no longer wanted','2026-10-08T14:02:08.286987191Z'), ('thing:upgrade-fixture-namesake','thing','A Later Heir','test',NULL,NULL,'on-demand','','fmknnq',NULL,NULL,NULL), ('thing:upgrade-fixture-successor','thing','A Recorded Namesake','test',NULL,NULL,'on-demand','','att7wx',NULL,NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','j68x6y',NULL,NULL,NULL), ('topic:instance','topic','This instance','test',NULL,NULL,'on-demand','','76j2qm',NULL,NULL,NULL), ('work:upgrade-fixture-prior','work','A Recorded Prior Task','test',NULL,'3vgp04','on-demand','','tpmbn2',NULL,NULL,NULL), ('work:upgrade-fixture-task','work','A Recorded Task','test',NULL,'3vgp04','on-demand','','3cfj26',NULL,NULL,NULL);
DROP TABLE IF EXISTS `entity_alias`;
CREATE TABLE `entity_alias` (
  `entity` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `alias` text NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_alias` (`entity`,`ordinal`,`alias`) VALUES ('j68x6y',1,'A Spare Copy');
DROP TABLE IF EXISTS `entity_former_handle`;
CREATE TABLE `entity_former_handle` (
  `former_handle` varchar(191) NOT NULL,
  `badge` varchar(16) NOT NULL,
  `changed_at` varchar(16) NOT NULL,
  `ordinal` int NOT NULL DEFAULT '1',
  PRIMARY KEY (`former_handle`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_former_handle` (`former_handle`,`badge`,`changed_at`,`ordinal`) VALUES ('thing:upgrade-fixture-namesake','att7wx','2026-10-08',1);
DROP TABLE IF EXISTS `entity_write`;
CREATE TABLE `entity_write` (
  `entity` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `written_at` varchar(40) NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-08T14:02:01.758545325Z'), ('bot:upgrade-fixture-heavy',1,'2026-10-08T14:02:08.555432719Z'), ('bot:upgrade-fixture-heavy',2,'2026-10-08T14:02:08.885498661Z'), ('bot:upgrade-fixture-heavy',3,'2026-10-08T14:02:09.090903579Z'), ('bot:upgrade-fixture-lead',1,'2026-10-08T14:02:05.813070645Z'), ('person:upgrade-fixture-person',1,'2026-10-08T14:02:02.206029514Z'), ('place:upgrade-fixture-place',1,'2026-10-08T14:02:02.388243659Z'), ('project:upgrade-fixture-project',1,'2026-10-08T14:02:04.02956944Z'), ('thing:blue-kite',1,'2026-10-08T14:02:03.602933823Z'), ('thing:recorded-twin',1,'2026-10-08T14:02:02.925920574Z'), ('thing:recorded-twin',2,'2026-10-08T14:02:03.155286284Z'), ('thing:red-kite',1,'2026-10-08T14:02:03.292444337Z'), ('thing:upgrade-fixture-archived',1,'2026-10-08T14:02:08.218393269Z'), ('thing:upgrade-fixture-archived',2,'2026-10-08T14:02:08.288706605Z'), ('thing:upgrade-fixture-namesake',1,'2026-10-08T14:02:07.246579285Z'), ('thing:upgrade-fixture-namesake',2,'2026-10-08T14:02:07.575445501Z'), ('thing:upgrade-fixture-successor',1,'2026-10-08T14:02:07.333312749Z'), ('thing:upgrade-fixture-thing',1,'2026-10-08T14:02:02.467075662Z'), ('thing:upgrade-fixture-thing',2,'2026-10-08T14:02:03.148686277Z'), ('topic:instance',1,'2026-10-08T14:02:06.525659174Z'), ('work:upgrade-fixture-prior',1,'2026-10-08T14:02:04.111110304Z'), ('work:upgrade-fixture-task',1,'2026-10-08T14:02:04.294119586Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('17t9nq','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:02.084240232Z',NULL,NULL,NULL), ('17t9nq','f2','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-08','connection','j68x6y',NULL,NULL,NULL,'2026-10-08T14:02:05.103472409Z',NULL,NULL,NULL), ('17t9nq','f3','the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:05.346675606Z',NULL,NULL,NULL), ('17t9nq','f4','the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:06.081706353Z',NULL,NULL,NULL), ('17t9nq','f5','the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:06.360230668Z',NULL,NULL,NULL), ('17t9nq','f6','claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:09.752542821Z',NULL,NULL,NULL), ('3cfj26','f1','the recorded task is in review',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:04.682727458Z',NULL,NULL,NULL), ('3vgp04','f1','the recorded project is under way',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:04.38187055Z',NULL,NULL,NULL), ('6z62bt','f1','the recorded place opens at nine',NULL,'observation','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:05.62591452Z',NULL,NULL,NULL), ('76j2qm','f1','the instance works in one zone',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:06.612572698Z',NULL,NULL,NULL), ('8vzbr1','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-08','location','6z62bt',NULL,NULL,NULL,'2026-10-08T14:02:02.549844621Z',NULL,NULL,NULL), ('8vzbr1','f2','visited the recorded place',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,'8vzbr1','f1','2026-10-08T14:02:02.762432417Z',NULL,NULL,NULL), ('9924zm','f1','where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:03.68386023Z',NULL,NULL,NULL), ('j68x6y','f1','the twin carried a note',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:03.009896039Z',NULL,NULL,NULL), ('j68x6y','f2','recorded as one thing',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:03.149656554Z',NULL,NULL,NULL), ('j68x6y','f3','a day kept under decide_by',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:04.828396442Z',NULL,NULL,NULL), ('j68x6y','f4','points at a handle two things have worn',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:07.756543975Z',NULL,NULL,NULL), ('j68x6y','f5','keeps company with a person and a place',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:07.908874662Z',NULL,NULL,NULL), ('jyhc7b','f1','the heavy bot keeps its one rule',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:09.207121892Z',NULL,NULL,NULL), ('tpmbn2','f1','the prior task is finished',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:04.537169773Z',NULL,NULL,NULL), ('y6gddq','f1','where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T14:02:03.470581416Z',NULL,NULL,NULL);
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
  `session` varchar(64),
  PRIMARY KEY (`entity`,`fact_id`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`,`session`) VALUES ('17t9nq','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:02.084240232Z',NULL,'2026-10-08T14:02:02.091331188Z',NULL,NULL,NULL), ('17t9nq','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:02.084240232Z',NULL,'2026-10-08T14:02:09.522814901Z',NULL,NULL,NULL), ('17t9nq','f2',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-08','connection','j68x6y',NULL,NULL,'2026-10-08T14:02:05.103472409Z',NULL,'2026-10-08T14:02:05.108517573Z',NULL,NULL,'qkqb'), ('17t9nq','f3',1,'the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:05.346675606Z',NULL,'2026-10-08T14:02:05.351572451Z',NULL,NULL,'qkqb'), ('17t9nq','f4',1,'the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:06.081706353Z',NULL,'2026-10-08T14:02:06.086433598Z',NULL,NULL,'p60v'), ('17t9nq','f5',1,'the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:06.360230668Z',NULL,'2026-10-08T14:02:06.364914784Z',NULL,NULL,'qkqb'), ('17t9nq','f6',1,'claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:09.752542821Z',NULL,'2026-10-08T14:02:09.756932287Z',NULL,NULL,NULL), ('3cfj26','f1',1,'the recorded task is in review',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:04.682727458Z',NULL,'2026-10-08T14:02:04.689178707Z',NULL,NULL,'qkqb'), ('3vgp04','f1',1,'the recorded project is under way',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:04.38187055Z',NULL,'2026-10-08T14:02:04.38822695Z',NULL,NULL,'qkqb'), ('6z62bt','f1',1,'the recorded place opens at nine',NULL,'observation','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:05.62591452Z',NULL,'2026-10-08T14:02:05.631606792Z',NULL,NULL,'qkqb'), ('76j2qm','f1',1,'the instance works in one zone',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:06.612572698Z',NULL,'2026-10-08T14:02:06.617318082Z',NULL,NULL,'qkqb'), ('8vzbr1','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-08','location','6z62bt',NULL,NULL,'2026-10-08T14:02:02.549844621Z',NULL,'2026-10-08T14:02:02.555330683Z',NULL,NULL,'qkqb'), ('8vzbr1','f2',1,'visited the recorded place',NULL,'inference','open','active','2026-10-08',NULL,NULL,'8vzbr1','f1','2026-10-08T14:02:02.762432417Z',NULL,'2026-10-08T14:02:02.769900882Z',NULL,NULL,'qkqb'), ('9924zm','f1',1,'where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:03.68386023Z',NULL,'2026-10-08T14:02:03.690956455Z',NULL,NULL,'qkqb'), ('j68x6y','f1',1,'the twin carried a note',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:03.009896039Z',NULL,'2026-10-08T14:02:03.014764994Z',NULL,NULL,'qkqb'), ('j68x6y','f2',1,'recorded as one thing',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:03.149656554Z',NULL,'2026-10-08T14:02:03.151751336Z',NULL,NULL,NULL), ('j68x6y','f3',1,'a day kept under decide_by',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:04.828396442Z',NULL,'2026-10-08T14:02:04.833874995Z',NULL,NULL,'qkqb'), ('j68x6y','f4',1,'points at a handle two things have worn',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:07.756543975Z',NULL,'2026-10-08T14:02:07.76151429Z',NULL,NULL,'qkqb'), ('j68x6y','f5',1,'keeps company with a person and a place',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:07.908874662Z',NULL,'2026-10-08T14:02:07.913241538Z',NULL,NULL,'qkqb'), ('jyhc7b','f1',1,'the heavy bot keeps its one rule',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:09.207121892Z',NULL,'2026-10-08T14:02:09.211773466Z',NULL,NULL,'qkqb'), ('tpmbn2','f1',1,'the prior task is finished',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:04.537169773Z',NULL,'2026-10-08T14:02:04.545370766Z',NULL,NULL,'qkqb'), ('y6gddq','f1',1,'where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T14:02:03.470581416Z',NULL,'2026-10-08T14:02:03.475603379Z',NULL,NULL,'qkqb');
DROP TABLE IF EXISTS `field_link`;
CREATE TABLE `field_link` (
  `entity` varchar(191) NOT NULL,
  `key` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `target` varchar(191) NOT NULL,
  PRIMARY KEY (`entity`,`key`,`ordinal`,`target`),
  KEY `by_target` (`target`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `field_link` (`entity`,`key`,`ordinal`,`target`) VALUES ('17t9nq','pairs_with',1,'zxttg0'), ('17t9nq','reports_to',1,'zxttg0'), ('3cfj26','depends_on',1,'tpmbn2'), ('3cfj26','owner',1,'8vzbr1'), ('8vzbr1','venue',1,'6z62bt'), ('j68x6y','upgrade_fixture_company',1,'6z62bt'), ('j68x6y','upgrade_fixture_company',1,'8vzbr1'), ('j68x6y','upgrade_fixture_points_at',1,'fmknnq');
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
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('17t9nq','pairs_with',1,'@#zxttg0','f5'), ('17t9nq','reports_to',1,'zxttg0','f4'), ('17t9nq','role/upgrade-fixture-holder/claimed_at',1,'2026-10-08T14:02:09.720965692Z','f6'), ('17t9nq','role/upgrade-fixture-holder/holder',1,'c2pw','f6'), ('17t9nq','role/upgrade-fixture-recorder/claimed_at',1,'2026-10-08T14:02:02.074362234Z','f1'), ('17t9nq','role/upgrade-fixture-recorder/claimed_at',2,'1970-01-01T00:00:00Z','f1'), ('17t9nq','role/upgrade-fixture-recorder/holder',1,'qkqb','f1'), ('17t9nq','role/upgrade-fixture-recorder/holder',2,NULL,'f1'), ('17t9nq','starred',1,'true','f3'), ('3cfj26','depends_on',1,'tpmbn2','f1'), ('3cfj26','owner',1,'8vzbr1','f1'), ('3cfj26','status',1,'review','f1'), ('3vgp04','columns',1,'someday, next, now, waiting, done, review','f1'), ('3vgp04','status',1,'now','f1'), ('6z62bt','read_from',1,'the recorded place\'s page','f1'), ('6z62bt','read_ref',1,'opening hours','f1'), ('76j2qm','timezone',1,'America/New_York','f1'), ('8vzbr1','since',1,'2026-01-01','f1'), ('8vzbr1','venue',1,'6z62bt','f2'), ('9924zm','status',1,'done','f1'), ('j68x6y','decide_by',1,'2026-12-01','f3'), ('j68x6y','due_on',1,'2026-12-01','f3'), ('j68x6y','merged_from',1,'thing:recorded-twin','f2'), ('j68x6y','upgrade_fixture_company',1,'@#8vzbr1, @#6z62bt','f5'), ('j68x6y','upgrade_fixture_points_at',1,'@#fmknnq','f4'), ('jyhc7b','starred',1,'true','f1'), ('tpmbn2','status',1,'done','f1'), ('y6gddq','status',1,'now','f1');
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
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`) VALUES ('0yqpgy','456b4c',7,'2026-10-08T14:02:08.190936806Z','quarantined messages: 78q97k (1)',NULL,'quarantine',NULL,NULL), ('0yqpgy','5nkctj',10,'2026-10-08T14:02:09.435216558Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL), ('0yqpgy','7keme9',8,'2026-10-08T14:02:08.420655966Z','archived entities: thing:upgrade-fixture-archived (1)',NULL,'archive_entity',NULL,NULL), ('0yqpgy','9ahawj',2,'2026-10-08T14:02:02.631870224Z','captured facts about: person:upgrade-fixture-person, thing:recorded-twin, thing:red-kite, thing:blue-kite, project:upgrade-fixture-project, … (17)','2026-10-08T14:02:09.38083274Z','capture',NULL,NULL), ('0yqpgy','bjy1cy',5,'2026-10-08T14:02:07.204607238Z','retired messages: b4jppx (1)',NULL,'mark_processed',NULL,NULL), ('0yqpgy','e01fwk',1,'2026-10-08T14:02:02.356905291Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing, thing:recorded-twin, thing:red-kite, … (15)','2026-10-08T14:02:08.603240517Z','add_entity',NULL,NULL), ('0yqpgy','j3b4d4',3,'2026-10-08T14:02:03.265435535Z','merged entities: thing:recorded-twin (1)',NULL,'merge_entities',NULL,NULL), ('0yqpgy','sherzz',6,'2026-10-08T14:02:07.544990887Z','renamed entities: thing:upgrade-fixture-successor (1)',NULL,'rename_entity',NULL,NULL), ('0yqpgy','td3272',9,'2026-10-08T14:02:08.93923844Z','wrote charters for: bot:upgrade-fixture-heavy, … (2)','2026-10-08T14:02:09.136237746Z','set_charter',NULL,NULL), ('0yqpgy','z6638g',4,'2026-10-08T14:02:06.902415567Z','posted to mailboxes: assistant, … (4)','2026-10-08T14:02:08.137180866Z','post_message',NULL,NULL), ('hvfw5x','d8dgce',1,'2026-10-08T14:02:06.298233865Z','captured facts about: bot:assistant (1)',NULL,'capture',NULL,NULL);
DROP TABLE IF EXISTS `kind`;
CREATE TABLE `kind` (
  `token` varchar(64) NOT NULL,
  `origin` varchar(16) NOT NULL DEFAULT 'declared',
  PRIMARY KEY (`token`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `kind` (`token`,`origin`) VALUES ('bot','shipped'), ('event','shipped'), ('machine','shipped'), ('org','shipped'), ('person','shipped'), ('pet','shipped'), ('place','shipped'), ('project','shipped'), ('promise','shipped'), ('rhythm','shipped'), ('session','shipped'), ('thing','shipped'), ('thread','shipped'), ('topic','shipped'), ('view','shipped'), ('work','shipped');
DROP TABLE IF EXISTS `mailbox`;
CREATE TABLE `mailbox` (
  `name` varchar(191) NOT NULL,
  `owner` varchar(191) NOT NULL,
  PRIMARY KEY (`name`),
  KEY `by_owner` (`owner`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `mailbox` (`name`,`owner`) VALUES ('assistant','bot:assistant'), ('upgrade-fixture-heavy','bot:upgrade-fixture-heavy'), ('upgrade-fixture-lead','bot:upgrade-fixture-lead');
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
  KEY `by_sender` (`sender`),
  KEY `in_order` (`mailbox`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('78q97k','assistant',4,'set aside as unreadable',NULL,'17t9nq','2026-10-08T14:02:08.093057447Z','quarantined',NULL,NULL,1,'0yqpgy','assistant','2026-10-08T14:02:08.17413455Z','recorded as unreadable'), ('b4jppx','assistant',3,'will be processed',NULL,'17t9nq','2026-10-08T14:02:07.033314521Z','processed',NULL,NULL,2,'0yqpgy',NULL,NULL,NULL), ('fkk4re','assistant',2,'will be read',NULL,'17t9nq','2026-10-08T14:02:06.950570094Z','read',NULL,NULL,1,'0yqpgy',NULL,NULL,NULL), ('stapsd','assistant',1,'left new',NULL,'17t9nq','2026-10-08T14:02:06.855972854Z','new',NULL,NULL,0,'0yqpgy',NULL,NULL,NULL);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('fkk4re','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-08T14:02:00.857851605Z'), ('0002_journal_entry','2026-10-08T14:02:00.868015403Z'), ('0003_minted','2026-10-08T14:02:00.87776878Z'), ('0004_mailbox','2026-10-08T14:02:00.888104506Z'), ('0005_message','2026-10-08T14:02:00.897461656Z'), ('0006_handover','2026-10-08T14:02:00.906367117Z'), ('0007_drop_minted','2026-10-08T14:02:00.914726599Z'), ('0008_entity','2026-10-08T14:02:00.925884443Z'), ('0009_entity_alias','2026-10-08T14:02:00.934942164Z'), ('0010_fact','2026-10-08T14:02:00.9485124Z'), ('0011_fact_event_metadata','2026-10-08T14:02:00.958182158Z'), ('0012_fact_event_ref','2026-10-08T14:02:00.973244658Z'), ('0013_type_field','2026-10-08T14:02:00.982899807Z'), ('0014_message_delivery','2026-10-08T14:02:00.992412016Z'), ('0015_type_field_origin','2026-10-08T14:02:01.00357546Z'), ('0016_field_write','2026-10-08T14:02:01.013513457Z'), ('0017_type_field_folds','2026-10-08T14:02:01.023026757Z'), ('0018_type_field_holds_wider','2026-10-08T14:02:01.033502702Z'), ('0019_entity_source_wider','2026-10-08T14:02:01.043944558Z'), ('0020_entity_crm_wider','2026-10-08T14:02:01.053090668Z'), ('0021_kind','2026-10-08T14:02:01.062571437Z'), ('0022_type_field_owner','2026-10-08T14:02:01.072391155Z'), ('0023_type_field_required','2026-10-08T14:02:01.081440055Z'), ('0024_rhythm_kind_takes_its_name','2026-10-08T14:02:01.089365949Z'), ('0025_type_field_one_of','2026-10-08T14:02:01.100732003Z'), ('0026_session_timezone','2026-10-08T14:02:01.111336418Z'), ('0027_fact_inserted_at','2026-10-08T14:02:01.121442825Z'), ('0028_fact_stale_after','2026-10-08T14:02:01.133033517Z'), ('0029_fact_by_source','2026-10-08T14:02:01.143897861Z'), ('0030_session_stated_day','2026-10-08T14:02:01.154608036Z'), ('0031_journal_entry_day','2026-10-08T14:02:01.164308595Z'), ('0032_entity_badge','2026-10-08T14:02:01.174573591Z'), ('0033_entity_merged_into','2026-10-08T14:02:01.184812869Z'), ('0034_fact_write','2026-10-08T14:02:01.195479042Z'), ('0035_fact_write_backfill','2026-10-08T14:02:01.204323014Z'), ('0036_fact_write_moment','2026-10-08T14:02:01.217018813Z'), ('0037_fact_happened_at','2026-10-08T14:02:01.227360189Z'), ('0038_fact_write_happened_at','2026-10-08T14:02:01.238622483Z'), ('0039_fact_recorded_at','2026-10-08T14:02:01.249752516Z'), ('0040_fact_write_recorded_at','2026-10-08T14:02:01.259898813Z'), ('0041_session_teaching','2026-10-08T14:02:01.27002252Z'), ('0042_entity_former_handle','2026-10-08T14:02:01.280971283Z'), ('0043_message_sender_mail_waiting','2026-10-08T14:02:01.291066931Z'), ('0044_fact_stands_for','2026-10-08T14:02:01.302095015Z'), ('0045_entity_former_handle_key_add','2026-10-08T14:02:01.342029155Z'), ('0045_entity_former_handle_key_drop','2026-10-08T14:02:01.332631155Z'), ('0045_entity_former_handle_ordinal','2026-10-08T14:02:01.32209084Z'), ('0045_entity_former_handle_width','2026-10-08T14:02:01.311872533Z'), ('0046_fact_status_archived','2026-10-08T14:02:01.349948858Z'), ('0047_fact_write_status_archived','2026-10-08T14:02:01.359826726Z'), ('0048_session_served_chars','2026-10-08T14:02:01.371936017Z'), ('0049_entity_archived','2026-10-08T14:02:01.381473886Z'), ('0050_entity_archived_at','2026-10-08T14:02:01.393040258Z'), ('0051_entity_write','2026-10-08T14:02:01.402633457Z'), ('0052_fact_happened_through','2026-10-08T14:02:01.412205325Z'), ('0053_fact_write_happened_through','2026-10-08T14:02:01.421737954Z'), ('0054_session_write','2026-10-08T14:02:01.431429363Z'), ('0055_message_posted_by_session','2026-10-08T14:02:01.441737639Z'), ('0056_session_stated_day','2026-10-08T14:02:01.451736076Z'), ('0057_message_quarantined_by','2026-10-08T14:02:01.461602344Z'), ('0058_message_quarantined_at','2026-10-08T14:02:01.470980394Z'), ('0059_message_quarantine_reason','2026-10-08T14:02:01.481399369Z'), ('0060_journal_entry_closing_focus','2026-10-08T14:02:01.491115378Z'), ('0061_displaced_type_field','2026-10-08T14:02:01.501927302Z'), ('0062_message_by_sender','2026-10-08T14:02:01.51191845Z'), ('0063_field_link','2026-10-08T14:02:01.522314627Z'), ('0064_session_wrap_window','2026-10-08T14:02:01.533634129Z'), ('0065_fact_write_session','2026-10-08T14:02:01.545165061Z');
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
  `wrap_window` varchar(80),
  PRIMARY KEY (`id`),
  KEY `by_bot` (`bot`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`,`wrap_window`) VALUES ('0yqpgy','qkqb','17t9nq','claimed the upgrade-fixture-recorder role','2026-10-08T14:02:02.165712915Z','wrapped',NULL,NULL,34313,NULL,NULL), ('hvfw5x','p60v','zxttg0','working','2026-10-08T14:02:06.272748867Z','active',NULL,NULL,2865,NULL,NULL), ('m8fpdj','c2pw','17t9nq','claimed the upgrade-fixture-holder role','2026-10-08T14:02:09.8446042Z','active',NULL,NULL,0,NULL,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('p60v','claim-subject','2026-10-08T14:02:06.335256458Z'), ('p60v','claims','2026-10-08T14:02:06.331477499Z'), ('qkqb','claim-direction','2026-10-08T14:02:02.667060829Z'), ('qkqb','claim-subject','2026-10-08T14:02:02.662093825Z'), ('qkqb','claims','2026-10-08T14:02:02.656727092Z'), ('qkqb','field-shadows-argument','2026-10-08T14:02:03.580059638Z'), ('qkqb','projects-skill','2026-10-08T14:02:04.085789727Z'), ('qkqb','session-id','2026-10-08T14:02:09.455973602Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('0yqpgy',1), ('0yqpgy',2), ('0yqpgy',3), ('0yqpgy',4), ('0yqpgy',5), ('0yqpgy',6), ('0yqpgy',7), ('0yqpgy',8), ('0yqpgy',9), ('0yqpgy',10), ('0yqpgy',11), ('0yqpgy',12), ('0yqpgy',13), ('0yqpgy',14), ('0yqpgy',15), ('0yqpgy',16), ('0yqpgy',17), ('0yqpgy',18), ('0yqpgy',19), ('0yqpgy',20), ('0yqpgy',21), ('0yqpgy',22), ('0yqpgy',23), ('0yqpgy',24), ('0yqpgy',25), ('0yqpgy',26), ('0yqpgy',27), ('0yqpgy',28), ('0yqpgy',29), ('0yqpgy',30), ('0yqpgy',31), ('0yqpgy',32), ('0yqpgy',33), ('0yqpgy',34), ('0yqpgy',35), ('0yqpgy',36), ('0yqpgy',37), ('0yqpgy',38), ('0yqpgy',39), ('0yqpgy',40), ('0yqpgy',41), ('0yqpgy',42), ('0yqpgy',43), ('0yqpgy',44), ('0yqpgy',45), ('hvfw5x',1), ('hvfw5x',2), ('m8fpdj',1);
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
INSERT INTO `type_field` (`type_name`,`key_name`,`ordinal`,`holds`,`origin`,`folds`,`owner`,`required`,`one_of`) VALUES ('bot','reports_to',1,'reference:bot','shipped','newest','kind',0,NULL), ('decide-by','decide_by',1,'date','shipped','newest','type',0,NULL), ('entitlement','admits',1,'reference','shipped','newest','type',1,NULL), ('entitlement','tier',4,'text','shipped','newest','type',0,NULL), ('entitlement','valid_from',2,'date','shipped','newest','type',0,NULL), ('entitlement','valid_until',3,'date','shipped','newest','type',0,NULL), ('project','columns',2,'list:text','shipped','newest','kind',0,NULL), ('project','status',1,'text','shipped','newest','kind',0,'someday,next,now,waiting,done'), ('promise','ended',3,'text','shipped','newest','kind',0,'delivered,withdrawn,overtaken'), ('promise','promised_by',1,'date','shipped','newest','kind',1,NULL), ('promise','regarding',2,'reference','shipped','newest','kind',0,NULL), ('record-labels','purpose',5,'text','shipped','describes','type',0,NULL), ('record-labels','read_from',1,'text','shipped','describes','type',0,NULL), ('record-labels','read_ref',2,'text','shipped','describes','type',0,NULL), ('record-labels','recorded_by',6,'text','shipped','describes','type',0,NULL), ('record-labels','starred',3,'text','shipped','describes','type',0,NULL), ('record-labels','subject',4,'text','shipped','describes','type',0,NULL), ('rhythm','advances_from',5,'text','shipped','newest','kind',0,'due_date,check_in_date'), ('rhythm','cadence_days',4,'number','shipped','newest','kind',0,NULL), ('rhythm','counts_from',6,'date','shipped','newest','kind',0,NULL), ('rhythm','last_check_in',2,'date','shipped','newest','kind',1,NULL), ('rhythm','name',1,'text','shipped','newest','kind',1,NULL), ('rhythm','note',3,'text','shipped','newest','kind',0,NULL), ('rhythm','outcome',8,'text','shipped','newest','kind',0,'ran,skipped,snoozed'), ('rhythm','snoozed_until',7,'date','shipped','newest','kind',0,NULL), ('runs-out','runs_out',1,'date','shipped','newest','type',0,NULL), ('topic','operator',1,'reference:person','shipped','newest','kind',0,NULL), ('trip','arrives_at',2,'reference','shipped','newest','type',1,NULL), ('trip','departs_from',1,'reference','shipped','newest','type',1,NULL), ('trip','leaves_on',3,'date','shipped','newest','type',1,NULL), ('trip','returns_on',4,'date','shipped','newest','type',1,NULL), ('upgrade-fixture-type','status',1,'text','declared','newest','type',0,'open,closed'), ('upgrade-fixture-visit','venue',1,'reference:place','declared','newest','type',0,NULL), ('view','asks',3,'text','shipped','newest','kind',0,'overdue'), ('view','selects',1,'text','shipped','newest','kind',1,NULL), ('view','shows',2,'text','shipped','newest','kind',0,NULL), ('work','commit',5,'text','shipped','newest','kind',0,NULL), ('work','depends_on',4,'list:reference','shipped','newest','kind',0,NULL), ('work','owner',2,'reference','shipped','newest','kind',0,NULL), ('work','status',1,'text','shipped','newest','kind',0,'someday,next,now,waiting,done'), ('work','verified_by',6,'text','shipped','newest','kind',0,NULL), ('work','waiting_on',3,'reference','shipped','newest','kind',0,NULL);
