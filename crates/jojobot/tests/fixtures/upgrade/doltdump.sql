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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','ffwmx4',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','awnapt',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','kz5c2k',NULL,NULL,NULL), ('project:upgrade-fixture-project','project','A Recorded Project','test',NULL,NULL,'on-demand','','05eeh3',NULL,NULL,NULL), ('thing:recorded-twin','thing','A Spare Copy','test',NULL,NULL,'on-demand','','3q1dyn','thing:upgrade-fixture-thing',NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','vtvb8b',NULL,NULL,NULL), ('work:upgrade-fixture-prior','work','A Recorded Prior Task','test',NULL,'05eeh3','on-demand','','j2m22m',NULL,NULL,NULL), ('work:upgrade-fixture-task','work','A Recorded Task','test',NULL,'05eeh3','on-demand','','t1h8sd',NULL,NULL,NULL);
DROP TABLE IF EXISTS `entity_alias`;
CREATE TABLE `entity_alias` (
  `entity` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `alias` text NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_alias` (`entity`,`ordinal`,`alias`) VALUES ('vtvb8b',1,'A Spare Copy');
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
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-07T12:14:05.040063567Z'), ('person:upgrade-fixture-person',1,'2026-10-07T12:14:05.571081556Z'), ('place:upgrade-fixture-place',1,'2026-10-07T12:14:05.730481966Z'), ('project:upgrade-fixture-project',1,'2026-10-07T12:14:07.531441308Z'), ('thing:recorded-twin',1,'2026-10-07T12:14:06.915011648Z'), ('thing:recorded-twin',2,'2026-10-07T12:14:07.279143791Z'), ('thing:upgrade-fixture-thing',1,'2026-10-07T12:14:05.877208269Z'), ('thing:upgrade-fixture-thing',2,'2026-10-07T12:14:07.273912132Z'), ('work:upgrade-fixture-prior',1,'2026-10-07T12:14:07.67669728Z'), ('work:upgrade-fixture-task',1,'2026-10-07T12:14:07.90854656Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('05eeh3','f1','the recorded project is under way',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T12:14:08.057370056Z',NULL,NULL,NULL), ('awnapt','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-07','location','kz5c2k',NULL,NULL,NULL,'2026-10-07T12:14:06.226203813Z',NULL,NULL,NULL), ('awnapt','f2','visited the recorded place',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,'awnapt','f1','2026-10-07T12:14:06.596607558Z',NULL,NULL,NULL), ('ffwmx4','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,NULL,NULL), ('ffwmx4','f2','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-07','connection','vtvb8b',NULL,NULL,NULL,'2026-10-07T12:14:09.050985858Z',NULL,NULL,NULL), ('ffwmx4','f3','claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T12:14:10.789047285Z',NULL,NULL,NULL), ('j2m22m','f1','the prior task is finished',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T12:14:08.262370505Z',NULL,NULL,NULL), ('t1h8sd','f1','the recorded task is in review',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T12:14:08.454647625Z',NULL,NULL,NULL), ('vtvb8b','f1','the twin carried a note',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T12:14:07.058989764Z',NULL,NULL,NULL), ('vtvb8b','f2','recorded as one thing',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T12:14:07.274851949Z',NULL,NULL,NULL), ('vtvb8b','f3','a day written by hand',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T12:14:08.664716994Z',NULL,NULL,NULL);
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
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`) VALUES ('05eeh3','f1',1,'the recorded project is under way',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:08.057370056Z',NULL,'2026-10-07T12:14:08.061546849Z',NULL,NULL), ('awnapt','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-07','location','kz5c2k',NULL,NULL,'2026-10-07T12:14:06.226203813Z',NULL,'2026-10-07T12:14:06.230999564Z',NULL,NULL), ('awnapt','f2',1,'visited the recorded place',NULL,'inference','open','active','2026-10-07',NULL,NULL,'awnapt','f1','2026-10-07T12:14:06.596607558Z',NULL,'2026-10-07T12:14:06.600882582Z',NULL,NULL), ('ffwmx4','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:05.373005398Z',NULL,NULL), ('ffwmx4','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:05.637533606Z',NULL,NULL), ('ffwmx4','f1',3,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:05.789359488Z',NULL,NULL), ('ffwmx4','f1',4,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:06.033830581Z',NULL,NULL), ('ffwmx4','f1',5,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:06.314351883Z',NULL,NULL), ('ffwmx4','f1',6,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:06.680717207Z',NULL,NULL), ('ffwmx4','f1',7,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:06.97356967Z',NULL,NULL), ('ffwmx4','f1',8,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:07.150031215Z',NULL,NULL), ('ffwmx4','f1',9,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:07.392843571Z',NULL,NULL), ('ffwmx4','f1',10,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:07.585589929Z',NULL,NULL), ('ffwmx4','f1',11,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:07.741978145Z',NULL,NULL), ('ffwmx4','f1',12,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:07.970941176Z',NULL,NULL), ('ffwmx4','f1',13,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:08.14509278Z',NULL,NULL), ('ffwmx4','f1',14,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:08.340769288Z',NULL,NULL), ('ffwmx4','f1',15,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:08.547522989Z',NULL,NULL), ('ffwmx4','f1',16,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:08.75933289Z',NULL,NULL), ('ffwmx4','f1',17,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:09.246345896Z',NULL,NULL), ('ffwmx4','f1',18,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:09.426828864Z',NULL,NULL), ('ffwmx4','f1',19,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:09.685708084Z',NULL,NULL), ('ffwmx4','f1',20,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:09.900734053Z',NULL,NULL), ('ffwmx4','f1',21,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:10.182664049Z',NULL,NULL), ('ffwmx4','f1',22,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:10.316751749Z',NULL,NULL), ('ffwmx4','f1',23,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:05.367253643Z',NULL,'2026-10-07T12:14:10.543459733Z',NULL,NULL), ('ffwmx4','f2',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-07','connection','vtvb8b',NULL,NULL,'2026-10-07T12:14:09.050985858Z',NULL,'2026-10-07T12:14:09.055502029Z',NULL,NULL), ('ffwmx4','f3',1,'claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:10.789047285Z',NULL,'2026-10-07T12:14:10.794595142Z',NULL,NULL), ('j2m22m','f1',1,'the prior task is finished',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:08.262370505Z',NULL,'2026-10-07T12:14:08.266527078Z',NULL,NULL), ('t1h8sd','f1',1,'the recorded task is in review',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:08.454647625Z',NULL,'2026-10-07T12:14:08.458961228Z',NULL,NULL), ('vtvb8b','f1',1,'the twin carried a note',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:07.058989764Z',NULL,'2026-10-07T12:14:07.063311447Z',NULL,NULL), ('vtvb8b','f2',1,'recorded as one thing',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:07.274851949Z',NULL,'2026-10-07T12:14:07.276553842Z',NULL,NULL), ('vtvb8b','f3',1,'a day written by hand',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T12:14:08.664716994Z',NULL,'2026-10-07T12:14:08.669346855Z',NULL,NULL);
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
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('05eeh3','columns',1,'someday, next, now, waiting, done, review','f1'), ('05eeh3','status',1,'now','f1'), ('awnapt','since',1,'2026-01-01','f1'), ('awnapt','venue',1,'kz5c2k','f2'), ('ffwmx4','role/upgrade-fixture-holder/claimed_at',1,'2026-10-07T12:14:10.767890897Z','f3'), ('ffwmx4','role/upgrade-fixture-holder/holder',1,'b88k','f3'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',1,'2026-10-07T12:14:05.358983627Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',2,'2026-10-07T12:14:05.607562032Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',3,'2026-10-07T12:14:05.763962136Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',4,'2026-10-07T12:14:06.013726576Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',5,'2026-10-07T12:14:06.292582031Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',6,'2026-10-07T12:14:06.659834812Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',7,'2026-10-07T12:14:06.95134397Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',8,'2026-10-07T12:14:07.12152941Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',9,'2026-10-07T12:14:07.368854817Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',10,'2026-10-07T12:14:07.563389389Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',11,'2026-10-07T12:14:07.715465052Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',12,'2026-10-07T12:14:07.948772227Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',13,'2026-10-07T12:14:08.120908469Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',14,'2026-10-07T12:14:08.318718357Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',15,'2026-10-07T12:14:08.522882269Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',16,'2026-10-07T12:14:08.73480808Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',17,'2026-10-07T12:14:09.222766322Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',18,'2026-10-07T12:14:09.404298826Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',19,'2026-10-07T12:14:09.661214103Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',20,'2026-10-07T12:14:09.874376289Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',21,'2026-10-07T12:14:10.158797496Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',22,'2026-10-07T12:14:10.28411687Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/claimed_at',23,'1970-01-01T00:00:00Z','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',1,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',2,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',3,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',4,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',5,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',6,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',7,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',8,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',9,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',10,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',11,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',12,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',13,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',14,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',15,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',16,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',17,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',18,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',19,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',20,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',21,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',22,'0ndm','f1'), ('ffwmx4','role/upgrade-fixture-recorder/holder',23,NULL,'f1'), ('j2m22m','status',1,'done','f1'), ('t1h8sd','depends_on',1,'work:upgrade-fixture-prior','f1'), ('t1h8sd','owner',1,'person:upgrade-fixture-person','f1'), ('t1h8sd','status',1,'review','f1'), ('vtvb8b','due_on',1,'2026-12-01','f3'), ('vtvb8b','merged_from',1,'thing:recorded-twin','f2');
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
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`) VALUES ('02tnc7','18ct1m',2,'2026-10-07T12:14:06.479999751Z','captured facts about: person:upgrade-fixture-person, thing:recorded-twin, project:upgrade-fixture-project, work:upgrade-fixture-prior, work:upgrade-fixture-task, … (8)','2026-10-07T12:14:09.323885582Z','capture',NULL,NULL), ('02tnc7','c5x51v',1,'2026-10-07T12:14:05.702259345Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing, thing:recorded-twin, project:upgrade-fixture-project, … (7)','2026-10-07T12:14:08.032079799Z','add_entity',NULL,NULL), ('02tnc7','d14ahh',3,'2026-10-07T12:14:07.459814239Z','merged entities: thing:recorded-twin (1)',NULL,'merge_entities',NULL,NULL), ('02tnc7','d70zv5',6,'2026-10-07T12:14:10.28411687Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL), ('02tnc7','dbh3b5',4,'2026-10-07T12:14:09.502984336Z','posted to mailboxes: assistant, … (3)','2026-10-07T12:14:09.970089691Z','post_message',NULL,NULL), ('02tnc7','vxjwhx',5,'2026-10-07T12:14:10.26574249Z','retired messages: 95z45t (1)',NULL,'mark_processed',NULL,NULL);
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
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('2zd6sn','assistant',1,'left new',NULL,'ffwmx4','2026-10-07T12:14:09.373831799Z','new',NULL,NULL,0,'02tnc7',NULL,NULL,NULL), ('95z45t','assistant',3,'will be processed',NULL,'ffwmx4','2026-10-07T12:14:09.850680506Z','processed',NULL,NULL,2,'02tnc7',NULL,NULL,NULL), ('qpydvh','assistant',2,'will be read',NULL,'ffwmx4','2026-10-07T12:14:09.529379378Z','read',NULL,NULL,1,'02tnc7',NULL,NULL,NULL);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('qpydvh','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-07T12:14:04.052654165Z'), ('0002_journal_entry','2026-10-07T12:14:04.063060892Z'), ('0003_minted','2026-10-07T12:14:04.073657508Z'), ('0004_mailbox','2026-10-07T12:14:04.083590786Z'), ('0005_message','2026-10-07T12:14:04.092790808Z'), ('0006_handover','2026-10-07T12:14:04.10167282Z'), ('0007_drop_minted','2026-10-07T12:14:04.110347253Z'), ('0008_entity','2026-10-07T12:14:04.12069986Z'), ('0009_entity_alias','2026-10-07T12:14:04.128771925Z'), ('0010_fact','2026-10-07T12:14:04.138896734Z'), ('0011_fact_event_metadata','2026-10-07T12:14:04.150177386Z'), ('0012_fact_event_ref','2026-10-07T12:14:04.160110814Z'), ('0013_type_field','2026-10-07T12:14:04.168677089Z'), ('0014_message_delivery','2026-10-07T12:14:04.177311922Z'), ('0015_type_field_origin','2026-10-07T12:14:04.186893012Z'), ('0016_field_write','2026-10-07T12:14:04.196151543Z'), ('0017_type_field_folds','2026-10-07T12:14:04.205814622Z'), ('0018_type_field_holds_wider','2026-10-07T12:14:04.215284393Z'), ('0019_entity_source_wider','2026-10-07T12:14:04.224321984Z'), ('0020_entity_crm_wider','2026-10-07T12:14:04.233413676Z'), ('0021_kind','2026-10-07T12:14:04.242335319Z'), ('0022_type_field_owner','2026-10-07T12:14:04.251270102Z'), ('0023_type_field_required','2026-10-07T12:14:04.259855385Z'), ('0024_rhythm_kind_takes_its_name','2026-10-07T12:14:04.266710026Z'), ('0025_type_field_one_of','2026-10-07T12:14:04.275521479Z'), ('0026_session_timezone','2026-10-07T12:14:04.290715436Z'), ('0027_fact_inserted_at','2026-10-07T12:14:04.300564204Z'), ('0028_fact_stale_after','2026-10-07T12:14:04.310990851Z'), ('0029_fact_by_source','2026-10-07T12:14:04.320887489Z'), ('0030_session_stated_day','2026-10-07T12:14:04.330528289Z'), ('0031_journal_entry_day','2026-10-07T12:14:04.341239963Z'), ('0032_entity_badge','2026-10-07T12:14:04.350319094Z'), ('0033_entity_merged_into','2026-10-07T12:14:04.359757306Z'), ('0034_fact_write','2026-10-07T12:14:04.37057571Z'), ('0035_fact_write_backfill','2026-10-07T12:14:04.378944385Z'), ('0036_fact_write_moment','2026-10-07T12:14:04.389377222Z'), ('0037_fact_happened_at','2026-10-07T12:14:04.399783558Z'), ('0038_fact_write_happened_at','2026-10-07T12:14:04.409857895Z'), ('0039_fact_recorded_at','2026-10-07T12:14:04.421593045Z'), ('0040_fact_write_recorded_at','2026-10-07T12:14:04.430893217Z'), ('0041_session_teaching','2026-10-07T12:14:04.439523261Z'), ('0042_entity_former_handle','2026-10-07T12:14:04.449588898Z'), ('0043_message_sender_mail_waiting','2026-10-07T12:14:04.461818237Z'), ('0044_fact_stands_for','2026-10-07T12:14:04.471985443Z'), ('0045_entity_former_handle_key_add','2026-10-07T12:14:04.512478754Z'), ('0045_entity_former_handle_key_drop','2026-10-07T12:14:04.502422136Z'), ('0045_entity_former_handle_ordinal','2026-10-07T12:14:04.491519812Z'), ('0045_entity_former_handle_width','2026-10-07T12:14:04.481628223Z'), ('0046_fact_status_archived','2026-10-07T12:14:04.521801345Z'), ('0047_fact_write_status_archived','2026-10-07T12:14:04.529454103Z'), ('0048_session_served_chars','2026-10-07T12:14:04.538808313Z'), ('0049_entity_archived','2026-10-07T12:14:04.548368093Z'), ('0050_entity_archived_at','2026-10-07T12:14:04.592207799Z'), ('0051_entity_write','2026-10-07T12:14:04.653438932Z'), ('0052_fact_happened_through','2026-10-07T12:14:04.665452801Z'), ('0053_fact_write_happened_through','2026-10-07T12:14:04.677338861Z'), ('0054_session_write','2026-10-07T12:14:04.688409274Z'), ('0055_message_posted_by_session','2026-10-07T12:14:04.700071045Z'), ('0056_session_stated_day','2026-10-07T12:14:04.711629617Z'), ('0057_message_quarantined_by','2026-10-07T12:14:04.72282598Z'), ('0058_message_quarantined_at','2026-10-07T12:14:04.734495181Z'), ('0059_message_quarantine_reason','2026-10-07T12:14:04.747856575Z'), ('0060_journal_entry_closing_focus','2026-10-07T12:14:04.758928598Z'), ('0061_displaced_type_field','2026-10-07T12:14:04.770562149Z');
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
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`) VALUES ('02tnc7','0ndm','ffwmx4','claimed the upgrade-fixture-recorder role','2026-10-07T12:14:05.535339815Z','wrapped',NULL,NULL,19059,NULL), ('tv8m5z','b88k','ffwmx4','claimed the upgrade-fixture-holder role','2026-10-07T12:14:10.864356371Z','active',NULL,NULL,0,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('0ndm','claim-direction','2026-10-07T12:14:06.511331374Z'), ('0ndm','claim-subject','2026-10-07T12:14:06.505246358Z'), ('0ndm','claims','2026-10-07T12:14:06.500409288Z'), ('0ndm','field-shadows-argument','2026-10-07T12:14:08.238860991Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('02tnc7',1), ('02tnc7',2), ('02tnc7',3), ('02tnc7',4), ('02tnc7',5), ('02tnc7',6), ('02tnc7',7), ('02tnc7',8), ('02tnc7',9), ('02tnc7',10), ('02tnc7',11), ('02tnc7',12), ('02tnc7',13), ('02tnc7',14), ('02tnc7',15), ('02tnc7',16), ('02tnc7',17), ('02tnc7',18), ('02tnc7',19), ('02tnc7',20), ('02tnc7',21), ('02tnc7',22), ('tv8m5z',1);
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
INSERT INTO `type_field` (`type_name`,`key_name`,`ordinal`,`holds`,`origin`,`folds`,`owner`,`required`,`one_of`) VALUES ('decide-by','decide_by',1,'date','shipped','newest','type',0,NULL), ('entitlement','admits',1,'reference','shipped','newest','type',1,NULL), ('entitlement','tier',4,'text','shipped','newest','type',0,NULL), ('entitlement','valid_from',2,'date','shipped','newest','type',0,NULL), ('entitlement','valid_until',3,'date','shipped','newest','type',0,NULL), ('project','budget',1,'text','declared','newest','type',0,NULL), ('promise','ended',3,'text','shipped','newest','kind',0,'delivered,withdrawn,overtaken'), ('promise','promised_by',1,'date','shipped','newest','kind',1,NULL), ('promise','regarding',2,'reference','shipped','newest','kind',0,NULL), ('rhythm','advances_from',5,'text','shipped','newest','kind',0,'due_date,check_in_date'), ('rhythm','cadence_days',4,'number','shipped','newest','kind',0,NULL), ('rhythm','counts_from',6,'date','shipped','newest','kind',0,NULL), ('rhythm','last_check_in',2,'date','shipped','newest','kind',1,NULL), ('rhythm','name',1,'text','shipped','newest','kind',1,NULL), ('rhythm','note',3,'text','shipped','newest','kind',0,NULL), ('rhythm','outcome',8,'text','shipped','newest','kind',0,'ran,skipped,snoozed'), ('rhythm','snoozed_until',7,'date','shipped','newest','kind',0,NULL), ('runs-out','runs_out',1,'date','shipped','newest','type',0,NULL), ('trip','arrives_at',2,'reference','shipped','newest','type',1,NULL), ('trip','departs_from',1,'reference','shipped','newest','type',1,NULL), ('trip','leaves_on',3,'date','shipped','newest','type',1,NULL), ('trip','returns_on',4,'date','shipped','newest','type',1,NULL), ('upgrade-fixture-type','status',1,'text','declared','newest','type',0,'open,closed'), ('upgrade-fixture-visit','venue',1,'reference:place','declared','newest','type',0,NULL), ('view','asks',3,'text','shipped','newest','kind',0,'overdue'), ('view','selects',1,'text','shipped','newest','kind',1,NULL), ('view','shows',2,'text','shipped','newest','kind',0,NULL);
