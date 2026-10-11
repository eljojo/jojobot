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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','xsact7',NULL,NULL,NULL), ('bot:upgrade-fixture-heavy','bot','A Recorded Heavy Bot','test',NULL,NULL,'on-demand','xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx','n4vjj4',NULL,NULL,NULL), ('bot:upgrade-fixture-lead','bot','A Recorded Lead','test',NULL,NULL,'on-demand','','szscm8',NULL,NULL,NULL), ('bot:upgrade-fixture-wrapper','bot','A Recorded Wrapper','test',NULL,NULL,'on-demand','','qs3hda',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','btyjfq',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','47357w',NULL,NULL,NULL), ('project:upgrade-fixture-project','project','A Recorded Project','test',NULL,NULL,'on-demand','','9fanny',NULL,NULL,NULL), ('role:upgrade-fixture-holder','role','upgrade-fixture-holder','jojobot',NULL,'xsact7','on-demand','','28xgks',NULL,NULL,NULL), ('role:upgrade-fixture-recorder','role','upgrade-fixture-recorder','jojobot',NULL,'xsact7','on-demand','','sw1xgs',NULL,NULL,NULL), ('thing:blue-kite','thing','A Recorded Spare Kite','test',NULL,NULL,'on-demand','','0d9cxk',NULL,NULL,NULL), ('thing:recorded-twin','thing','A Spare Copy','test',NULL,NULL,'on-demand','','ym01pm','thing:upgrade-fixture-thing',NULL,NULL), ('thing:red-kite','thing','A Recorded Kite','test',NULL,NULL,'on-demand','','wwdr5h',NULL,NULL,NULL), ('thing:upgrade-fixture-archived','thing','A Recorded Archive','test',NULL,NULL,'on-demand','','hh0848',NULL,'recorded as no longer wanted','2026-10-11T18:45:20.490783719Z'), ('thing:upgrade-fixture-namesake','thing','A Later Heir','test',NULL,NULL,'on-demand','','2hv2k1',NULL,NULL,NULL), ('thing:upgrade-fixture-successor','thing','A Recorded Namesake','test',NULL,NULL,'on-demand','','crfz65',NULL,NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','em9qzt',NULL,NULL,NULL), ('topic:instance','topic','This instance','test',NULL,NULL,'on-demand','','wgf9dh',NULL,NULL,NULL), ('work:upgrade-fixture-prior','work','A Recorded Prior Task','test',NULL,'9fanny','on-demand','','syysk2',NULL,NULL,NULL), ('work:upgrade-fixture-task','work','A Recorded Task','test',NULL,'9fanny','on-demand','','y9x1k8',NULL,NULL,NULL);
DROP TABLE IF EXISTS `entity_alias`;
CREATE TABLE `entity_alias` (
  `entity` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `alias` text NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_alias` (`entity`,`ordinal`,`alias`) VALUES ('em9qzt',1,'A Spare Copy');
DROP TABLE IF EXISTS `entity_former_handle`;
CREATE TABLE `entity_former_handle` (
  `former_handle` varchar(191) NOT NULL,
  `badge` varchar(16) NOT NULL,
  `changed_at` varchar(16) NOT NULL,
  `ordinal` int NOT NULL DEFAULT '1',
  PRIMARY KEY (`former_handle`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_former_handle` (`former_handle`,`badge`,`changed_at`,`ordinal`) VALUES ('thing:upgrade-fixture-namesake','crfz65','2026-10-11',1);
DROP TABLE IF EXISTS `entity_write`;
CREATE TABLE `entity_write` (
  `entity` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `written_at` varchar(40) NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-11T18:45:10.868482874Z'), ('bot:upgrade-fixture-heavy',1,'2026-10-11T18:45:20.947730814Z'), ('bot:upgrade-fixture-heavy',2,'2026-10-11T18:45:21.601147784Z'), ('bot:upgrade-fixture-heavy',3,'2026-10-11T18:45:22.074655615Z'), ('bot:upgrade-fixture-lead',1,'2026-10-11T18:45:16.23771035Z'), ('bot:upgrade-fixture-wrapper',1,'2026-10-11T18:45:24.077342022Z'), ('person:upgrade-fixture-person',1,'2026-10-11T18:45:11.777055372Z'), ('place:upgrade-fixture-place',1,'2026-10-11T18:45:11.974341216Z'), ('project:upgrade-fixture-project',1,'2026-10-11T18:45:14.033159784Z'), ('role:upgrade-fixture-holder',1,'2026-10-11T18:45:23.420084562Z'), ('role:upgrade-fixture-recorder',1,'2026-10-11T18:45:11.368907298Z'), ('thing:blue-kite',1,'2026-10-11T18:45:13.559183247Z'), ('thing:recorded-twin',1,'2026-10-11T18:45:12.804999896Z'), ('thing:recorded-twin',2,'2026-10-11T18:45:13.159907824Z'), ('thing:red-kite',1,'2026-10-11T18:45:13.299254488Z'), ('thing:upgrade-fixture-archived',1,'2026-10-11T18:45:20.373936628Z'), ('thing:upgrade-fixture-archived',2,'2026-10-11T18:45:20.496549379Z'), ('thing:upgrade-fixture-namesake',1,'2026-10-11T18:45:18.599515631Z'), ('thing:upgrade-fixture-namesake',2,'2026-10-11T18:45:18.986179608Z'), ('thing:upgrade-fixture-successor',1,'2026-10-11T18:45:18.756037183Z'), ('thing:upgrade-fixture-thing',1,'2026-10-11T18:45:12.065518717Z'), ('thing:upgrade-fixture-thing',2,'2026-10-11T18:45:13.152539579Z'), ('topic:instance',1,'2026-10-11T18:45:17.338613359Z'), ('work:upgrade-fixture-prior',1,'2026-10-11T18:45:14.240250867Z'), ('work:upgrade-fixture-task',1,'2026-10-11T18:45:14.439567474Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('0d9cxk','f1','where it stands',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:13.694468044Z',NULL,NULL,NULL), ('28xgks','f1','claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:23.487933187Z',NULL,NULL,NULL), ('47357w','f1','the recorded place opens at nine',NULL,'observation','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:15.911702524Z',NULL,NULL,NULL), ('9fanny','f1','the recorded project is under way',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:14.541874549Z',NULL,NULL,NULL), ('btyjfq','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-11','location','47357w',NULL,NULL,NULL,'2026-10-11T18:45:12.261796915Z',NULL,NULL,NULL), ('btyjfq','f2','visited the recorded place',NULL,'inference','open','active','2026-10-11',NULL,NULL,NULL,'btyjfq','f1','2026-10-11T18:45:12.615640966Z',NULL,NULL,NULL), ('btyjfq','f3','a property of the thing, set by this claim',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:22.573879838Z',NULL,NULL,NULL), ('em9qzt','f1','the twin carried a note',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:12.894437443Z',NULL,NULL,NULL), ('em9qzt','f2','recorded as one thing',NULL,'inference','open','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:13.153631815Z',NULL,NULL,NULL), ('em9qzt','f3','a day kept under decide_by',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:15.136803222Z',NULL,NULL,NULL), ('em9qzt','f4','points at a handle two things have worn',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:19.20692907Z',NULL,NULL,NULL), ('em9qzt','f5','keeps company with a person and a place',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:19.603419889Z',NULL,NULL,NULL), ('n4vjj4','f1','the heavy bot keeps its one rule',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:22.356592474Z',NULL,NULL,NULL), ('sw1xgs','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:11.534853924Z',NULL,NULL,NULL), ('syysk2','f1','the prior task is finished',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:14.809915532Z',NULL,NULL,NULL), ('wgf9dh','f1','the instance works in one zone',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:17.482322068Z',NULL,NULL,NULL), ('wwdr5h','f1','where it stands',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:13.400972055Z',NULL,NULL,NULL), ('xsact7','f1','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-11','connection','em9qzt',NULL,NULL,NULL,'2026-10-11T18:45:15.420366423Z',NULL,NULL,NULL), ('xsact7','f2','the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:15.638241843Z',NULL,NULL,NULL), ('xsact7','f3','the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:16.691137864Z',NULL,NULL,NULL), ('xsact7','f4','the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:17.023016692Z',NULL,NULL,NULL), ('y9x1k8','f1','the recorded task is in review',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,NULL,'2026-10-11T18:45:14.968213866Z',NULL,NULL,NULL);
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
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`,`session`) VALUES ('0d9cxk','f1',1,'where it stands',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:13.694468044Z',NULL,'2026-10-11T18:45:13.700679313Z',NULL,NULL,'533q'), ('28xgks','f1',1,'claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:23.487933187Z',NULL,'2026-10-11T18:45:23.495150604Z',NULL,NULL,NULL), ('47357w','f1',1,'the recorded place opens at nine',NULL,'observation','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:15.911702524Z',NULL,'2026-10-11T18:45:15.917603147Z',NULL,NULL,'533q'), ('9fanny','f1',1,'the recorded project is under way',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:14.541874549Z',NULL,'2026-10-11T18:45:14.548086098Z',NULL,NULL,'533q'), ('btyjfq','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-11','location','47357w',NULL,NULL,'2026-10-11T18:45:12.261796915Z',NULL,'2026-10-11T18:45:12.270062207Z',NULL,NULL,'533q'), ('btyjfq','f2',1,'visited the recorded place',NULL,'inference','open','active','2026-10-11',NULL,NULL,'btyjfq','f1','2026-10-11T18:45:12.615640966Z',NULL,'2026-10-11T18:45:12.622308564Z',NULL,NULL,'533q'), ('btyjfq','f3',1,'a property of the thing, set by this claim',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:22.573879838Z',NULL,'2026-10-11T18:45:22.580730186Z',NULL,NULL,'533q'), ('em9qzt','f1',1,'the twin carried a note',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:12.894437443Z',NULL,'2026-10-11T18:45:12.899705586Z',NULL,NULL,'533q'), ('em9qzt','f2',1,'recorded as one thing',NULL,'inference','open','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:13.153631815Z',NULL,'2026-10-11T18:45:13.155695008Z',NULL,NULL,NULL), ('em9qzt','f3',1,'a day kept under decide_by',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:15.136803222Z',NULL,'2026-10-11T18:45:15.142452424Z',NULL,NULL,'533q'), ('em9qzt','f4',1,'points at a handle two things have worn',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:19.20692907Z',NULL,'2026-10-11T18:45:19.213739539Z',NULL,NULL,'533q'), ('em9qzt','f5',1,'keeps company with a person and a place',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:19.603419889Z',NULL,'2026-10-11T18:45:19.611444901Z',NULL,NULL,'533q'), ('n4vjj4','f1',1,'the heavy bot keeps its one rule',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:22.356592474Z',NULL,'2026-10-11T18:45:22.366624921Z',NULL,NULL,'533q'), ('sw1xgs','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:11.534853924Z',NULL,'2026-10-11T18:45:11.543949505Z',NULL,NULL,NULL), ('sw1xgs','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:11.534853924Z',NULL,'2026-10-11T18:45:23.05586833Z',NULL,NULL,NULL), ('syysk2','f1',1,'the prior task is finished',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:14.809915532Z',NULL,'2026-10-11T18:45:14.819165591Z',NULL,NULL,'533q'), ('wgf9dh','f1',1,'the instance works in one zone',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:17.482322068Z',NULL,'2026-10-11T18:45:17.48839915Z',NULL,NULL,'533q'), ('wwdr5h','f1',1,'where it stands',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:13.400972055Z',NULL,'2026-10-11T18:45:13.406743506Z',NULL,NULL,'533q'), ('xsact7','f1',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-11','connection','em9qzt',NULL,NULL,'2026-10-11T18:45:15.420366423Z',NULL,'2026-10-11T18:45:15.425288259Z',NULL,NULL,'533q'), ('xsact7','f2',1,'the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:15.638241843Z',NULL,'2026-10-11T18:45:15.643543446Z',NULL,NULL,'533q'), ('xsact7','f3',1,'the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:16.691137864Z',NULL,'2026-10-11T18:45:16.697096496Z',NULL,NULL,'xjj8'), ('xsact7','f4',1,'the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:17.023016692Z',NULL,'2026-10-11T18:45:17.029352393Z',NULL,NULL,'533q'), ('y9x1k8','f1',1,'the recorded task is in review',NULL,'testimony','settled','active','2026-10-11',NULL,NULL,NULL,NULL,'2026-10-11T18:45:14.968213866Z',NULL,'2026-10-11T18:45:14.978661174Z',NULL,NULL,'533q');
DROP TABLE IF EXISTS `field_link`;
CREATE TABLE `field_link` (
  `entity` varchar(191) NOT NULL,
  `key` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `target` varchar(191) NOT NULL,
  PRIMARY KEY (`entity`,`key`,`ordinal`,`target`),
  KEY `by_target` (`target`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `field_link` (`entity`,`key`,`ordinal`,`target`) VALUES ('btyjfq','venue',1,'47357w'), ('em9qzt','upgrade_fixture_company',1,'47357w'), ('em9qzt','upgrade_fixture_company',1,'btyjfq'), ('em9qzt','upgrade_fixture_points_at',1,'2hv2k1'), ('xsact7','pairs_with',1,'szscm8'), ('xsact7','reports_to',1,'szscm8'), ('y9x1k8','depends_on',1,'syysk2'), ('y9x1k8','owner',1,'btyjfq');
DROP TABLE IF EXISTS `field_write`;
CREATE TABLE `field_write` (
  `entity` varchar(191) NOT NULL,
  `key` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `value` longtext,
  `fact_id` varchar(64) NOT NULL,
  `write_class` varchar(8) NOT NULL DEFAULT 'legacy',
  `written_at` varchar(40),
  PRIMARY KEY (`entity`,`key`,`ordinal`),
  KEY `by_record` (`entity`,`fact_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`,`write_class`,`written_at`) VALUES ('0d9cxk','status',1,'done','f1','own','2026-10-11T18:45:13.705208899Z'), ('28xgks','claimed_at',1,'2026-10-11T18:45:23.390538306Z','f1','own','2026-10-11T18:45:23.501929662Z'), ('28xgks','holder',1,'sg1j','f1','own','2026-10-11T18:45:23.503669636Z'), ('47357w','read_from',1,'the recorded place\'s page','f1','own','2026-10-11T18:45:15.923067991Z'), ('47357w','read_ref',1,'opening hours','f1','own','2026-10-11T18:45:15.924348066Z'), ('9fanny','columns',1,'someday, next, now, waiting, done, review','f1','own','2026-10-11T18:45:14.553329992Z'), ('9fanny','status',1,'now','f1','own','2026-10-11T18:45:14.554502067Z'), ('btyjfq','since',1,'2026-01-01','f1','own','2026-10-11T18:45:12.275296921Z'), ('btyjfq','upgrade_fixture_setting',1,'kept-under-sets','f3','sets','2026-10-11T18:45:22.58555219Z'), ('btyjfq','venue',1,'47357w','f2','own','2026-10-11T18:45:12.627162228Z'), ('em9qzt','decide_by',1,'2026-12-01','f3','own','2026-10-11T18:45:15.14733539Z'), ('em9qzt','due_on',1,'2026-12-01','f3','own','2026-10-11T18:45:15.148376257Z'), ('em9qzt','merged_from',1,'thing:recorded-twin','f2','own','2026-10-11T18:45:13.157841071Z'), ('em9qzt','upgrade_fixture_company',1,'@#btyjfq, @#47357w','f5','own','2026-10-11T18:45:19.621353228Z'), ('em9qzt','upgrade_fixture_points_at',1,'@#2hv2k1','f4','own','2026-10-11T18:45:19.218942281Z'), ('n4vjj4','starred',1,'true','f1','own','2026-10-11T18:45:22.373558637Z'), ('sw1xgs','claimed_at',1,'2026-10-11T18:45:11.353278049Z','f1','own','2026-10-11T18:45:11.550061945Z'), ('sw1xgs','claimed_at',2,'1970-01-01T00:00:00Z','f1','own','2026-10-11T18:45:23.061445292Z'), ('sw1xgs','holder',1,'533q','f1','own','2026-10-11T18:45:11.5517004Z'), ('sw1xgs','holder',2,NULL,'f1','own','2026-10-11T18:45:23.060352966Z'), ('syysk2','status',1,'done','f1','own','2026-10-11T18:45:14.822926549Z'), ('wgf9dh','timezone',1,'America/New_York','f1','own','2026-10-11T18:45:17.492851987Z'), ('wwdr5h','status',1,'now','f1','own','2026-10-11T18:45:13.410896832Z'), ('xsact7','pairs_with',1,'@#szscm8','f4','own','2026-10-11T18:45:17.033882359Z'), ('xsact7','reports_to',1,'szscm8','f3','own','2026-10-11T18:45:16.700693925Z'), ('xsact7','starred',1,'true','f2','own','2026-10-11T18:45:15.647848743Z'), ('y9x1k8','depends_on',1,'syysk2','f1','own','2026-10-11T18:45:14.987876067Z'), ('y9x1k8','owner',1,'btyjfq','f1','own','2026-10-11T18:45:14.989706481Z'), ('y9x1k8','status',1,'review','f1','own','2026-10-11T18:45:14.992019034Z');
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
  `closing` tinyint(1) NOT NULL DEFAULT '0',
  PRIMARY KEY (`session`,`id`),
  KEY `in_order` (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`,`closing`) VALUES ('8p78ad','10x9xk',5,'2026-10-11T18:45:18.533733394Z','retired messages: e5sr2a (1)',NULL,'mark_processed',NULL,NULL,0), ('8p78ad','50acxx',6,'2026-10-11T18:45:18.947484807Z','renamed entities: thing:upgrade-fixture-successor (1)',NULL,'rename_entity',NULL,NULL,0), ('8p78ad','8ymdd7',2,'2026-10-11T18:45:12.45537204Z','captured facts about: person:upgrade-fixture-person, thing:recorded-twin, thing:red-kite, thing:blue-kite, project:upgrade-fixture-project, … (18)','2026-10-11T18:45:22.800979061Z','capture',NULL,NULL,0), ('8p78ad','9wh1ya',8,'2026-10-11T18:45:20.663702551Z','archived entities: thing:upgrade-fixture-archived (1)',NULL,'archive_entity',NULL,NULL,0), ('8p78ad','ayrexr',10,'2026-10-11T18:45:22.871534666Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL,0), ('8p78ad','erghzs',4,'2026-10-11T18:45:17.711503203Z','posted to mailboxes: assistant, … (4)','2026-10-11T18:45:20.210306114Z','post_message',NULL,NULL,0), ('8p78ad','nh2fwk',7,'2026-10-11T18:45:20.329201658Z','quarantined messages: y9pb1m (1)',NULL,'quarantine',NULL,NULL,0), ('8p78ad','nrf4zr',11,'2026-10-11T18:45:22.938435724Z','closed the recording run after it seeded the store',NULL,NULL,NULL,'claimed the upgrade-fixture-recorder role',1), ('8p78ad','nyxxjj',3,'2026-10-11T18:45:13.26187896Z','merged entities: thing:recorded-twin (1)',NULL,'merge_entities',NULL,NULL,0), ('8p78ad','tcvtw3',9,'2026-10-11T18:45:21.792303906Z','wrote charters for: bot:upgrade-fixture-heavy, … (2)','2026-10-11T18:45:22.254897743Z','set_charter',NULL,NULL,0), ('8p78ad','zx1ded',1,'2026-10-11T18:45:11.939937458Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing, thing:recorded-twin, thing:red-kite, … (15)','2026-10-11T18:45:21.118022615Z','add_entity',NULL,NULL,0), ('9ha2j2','89gq9v',2,'2026-10-11T18:45:24.797677041Z','wrapped the only run this bot had',NULL,NULL,NULL,'did one thing before wrapping',1), ('9ha2j2','hjjcja',1,'2026-10-11T18:45:24.735506788Z','did one thing before wrapping',NULL,NULL,NULL,NULL,0), ('9kzpha','thmgjt',1,'2026-10-11T18:45:16.941774985Z','captured facts about: bot:assistant (1)',NULL,'capture',NULL,NULL,0), ('qyv3vq','q49wmt',1,'2026-10-11T18:45:24.209031113Z','brought entities into being: bot:upgrade-fixture-wrapper (1)',NULL,'add_entity',NULL,NULL,0);
DROP TABLE IF EXISTS `kind`;
CREATE TABLE `kind` (
  `token` varchar(64) NOT NULL,
  `origin` varchar(16) NOT NULL DEFAULT 'declared',
  PRIMARY KEY (`token`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `kind` (`token`,`origin`) VALUES ('bot','shipped'), ('event','shipped'), ('machine','shipped'), ('org','shipped'), ('person','shipped'), ('pet','shipped'), ('place','shipped'), ('project','shipped'), ('promise','shipped'), ('rhythm','shipped'), ('role','shipped'), ('session','shipped'), ('thing','shipped'), ('thread','shipped'), ('topic','shipped'), ('view','shipped'), ('work','shipped');
DROP TABLE IF EXISTS `mailbox`;
CREATE TABLE `mailbox` (
  `name` varchar(191) NOT NULL,
  `owner` varchar(191) NOT NULL,
  PRIMARY KEY (`name`),
  KEY `by_owner` (`owner`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `mailbox` (`name`,`owner`) VALUES ('assistant','bot:assistant'), ('upgrade-fixture-heavy','bot:upgrade-fixture-heavy'), ('upgrade-fixture-lead','bot:upgrade-fixture-lead'), ('upgrade-fixture-wrapper','bot:upgrade-fixture-wrapper');
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
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('5qpza2','assistant',1,'left new',NULL,'xsact7','2026-10-11T18:45:17.654529863Z','new',NULL,NULL,0,'8p78ad',NULL,NULL,NULL), ('b2hyjb','assistant',2,'will be read',NULL,'xsact7','2026-10-11T18:45:17.761785952Z','read',NULL,NULL,1,'8p78ad',NULL,NULL,NULL), ('e5sr2a','assistant',3,'will be processed',NULL,'xsact7','2026-10-11T18:45:17.979108658Z','processed',NULL,NULL,2,'8p78ad',NULL,NULL,NULL), ('y9pb1m','assistant',4,'set aside as unreadable',NULL,'xsact7','2026-10-11T18:45:20.026653157Z','quarantined',NULL,NULL,1,'8p78ad','assistant','2026-10-11T18:45:20.286015711Z','recorded as unreadable');
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('b2hyjb','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-11T18:45:09.464968463Z'), ('0002_journal_entry','2026-10-11T18:45:09.475648697Z'), ('0003_minted','2026-10-11T18:45:09.48918561Z'), ('0004_mailbox','2026-10-11T18:45:09.502537535Z'), ('0005_message','2026-10-11T18:45:09.513075588Z'), ('0006_handover','2026-10-11T18:45:09.523447243Z'), ('0007_drop_minted','2026-10-11T18:45:09.533910797Z'), ('0008_entity','2026-10-11T18:45:09.549575753Z'), ('0009_entity_alias','2026-10-11T18:45:09.560632575Z'), ('0010_fact','2026-10-11T18:45:09.57371254Z'), ('0011_fact_event_metadata','2026-10-11T18:45:09.585205661Z'), ('0012_fact_event_ref','2026-10-11T18:45:09.599027274Z'), ('0013_type_field','2026-10-11T18:45:09.609683177Z'), ('0014_message_delivery','2026-10-11T18:45:09.619635933Z'), ('0015_type_field_origin','2026-10-11T18:45:09.647179238Z'), ('0016_field_write','2026-10-11T18:45:09.68479782Z'), ('0017_type_field_folds','2026-10-11T18:45:09.699980648Z'), ('0018_type_field_holds_wider','2026-10-11T18:45:09.711365479Z'), ('0019_entity_source_wider','2026-10-11T18:45:09.726209858Z'), ('0020_entity_crm_wider','2026-10-11T18:45:09.742901961Z'), ('0021_kind','2026-10-11T18:45:09.802332638Z'), ('0022_type_field_owner','2026-10-11T18:45:09.815172103Z'), ('0023_type_field_required','2026-10-11T18:45:09.829633704Z'), ('0024_rhythm_kind_takes_its_name','2026-10-11T18:45:09.83946338Z'), ('0025_type_field_one_of','2026-10-11T18:45:09.850620432Z'), ('0026_session_timezone','2026-10-11T18:45:09.863053789Z'), ('0027_fact_inserted_at','2026-10-11T18:45:09.878543765Z'), ('0028_fact_stale_after','2026-10-11T18:45:09.891909631Z'), ('0029_fact_by_source','2026-10-11T18:45:09.908029135Z'), ('0030_session_stated_day','2026-10-11T18:45:09.921931738Z'), ('0031_journal_entry_day','2026-10-11T18:45:09.936245968Z'), ('0032_entity_badge','2026-10-11T18:45:09.948204858Z'), ('0033_entity_merged_into','2026-10-11T18:45:09.962773467Z'), ('0034_fact_write','2026-10-11T18:45:09.97647147Z'), ('0035_fact_write_backfill','2026-10-11T18:45:09.986879485Z'), ('0036_fact_write_moment','2026-10-11T18:45:09.999556051Z'), ('0037_fact_happened_at','2026-10-11T18:45:10.013880202Z'), ('0038_fact_write_happened_at','2026-10-11T18:45:10.029531988Z'), ('0039_fact_recorded_at','2026-10-11T18:45:10.044069399Z'), ('0040_fact_write_recorded_at','2026-10-11T18:45:10.059755696Z'), ('0041_session_teaching','2026-10-11T18:45:10.072717691Z'), ('0042_entity_former_handle','2026-10-11T18:45:10.085938535Z'), ('0043_message_sender_mail_waiting','2026-10-11T18:45:10.098733951Z'), ('0044_fact_stands_for','2026-10-11T18:45:10.112525955Z'), ('0045_entity_former_handle_key_add','2026-10-11T18:45:10.167183377Z'), ('0045_entity_former_handle_key_drop','2026-10-11T18:45:10.152252399Z'), ('0045_entity_former_handle_ordinal','2026-10-11T18:45:10.138211896Z'), ('0045_entity_former_handle_width','2026-10-11T18:45:10.124118625Z'), ('0046_fact_status_archived','2026-10-11T18:45:10.179558355Z'), ('0047_fact_write_status_archived','2026-10-11T18:45:10.190528007Z'), ('0048_session_served_chars','2026-10-11T18:45:10.203640852Z'), ('0049_entity_archived','2026-10-11T18:45:10.217547504Z'), ('0050_entity_archived_at','2026-10-11T18:45:10.235414424Z'), ('0051_entity_write','2026-10-11T18:45:10.249800874Z'), ('0052_fact_happened_through','2026-10-11T18:45:10.264963781Z'), ('0053_fact_write_happened_through','2026-10-11T18:45:10.283377059Z'), ('0054_session_write','2026-10-11T18:45:10.29500232Z'), ('0055_message_posted_by_session','2026-10-11T18:45:10.307642855Z'), ('0056_session_stated_day','2026-10-11T18:45:10.322314126Z'), ('0057_message_quarantined_by','2026-10-11T18:45:10.334776493Z'), ('0058_message_quarantined_at','2026-10-11T18:45:10.353687009Z'), ('0059_message_quarantine_reason','2026-10-11T18:45:10.36781729Z'), ('0060_journal_entry_closing_focus','2026-10-11T18:45:10.387703052Z'), ('0061_displaced_type_field','2026-10-11T18:45:10.404560484Z'), ('0062_message_by_sender','2026-10-11T18:45:10.429367478Z'), ('0063_field_link','2026-10-11T18:45:10.444315918Z'), ('0064_session_wrap_window','2026-10-11T18:45:10.457019734Z'), ('0065_fact_write_session','2026-10-11T18:45:10.476977046Z'), ('0067_field_write_class','2026-10-11T18:45:10.492700321Z'), ('0068_field_write_written_at','2026-10-11T18:45:10.51071505Z'), ('0069_journal_entry_closing','2026-10-11T18:45:10.522895509Z');
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
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`,`wrap_window`) VALUES ('8p78ad','533q','xsact7','claimed the upgrade-fixture-recorder role','2026-10-11T18:45:11.720477556Z','wrapped',NULL,NULL,34514,NULL,NULL), ('9ha2j2','efjk','qs3hda','did one thing before wrapping','2026-10-11T18:45:24.701847096Z','wrapped',NULL,NULL,2034,NULL,'offered wc-3n3mcjw720kr'), ('9kzpha','xjj8','szscm8','working','2026-10-11T18:45:16.906263151Z','active',NULL,NULL,3026,NULL,NULL), ('qyv3vq','sg1j','xsact7','claimed the upgrade-fixture-holder role','2026-10-11T18:45:23.581742908Z','active',NULL,NULL,517,NULL,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('533q','claim-direction','2026-10-11T18:45:12.49552009Z'), ('533q','claim-subject','2026-10-11T18:45:12.490370946Z'), ('533q','claims','2026-10-11T18:45:12.483753938Z'), ('533q','field-shadows-argument','2026-10-11T18:45:13.528393807Z'), ('533q','projects-skill','2026-10-11T18:45:14.209492158Z'), ('533q','session-id','2026-10-11T18:45:22.906416869Z'), ('efjk','session-id','2026-10-11T18:45:24.764368387Z'), ('xjj8','claim-subject','2026-10-11T18:45:16.98691689Z'), ('xjj8','claims','2026-10-11T18:45:16.98020741Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('8p78ad',1), ('8p78ad',2), ('8p78ad',3), ('8p78ad',4), ('8p78ad',5), ('8p78ad',6), ('8p78ad',7), ('8p78ad',8), ('8p78ad',9), ('8p78ad',10), ('8p78ad',11), ('8p78ad',12), ('8p78ad',13), ('8p78ad',14), ('8p78ad',15), ('8p78ad',16), ('8p78ad',17), ('8p78ad',18), ('8p78ad',19), ('8p78ad',20), ('8p78ad',21), ('8p78ad',22), ('8p78ad',23), ('8p78ad',24), ('8p78ad',25), ('8p78ad',26), ('8p78ad',27), ('8p78ad',28), ('8p78ad',29), ('8p78ad',30), ('8p78ad',31), ('8p78ad',32), ('8p78ad',33), ('8p78ad',34), ('8p78ad',35), ('8p78ad',36), ('8p78ad',37), ('8p78ad',38), ('8p78ad',39), ('8p78ad',40), ('8p78ad',41), ('8p78ad',42), ('8p78ad',43), ('8p78ad',44), ('8p78ad',45), ('8p78ad',46), ('8p78ad',47), ('9ha2j2',1), ('9ha2j2',2), ('9ha2j2',3), ('9kzpha',1), ('9kzpha',2), ('qyv3vq',1), ('qyv3vq',2);
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
INSERT INTO `type_field` (`type_name`,`key_name`,`ordinal`,`holds`,`origin`,`folds`,`owner`,`required`,`one_of`) VALUES ('bot','reports_to',1,'reference:bot','shipped','newest','kind',0,NULL), ('decide-by','decide_by',1,'date','shipped','newest','type',0,NULL), ('entitlement','admits',1,'reference','shipped','newest','type',1,NULL), ('entitlement','tier',4,'text','shipped','newest','type',0,NULL), ('entitlement','valid_from',2,'date','shipped','newest','type',0,NULL), ('entitlement','valid_until',3,'date','shipped','newest','type',0,NULL), ('project','columns',2,'list:text','shipped','newest','kind',0,NULL), ('project','status',1,'text','shipped','newest','kind',0,'someday,next,now,waiting,done'), ('promise','ended',3,'text','shipped','newest','kind',0,'delivered,withdrawn,overtaken'), ('promise','promised_by',1,'date','shipped','newest','kind',1,NULL), ('promise','regarding',2,'reference','shipped','newest','kind',0,NULL), ('record-labels','purpose',5,'text','shipped','describes','type',0,NULL), ('record-labels','read_from',1,'text','shipped','describes','type',0,NULL), ('record-labels','read_ref',2,'text','shipped','describes','type',0,NULL), ('record-labels','recorded_by',6,'text','shipped','describes','type',0,NULL), ('record-labels','starred',3,'text','shipped','describes','type',0,NULL), ('record-labels','subject',4,'text','shipped','describes','type',0,NULL), ('rhythm','advances_from',5,'text','shipped','newest','kind',0,'due_date,check_in_date'), ('rhythm','cadence_days',4,'number','shipped','newest','kind',0,NULL), ('rhythm','counts_from',6,'date','shipped','newest','kind',0,NULL), ('rhythm','last_check_in',2,'date','shipped','newest','kind',1,NULL), ('rhythm','name',1,'text','shipped','newest','kind',1,NULL), ('rhythm','note',3,'text','shipped','newest','kind',0,NULL), ('rhythm','outcome',8,'text','shipped','newest','kind',0,'ran,skipped,snoozed'), ('rhythm','snoozed_until',7,'date','shipped','newest','kind',0,NULL), ('role','agent',3,'text','shipped','newest','kind',0,NULL), ('role','claimed_at',2,'text','shipped','newest','kind',0,NULL), ('role','holder',1,'text','shipped','newest','kind',0,NULL), ('runs-out','runs_out',1,'date','shipped','newest','type',0,NULL), ('topic','operator',1,'reference:person','shipped','newest','kind',0,NULL), ('trip','arrives_at',2,'reference','shipped','newest','type',1,NULL), ('trip','departs_from',1,'reference','shipped','newest','type',1,NULL), ('trip','leaves_on',3,'date','shipped','newest','type',1,NULL), ('trip','returns_on',4,'date','shipped','newest','type',1,NULL), ('upgrade-fixture-type','status',1,'text','declared','newest','type',0,'open,closed'), ('upgrade-fixture-visit','venue',1,'reference:place','declared','newest','type',0,NULL), ('view','asks',3,'text','shipped','newest','kind',0,'overdue'), ('view','selects',1,'text','shipped','newest','kind',1,NULL), ('view','shows',2,'text','shipped','newest','kind',0,NULL), ('work','commit',5,'text','shipped','newest','kind',0,NULL), ('work','depends_on',4,'list:reference','shipped','newest','kind',0,NULL), ('work','owner',2,'reference','shipped','newest','kind',0,NULL), ('work','status',1,'text','shipped','newest','kind',0,'someday,next,now,waiting,done'), ('work','verified_by',6,'text','shipped','newest','kind',0,NULL), ('work','waiting_on',3,'reference','shipped','newest','kind',0,NULL);
