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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','w4b8f3',NULL,NULL,NULL), ('bot:upgrade-fixture-lead','bot','A Recorded Lead','test',NULL,NULL,'on-demand','','jcdfvq',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','x9ye1x',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','98nz2d',NULL,NULL,NULL), ('project:upgrade-fixture-project','project','A Recorded Project','test',NULL,NULL,'on-demand','','z75j3t',NULL,NULL,NULL), ('thing:blue-kite','thing','A Recorded Spare Kite','test',NULL,NULL,'on-demand','','rsavmx',NULL,NULL,NULL), ('thing:recorded-twin','thing','A Spare Copy','test',NULL,NULL,'on-demand','','9xa5bv','thing:upgrade-fixture-thing',NULL,NULL), ('thing:red-kite','thing','A Recorded Kite','test',NULL,NULL,'on-demand','','eqmyh3',NULL,NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','07d50s',NULL,NULL,NULL), ('topic:instance','topic','This instance','test',NULL,NULL,'on-demand','','0f2pzh',NULL,NULL,NULL), ('work:upgrade-fixture-prior','work','A Recorded Prior Task','test',NULL,'z75j3t','on-demand','','1stecx',NULL,NULL,NULL), ('work:upgrade-fixture-task','work','A Recorded Task','test',NULL,'z75j3t','on-demand','','qqcbab',NULL,NULL,NULL);
DROP TABLE IF EXISTS `entity_alias`;
CREATE TABLE `entity_alias` (
  `entity` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `alias` text NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_alias` (`entity`,`ordinal`,`alias`) VALUES ('07d50s',1,'A Spare Copy');
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
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-08T05:20:06.861720946Z'), ('bot:upgrade-fixture-lead',1,'2026-10-08T05:20:12.702017643Z'), ('person:upgrade-fixture-person',1,'2026-10-08T05:20:07.287539275Z'), ('place:upgrade-fixture-place',1,'2026-10-08T05:20:07.43331409Z'), ('project:upgrade-fixture-project',1,'2026-10-08T05:20:10.295738172Z'), ('thing:blue-kite',1,'2026-10-08T05:20:09.803499161Z'), ('thing:recorded-twin',1,'2026-10-08T05:20:08.608002892Z'), ('thing:recorded-twin',2,'2026-10-08T05:20:09.045183857Z'), ('thing:red-kite',1,'2026-10-08T05:20:09.359346415Z'), ('thing:upgrade-fixture-thing',1,'2026-10-08T05:20:07.799772408Z'), ('thing:upgrade-fixture-thing',2,'2026-10-08T05:20:09.031552475Z'), ('topic:instance',1,'2026-10-08T05:20:13.448896307Z'), ('work:upgrade-fixture-prior',1,'2026-10-08T05:20:10.457484242Z'), ('work:upgrade-fixture-task',1,'2026-10-08T05:20:10.619574912Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('07d50s','f1','the twin carried a note',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:08.767678422Z',NULL,NULL,NULL), ('07d50s','f2','recorded as one thing',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:09.033874176Z',NULL,NULL,NULL), ('07d50s','f3','a day kept under decide_by',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:11.57245333Z',NULL,NULL,NULL), ('0f2pzh','f1','the instance works in one zone',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:13.622396637Z',NULL,NULL,NULL), ('1stecx','f1','the prior task is finished',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:11.177426601Z',NULL,NULL,NULL), ('98nz2d','f1','the recorded place opens at nine',NULL,'observation','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:12.467892407Z',NULL,NULL,NULL), ('eqmyh3','f1','where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:09.509071204Z',NULL,NULL,NULL), ('qqcbab','f1','the recorded task is in review',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:11.374929716Z',NULL,NULL,NULL), ('rsavmx','f1','where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:10.03462735Z',NULL,NULL,NULL), ('w4b8f3','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,NULL,NULL), ('w4b8f3','f2','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-08','connection','07d50s',NULL,NULL,NULL,'2026-10-08T05:20:11.890951639Z',NULL,NULL,NULL), ('w4b8f3','f3','the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:12.135768359Z',NULL,NULL,NULL), ('w4b8f3','f4','the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:12.960022242Z',NULL,NULL,NULL), ('w4b8f3','f5','the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:13.116988552Z',NULL,NULL,NULL), ('w4b8f3','f6','claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:15.099835877Z',NULL,NULL,NULL), ('x9ye1x','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-08','location','98nz2d',NULL,NULL,NULL,'2026-10-08T05:20:08.046601657Z',NULL,NULL,NULL), ('x9ye1x','f2','visited the recorded place',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,'x9ye1x','f1','2026-10-08T05:20:08.296979694Z',NULL,NULL,NULL), ('z75j3t','f1','the recorded project is under way',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:20:10.880766894Z',NULL,NULL,NULL);
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
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`) VALUES ('07d50s','f1',1,'the twin carried a note',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:08.767678422Z',NULL,'2026-10-08T05:20:08.771789443Z',NULL,NULL), ('07d50s','f2',1,'recorded as one thing',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:09.033874176Z',NULL,'2026-10-08T05:20:09.040257246Z',NULL,NULL), ('07d50s','f3',1,'a day kept under decide_by',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:11.57245333Z',NULL,'2026-10-08T05:20:11.57654215Z',NULL,NULL), ('0f2pzh','f1',1,'the instance works in one zone',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:13.622396637Z',NULL,'2026-10-08T05:20:13.626528616Z',NULL,NULL), ('1stecx','f1',1,'the prior task is finished',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:11.177426601Z',NULL,'2026-10-08T05:20:11.183018942Z',NULL,NULL), ('98nz2d','f1',1,'the recorded place opens at nine',NULL,'observation','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:12.467892407Z',NULL,'2026-10-08T05:20:12.471825161Z',NULL,NULL), ('eqmyh3','f1',1,'where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:09.509071204Z',NULL,'2026-10-08T05:20:09.512560875Z',NULL,NULL), ('qqcbab','f1',1,'the recorded task is in review',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:11.374929716Z',NULL,'2026-10-08T05:20:11.380503876Z',NULL,NULL), ('rsavmx','f1',1,'where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:10.03462735Z',NULL,'2026-10-08T05:20:10.03874297Z',NULL,NULL), ('w4b8f3','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:07.197753821Z',NULL,NULL), ('w4b8f3','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:07.343796775Z',NULL,NULL), ('w4b8f3','f1',3,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:07.596804078Z',NULL,NULL), ('w4b8f3','f1',4,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:07.855892346Z',NULL,NULL), ('w4b8f3','f1',5,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:08.130088604Z',NULL,NULL), ('w4b8f3','f1',6,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:08.486985818Z',NULL,NULL), ('w4b8f3','f1',7,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:08.66738859Z',NULL,NULL), ('w4b8f3','f1',8,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:08.852074222Z',NULL,NULL), ('w4b8f3','f1',9,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:09.269319325Z',NULL,NULL), ('w4b8f3','f1',10,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:09.415457672Z',NULL,NULL), ('w4b8f3','f1',11,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:09.687876857Z',NULL,NULL), ('w4b8f3','f1',12,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:09.862923528Z',NULL,NULL), ('w4b8f3','f1',13,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:10.11964945Z',NULL,NULL), ('w4b8f3','f1',14,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:10.35848479Z',NULL,NULL), ('w4b8f3','f1',15,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:10.52355391Z',NULL,NULL), ('w4b8f3','f1',16,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:10.679353409Z',NULL,NULL), ('w4b8f3','f1',17,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:11.064342727Z',NULL,NULL), ('w4b8f3','f1',18,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:11.260072081Z',NULL,NULL), ('w4b8f3','f1',19,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:11.464008027Z',NULL,NULL), ('w4b8f3','f1',20,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:11.665276842Z',NULL,NULL), ('w4b8f3','f1',21,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:11.97995707Z',NULL,NULL), ('w4b8f3','f1',22,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:12.33266533Z',NULL,NULL), ('w4b8f3','f1',23,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:12.548735386Z',NULL,NULL), ('w4b8f3','f1',24,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:12.762538208Z',NULL,NULL), ('w4b8f3','f1',25,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:13.217349798Z',NULL,NULL), ('w4b8f3','f1',26,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:13.508060105Z',NULL,NULL), ('w4b8f3','f1',27,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:13.706429335Z',NULL,NULL), ('w4b8f3','f1',28,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:13.89469531Z',NULL,NULL), ('w4b8f3','f1',29,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:14.055732808Z',NULL,NULL), ('w4b8f3','f1',30,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:14.346760849Z',NULL,NULL), ('w4b8f3','f1',31,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:14.664077676Z',NULL,NULL), ('w4b8f3','f1',32,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:14.794704949Z',NULL,NULL), ('w4b8f3','f1',33,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:07.192339021Z',NULL,'2026-10-08T05:20:14.92789708Z',NULL,NULL), ('w4b8f3','f2',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-08','connection','07d50s',NULL,NULL,'2026-10-08T05:20:11.890951639Z',NULL,'2026-10-08T05:20:11.895280199Z',NULL,NULL), ('w4b8f3','f3',1,'the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:12.135768359Z',NULL,'2026-10-08T05:20:12.139781493Z',NULL,NULL), ('w4b8f3','f4',1,'the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:12.960022242Z',NULL,'2026-10-08T05:20:12.964705111Z',NULL,NULL), ('w4b8f3','f5',1,'the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:13.116988552Z',NULL,'2026-10-08T05:20:13.121650549Z',NULL,NULL), ('w4b8f3','f6',1,'claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:15.099835877Z',NULL,'2026-10-08T05:20:15.104458377Z',NULL,NULL), ('x9ye1x','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-08','location','98nz2d',NULL,NULL,'2026-10-08T05:20:08.046601657Z',NULL,'2026-10-08T05:20:08.050390554Z',NULL,NULL), ('x9ye1x','f2',1,'visited the recorded place',NULL,'inference','open','active','2026-10-08',NULL,NULL,'x9ye1x','f1','2026-10-08T05:20:08.296979694Z',NULL,'2026-10-08T05:20:08.300918254Z',NULL,NULL), ('z75j3t','f1',1,'the recorded project is under way',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:20:10.880766894Z',NULL,'2026-10-08T05:20:10.884625844Z',NULL,NULL);
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
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('07d50s','decide_by',1,'2026-12-01','f3'), ('07d50s','due_on',1,'2026-12-01','f3'), ('07d50s','merged_from',1,'thing:recorded-twin','f2'), ('0f2pzh','timezone',1,'America/New_York','f1'), ('1stecx','status',1,'done','f1'), ('98nz2d','read_from',1,'the recorded place\'s page','f1'), ('98nz2d','read_ref',1,'opening hours','f1'), ('eqmyh3','status',1,'now','f1'), ('qqcbab','depends_on',1,'1stecx','f1'), ('qqcbab','owner',1,'x9ye1x','f1'), ('qqcbab','status',1,'review','f1'), ('rsavmx','status',1,'done','f1'), ('w4b8f3','pairs_with',1,'bot:upgrade-fixture-lead','f5'), ('w4b8f3','reports_to',1,'jcdfvq','f4'), ('w4b8f3','role/upgrade-fixture-holder/claimed_at',1,'2026-10-08T05:20:15.07593385Z','f6'), ('w4b8f3','role/upgrade-fixture-holder/holder',1,'387r','f6'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',1,'2026-10-08T05:20:07.18563927Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',2,'2026-10-08T05:20:07.325531952Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',3,'2026-10-08T05:20:07.57307728Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',4,'2026-10-08T05:20:07.837487142Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',5,'2026-10-08T05:20:08.110246791Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',6,'2026-10-08T05:20:08.467651775Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',7,'2026-10-08T05:20:08.648542898Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',8,'2026-10-08T05:20:08.82974972Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',9,'2026-10-08T05:20:09.249308092Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',10,'2026-10-08T05:20:09.397954631Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',11,'2026-10-08T05:20:09.668463654Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',12,'2026-10-08T05:20:09.842701765Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',13,'2026-10-08T05:20:10.099756437Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',14,'2026-10-08T05:20:10.337648557Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',15,'2026-10-08T05:20:10.501808307Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',16,'2026-10-08T05:20:10.660433997Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',17,'2026-10-08T05:20:11.044262905Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',18,'2026-10-08T05:20:11.237546228Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',19,'2026-10-08T05:20:11.445337774Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',20,'2026-10-08T05:20:11.642765579Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',21,'2026-10-08T05:20:11.960277928Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',22,'2026-10-08T05:20:12.308874146Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',23,'2026-10-08T05:20:12.528863865Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',24,'2026-10-08T05:20:12.742078315Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',25,'2026-10-08T05:20:13.197701879Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',26,'2026-10-08T05:20:13.486115943Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',27,'2026-10-08T05:20:13.682774439Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',28,'2026-10-08T05:20:13.873755033Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',29,'2026-10-08T05:20:14.035161641Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',30,'2026-10-08T05:20:14.324350342Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',31,'2026-10-08T05:20:14.641076809Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',32,'2026-10-08T05:20:14.765470773Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/claimed_at',33,'1970-01-01T00:00:00Z','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',1,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',2,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',3,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',4,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',5,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',6,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',7,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',8,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',9,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',10,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',11,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',12,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',13,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',14,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',15,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',16,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',17,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',18,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',19,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',20,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',21,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',22,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',23,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',24,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',25,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',26,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',27,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',28,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',29,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',30,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',31,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',32,'h5qq','f1'), ('w4b8f3','role/upgrade-fixture-recorder/holder',33,NULL,'f1'), ('w4b8f3','starred',1,'true','f3'), ('x9ye1x','since',1,'2026-01-01','f1'), ('x9ye1x','venue',1,'98nz2d','f2'), ('z75j3t','columns',1,'someday, next, now, waiting, done, review','f1'), ('z75j3t','status',1,'now','f1');
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
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`) VALUES ('2eyrmr','6mzrw2',1,'2026-10-08T05:20:13.064956087Z','captured facts about: bot:assistant (1)',NULL,'capture',NULL,NULL), ('jcz7ha','4svzgz',5,'2026-10-08T05:20:14.748163835Z','retired messages: mnqe1s (1)',NULL,'mark_processed',NULL,NULL), ('jcz7ha','5a4cyc',4,'2026-10-08T05:20:13.97514384Z','posted to mailboxes: assistant, … (3)','2026-10-08T05:20:14.433082227Z','post_message',NULL,NULL), ('jcz7ha','hz3nfm',6,'2026-10-08T05:20:14.765470773Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL), ('jcz7ha','k7x6dn',1,'2026-10-08T05:20:07.408292845Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing, thing:recorded-twin, thing:red-kite, … (11)','2026-10-08T05:20:13.594995651Z','add_entity',NULL,NULL), ('jcz7ha','x2j234',3,'2026-10-08T05:20:09.336808774Z','merged entities: thing:recorded-twin (1)',NULL,'merge_entities',NULL,NULL), ('jcz7ha','yer5hh',2,'2026-10-08T05:20:08.197028292Z','captured facts about: person:upgrade-fixture-person, thing:recorded-twin, thing:red-kite, thing:blue-kite, project:upgrade-fixture-project, … (14)','2026-10-08T05:20:13.794366664Z','capture',NULL,NULL);
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
INSERT INTO `mailbox` (`name`,`owner`) VALUES ('assistant','bot:assistant'), ('upgrade-fixture-lead','bot:upgrade-fixture-lead');
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
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('4yj99r','assistant',1,'left new',NULL,'w4b8f3','2026-10-08T05:20:13.839569447Z','new',NULL,NULL,0,'jcz7ha',NULL,NULL,NULL), ('mnqe1s','assistant',3,'will be processed',NULL,'w4b8f3','2026-10-08T05:20:14.291276796Z','processed',NULL,NULL,2,'jcz7ha',NULL,NULL,NULL), ('xnn3e7','assistant',2,'will be read',NULL,'w4b8f3','2026-10-08T05:20:14.002851306Z','read',NULL,NULL,1,'jcz7ha',NULL,NULL,NULL);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('xnn3e7','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-08T05:20:06.055384106Z'), ('0002_journal_entry','2026-10-08T05:20:06.064335428Z'), ('0003_minted','2026-10-08T05:20:06.07298853Z'), ('0004_mailbox','2026-10-08T05:20:06.080648281Z'), ('0005_message','2026-10-08T05:20:06.090500042Z'), ('0006_handover','2026-10-08T05:20:06.097444953Z'), ('0007_drop_minted','2026-10-08T05:20:06.103756884Z'), ('0008_entity','2026-10-08T05:20:06.111358975Z'), ('0009_entity_alias','2026-10-08T05:20:06.119085917Z'), ('0010_fact','2026-10-08T05:20:06.127671198Z'), ('0011_fact_event_metadata','2026-10-08T05:20:06.13662467Z'), ('0012_fact_event_ref','2026-10-08T05:20:06.145340022Z'), ('0013_type_field','2026-10-08T05:20:06.153140232Z'), ('0014_message_delivery','2026-10-08T05:20:06.161409953Z'), ('0015_type_field_origin','2026-10-08T05:20:06.170533795Z'), ('0016_field_write','2026-10-08T05:20:06.179175607Z'), ('0017_type_field_folds','2026-10-08T05:20:06.188150617Z'), ('0018_type_field_holds_wider','2026-10-08T05:20:06.198386879Z'), ('0019_entity_source_wider','2026-10-08T05:20:06.207288661Z'), ('0020_entity_crm_wider','2026-10-08T05:20:06.215043323Z'), ('0021_kind','2026-10-08T05:20:06.223920623Z'), ('0022_type_field_owner','2026-10-08T05:20:06.233061815Z'), ('0023_type_field_required','2026-10-08T05:20:06.240777477Z'), ('0024_rhythm_kind_takes_its_name','2026-10-08T05:20:06.247421378Z'), ('0025_type_field_one_of','2026-10-08T05:20:06.255721649Z'), ('0026_session_timezone','2026-10-08T05:20:06.264035401Z'), ('0027_fact_inserted_at','2026-10-08T05:20:06.272549052Z'), ('0028_fact_stale_after','2026-10-08T05:20:06.280964792Z'), ('0029_fact_by_source','2026-10-08T05:20:06.289918094Z'), ('0030_session_stated_day','2026-10-08T05:20:06.297977656Z'), ('0031_journal_entry_day','2026-10-08T05:20:06.306239827Z'), ('0032_entity_badge','2026-10-08T05:20:06.314105169Z'), ('0033_entity_merged_into','2026-10-08T05:20:06.32238827Z'), ('0034_fact_write','2026-10-08T05:20:06.332455791Z'), ('0035_fact_write_backfill','2026-10-08T05:20:06.338871852Z'), ('0036_fact_write_moment','2026-10-08T05:20:06.346615774Z'), ('0037_fact_happened_at','2026-10-08T05:20:06.354735325Z'), ('0038_fact_write_happened_at','2026-10-08T05:20:06.362809737Z'), ('0039_fact_recorded_at','2026-10-08T05:20:06.370575988Z'), ('0040_fact_write_recorded_at','2026-10-08T05:20:06.377890979Z'), ('0041_session_teaching','2026-10-08T05:20:06.3854174Z'), ('0042_entity_former_handle','2026-10-08T05:20:06.393684751Z'), ('0043_message_sender_mail_waiting','2026-10-08T05:20:06.401814122Z'), ('0044_fact_stands_for','2026-10-08T05:20:06.410905024Z'), ('0045_entity_former_handle_key_add','2026-10-08T05:20:06.44783219Z'), ('0045_entity_former_handle_key_drop','2026-10-08T05:20:06.439547269Z'), ('0045_entity_former_handle_ordinal','2026-10-08T05:20:06.429074667Z'), ('0045_entity_former_handle_width','2026-10-08T05:20:06.420023016Z'), ('0046_fact_status_archived','2026-10-08T05:20:06.454801491Z'), ('0047_fact_write_status_archived','2026-10-08T05:20:06.461603942Z'), ('0048_session_served_chars','2026-10-08T05:20:06.470054814Z'), ('0049_entity_archived','2026-10-08T05:20:06.478612555Z'), ('0050_entity_archived_at','2026-10-08T05:20:06.487860316Z'), ('0051_entity_write','2026-10-08T05:20:06.497716248Z'), ('0052_fact_happened_through','2026-10-08T05:20:06.506816789Z'), ('0053_fact_write_happened_through','2026-10-08T05:20:06.517410691Z'), ('0054_session_write','2026-10-08T05:20:06.526171993Z'), ('0055_message_posted_by_session','2026-10-08T05:20:06.535605494Z'), ('0056_session_stated_day','2026-10-08T05:20:06.546319386Z'), ('0057_message_quarantined_by','2026-10-08T05:20:06.555489198Z'), ('0058_message_quarantined_at','2026-10-08T05:20:06.564917189Z'), ('0059_message_quarantine_reason','2026-10-08T05:20:06.575906131Z'), ('0060_journal_entry_closing_focus','2026-10-08T05:20:06.585479432Z'), ('0061_displaced_type_field','2026-10-08T05:20:06.594790264Z'), ('0062_message_by_sender','2026-10-08T05:20:06.602386335Z');
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
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`) VALUES ('2eyrmr','5257','jcdfvq','working','2026-10-08T05:20:13.047789473Z','active',NULL,NULL,2865,NULL), ('jcz7ha','h5qq','w4b8f3','claimed the upgrade-fixture-recorder role','2026-10-08T05:20:07.260103991Z','wrapped',NULL,NULL,25040,NULL), ('wtgypr','387r','w4b8f3','claimed the upgrade-fixture-holder role','2026-10-08T05:20:15.188398755Z','active',NULL,NULL,0,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('5257','claim-subject','2026-10-08T05:20:13.091780449Z'), ('5257','claims','2026-10-08T05:20:13.088562069Z'), ('h5qq','claim-direction','2026-10-08T05:20:08.221557784Z'), ('h5qq','claim-subject','2026-10-08T05:20:08.217862744Z'), ('h5qq','claims','2026-10-08T05:20:08.214329844Z'), ('h5qq','field-shadows-argument','2026-10-08T05:20:09.785418508Z'), ('h5qq','projects-skill','2026-10-08T05:20:10.437414599Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('2eyrmr',1), ('2eyrmr',2), ('jcz7ha',1), ('jcz7ha',2), ('jcz7ha',3), ('jcz7ha',4), ('jcz7ha',5), ('jcz7ha',6), ('jcz7ha',7), ('jcz7ha',8), ('jcz7ha',9), ('jcz7ha',10), ('jcz7ha',11), ('jcz7ha',12), ('jcz7ha',13), ('jcz7ha',14), ('jcz7ha',15), ('jcz7ha',16), ('jcz7ha',17), ('jcz7ha',18), ('jcz7ha',19), ('jcz7ha',20), ('jcz7ha',21), ('jcz7ha',22), ('jcz7ha',23), ('jcz7ha',24), ('jcz7ha',25), ('jcz7ha',26), ('jcz7ha',27), ('jcz7ha',28), ('jcz7ha',29), ('jcz7ha',30), ('jcz7ha',31), ('jcz7ha',32), ('wtgypr',1);
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
INSERT INTO `type_field` (`type_name`,`key_name`,`ordinal`,`holds`,`origin`,`folds`,`owner`,`required`,`one_of`) VALUES ('bot','reports_to',1,'reference:bot','shipped','newest','kind',0,NULL), ('decide-by','decide_by',1,'date','shipped','newest','type',0,NULL), ('entitlement','admits',1,'reference','shipped','newest','type',1,NULL), ('entitlement','tier',4,'text','shipped','newest','type',0,NULL), ('entitlement','valid_from',2,'date','shipped','newest','type',0,NULL), ('entitlement','valid_until',3,'date','shipped','newest','type',0,NULL), ('project','columns',2,'list:text','shipped','newest','kind',0,NULL), ('project','status',1,'text','shipped','newest','kind',0,'someday,next,now,waiting,done'), ('promise','ended',3,'text','shipped','newest','kind',0,'delivered,withdrawn,overtaken'), ('promise','promised_by',1,'date','shipped','newest','kind',1,NULL), ('promise','regarding',2,'reference','shipped','newest','kind',0,NULL), ('record-labels','purpose',5,'text','shipped','describes','type',0,NULL), ('record-labels','read_from',1,'text','shipped','describes','type',0,NULL), ('record-labels','read_ref',2,'text','shipped','describes','type',0,NULL), ('record-labels','recorded_by',6,'text','shipped','describes','type',0,NULL), ('record-labels','starred',3,'text','shipped','describes','type',0,NULL), ('record-labels','subject',4,'text','shipped','describes','type',0,NULL), ('rhythm','advances_from',5,'text','shipped','newest','kind',0,'due_date,check_in_date'), ('rhythm','cadence_days',4,'number','shipped','newest','kind',0,NULL), ('rhythm','counts_from',6,'date','shipped','newest','kind',0,NULL), ('rhythm','last_check_in',2,'date','shipped','newest','kind',1,NULL), ('rhythm','name',1,'text','shipped','newest','kind',1,NULL), ('rhythm','note',3,'text','shipped','newest','kind',0,NULL), ('rhythm','outcome',8,'text','shipped','newest','kind',0,'ran,skipped,snoozed'), ('rhythm','snoozed_until',7,'date','shipped','newest','kind',0,NULL), ('runs-out','runs_out',1,'date','shipped','newest','type',0,NULL), ('trip','arrives_at',2,'reference','shipped','newest','type',1,NULL), ('trip','departs_from',1,'reference','shipped','newest','type',1,NULL), ('trip','leaves_on',3,'date','shipped','newest','type',1,NULL), ('trip','returns_on',4,'date','shipped','newest','type',1,NULL), ('upgrade-fixture-type','status',1,'text','declared','newest','type',0,'open,closed'), ('upgrade-fixture-visit','venue',1,'reference:place','declared','newest','type',0,NULL), ('view','asks',3,'text','shipped','newest','kind',0,'overdue'), ('view','selects',1,'text','shipped','newest','kind',1,NULL), ('view','shows',2,'text','shipped','newest','kind',0,NULL), ('work','commit',5,'text','shipped','newest','kind',0,NULL), ('work','depends_on',4,'list:reference','shipped','newest','kind',0,NULL), ('work','owner',2,'reference','shipped','newest','kind',0,NULL), ('work','status',1,'text','shipped','newest','kind',0,'someday,next,now,waiting,done'), ('work','verified_by',6,'text','shipped','newest','kind',0,NULL), ('work','waiting_on',3,'reference','shipped','newest','kind',0,NULL);
