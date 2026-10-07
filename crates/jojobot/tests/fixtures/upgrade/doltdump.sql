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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','fnfwwr',NULL,NULL,NULL), ('bot:upgrade-fixture-lead','bot','A Recorded Lead','test',NULL,NULL,'on-demand','','2gfwm3',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','vzewt8',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','frznhy',NULL,NULL,NULL), ('project:upgrade-fixture-project','project','A Recorded Project','test',NULL,NULL,'on-demand','','efce87',NULL,NULL,NULL), ('thing:recorded-twin','thing','A Spare Copy','test',NULL,NULL,'on-demand','','ztjrxb','thing:upgrade-fixture-thing',NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','7ew332',NULL,NULL,NULL), ('topic:instance','topic','This instance','test',NULL,NULL,'on-demand','','wnm5nb',NULL,NULL,NULL), ('work:upgrade-fixture-prior','work','A Recorded Prior Task','test',NULL,'efce87','on-demand','','k5hpqb',NULL,NULL,NULL), ('work:upgrade-fixture-task','work','A Recorded Task','test',NULL,'efce87','on-demand','','dd87dd',NULL,NULL,NULL);
DROP TABLE IF EXISTS `entity_alias`;
CREATE TABLE `entity_alias` (
  `entity` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `alias` text NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_alias` (`entity`,`ordinal`,`alias`) VALUES ('7ew332',1,'A Spare Copy');
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
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-07T23:08:11.252791908Z'), ('bot:upgrade-fixture-lead',1,'2026-10-07T23:08:16.125729897Z'), ('person:upgrade-fixture-person',1,'2026-10-07T23:08:11.691398485Z'), ('place:upgrade-fixture-place',1,'2026-10-07T23:08:11.83247012Z'), ('project:upgrade-fixture-project',1,'2026-10-07T23:08:13.550809Z'), ('thing:recorded-twin',1,'2026-10-07T23:08:12.575441413Z'), ('thing:recorded-twin',2,'2026-10-07T23:08:13.203866762Z'), ('thing:upgrade-fixture-thing',1,'2026-10-07T23:08:11.972526288Z'), ('thing:upgrade-fixture-thing',2,'2026-10-07T23:08:13.199570189Z'), ('topic:instance',1,'2026-10-07T23:08:17.115596012Z'), ('work:upgrade-fixture-prior',1,'2026-10-07T23:08:13.798450818Z'), ('work:upgrade-fixture-task',1,'2026-10-07T23:08:14.012392591Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('7ew332','f1','the twin carried a note',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:12.717577964Z',NULL,NULL,NULL), ('7ew332','f2','recorded as one thing',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:13.200320836Z',NULL,NULL,NULL), ('7ew332','f3','a day kept under decide_by',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:14.852453445Z',NULL,NULL,NULL), ('dd87dd','f1','the recorded task is in review',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:14.647355364Z',NULL,NULL,NULL), ('efce87','f1','the recorded project is under way',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:14.157722439Z',NULL,NULL,NULL), ('fnfwwr','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,NULL,NULL), ('fnfwwr','f2','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-07','connection','7ew332',NULL,NULL,NULL,'2026-10-07T23:08:15.165453394Z',NULL,NULL,NULL), ('fnfwwr','f3','the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:15.430724713Z',NULL,NULL,NULL), ('fnfwwr','f4','the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:16.553815809Z',NULL,NULL,NULL), ('fnfwwr','f5','the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:16.774618568Z',NULL,NULL,NULL), ('fnfwwr','f6','claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:18.743325853Z',NULL,NULL,NULL), ('frznhy','f1','the recorded place opens at nine',NULL,'observation','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:15.858615505Z',NULL,NULL,NULL), ('k5hpqb','f1','the prior task is finished',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:14.460168003Z',NULL,NULL,NULL), ('vzewt8','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-07','location','frznhy',NULL,NULL,NULL,'2026-10-07T23:08:12.11981695Z',NULL,NULL,NULL), ('vzewt8','f2','visited the recorded place',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,'vzewt8','f1','2026-10-07T23:08:12.35861018Z',NULL,NULL,NULL), ('wnm5nb','f1','the instance works in one zone',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,NULL,'2026-10-07T23:08:17.27076885Z',NULL,NULL,NULL);
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
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`) VALUES ('7ew332','f1',1,'the twin carried a note',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:12.717577964Z',NULL,'2026-10-07T23:08:12.721584168Z',NULL,NULL), ('7ew332','f2',1,'recorded as one thing',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:13.200320836Z',NULL,'2026-10-07T23:08:13.201891839Z',NULL,NULL), ('7ew332','f3',1,'a day kept under decide_by',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:14.852453445Z',NULL,'2026-10-07T23:08:14.856786761Z',NULL,NULL), ('dd87dd','f1',1,'the recorded task is in review',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:14.647355364Z',NULL,'2026-10-07T23:08:14.653653603Z',NULL,NULL), ('efce87','f1',1,'the recorded project is under way',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:14.157722439Z',NULL,'2026-10-07T23:08:14.161820495Z',NULL,NULL), ('fnfwwr','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:11.497967796Z',NULL,NULL), ('fnfwwr','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:11.743418022Z',NULL,NULL), ('fnfwwr','f1',3,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:11.888080043Z',NULL,NULL), ('fnfwwr','f1',4,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:12.027793702Z',NULL,NULL), ('fnfwwr','f1',5,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:12.19490762Z',NULL,NULL), ('fnfwwr','f1',6,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:12.449155963Z',NULL,NULL), ('fnfwwr','f1',7,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:12.628049507Z',NULL,NULL), ('fnfwwr','f1',8,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:12.889034075Z',NULL,NULL), ('fnfwwr','f1',9,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:13.407487324Z',NULL,NULL), ('fnfwwr','f1',10,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:13.708557483Z',NULL,NULL), ('fnfwwr','f1',11,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:13.855634676Z',NULL,NULL), ('fnfwwr','f1',12,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:14.063522276Z',NULL,NULL), ('fnfwwr','f1',13,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:14.337528624Z',NULL,NULL), ('fnfwwr','f1',14,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:14.536342737Z',NULL,NULL), ('fnfwwr','f1',15,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:14.735910536Z',NULL,NULL), ('fnfwwr','f1',16,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:15.044844739Z',NULL,NULL), ('fnfwwr','f1',17,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:15.257017766Z',NULL,NULL), ('fnfwwr','f1',18,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:15.673581156Z',NULL,NULL), ('fnfwwr','f1',19,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:15.946684089Z',NULL,NULL), ('fnfwwr','f1',20,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:16.286849126Z',NULL,NULL), ('fnfwwr','f1',21,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:16.649644077Z',NULL,NULL), ('fnfwwr','f1',22,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:16.88385354Z',NULL,NULL), ('fnfwwr','f1',23,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:17.167708017Z',NULL,NULL), ('fnfwwr','f1',24,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:17.345647849Z',NULL,NULL), ('fnfwwr','f1',25,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:17.627340582Z',NULL,NULL), ('fnfwwr','f1',26,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:17.773235942Z',NULL,NULL), ('fnfwwr','f1',27,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:17.91804375Z',NULL,NULL), ('fnfwwr','f1',28,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:18.225004459Z',NULL,NULL), ('fnfwwr','f1',29,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:18.444628952Z',NULL,NULL), ('fnfwwr','f1',30,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:11.49161142Z',NULL,'2026-10-07T23:08:18.570029221Z',NULL,NULL), ('fnfwwr','f2',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-07','connection','7ew332',NULL,NULL,'2026-10-07T23:08:15.165453394Z',NULL,'2026-10-07T23:08:15.169932529Z',NULL,NULL), ('fnfwwr','f3',1,'the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:15.430724713Z',NULL,'2026-10-07T23:08:15.438200527Z',NULL,NULL), ('fnfwwr','f4',1,'the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:16.553815809Z',NULL,'2026-10-07T23:08:16.558952641Z',NULL,NULL), ('fnfwwr','f5',1,'the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:16.774618568Z',NULL,'2026-10-07T23:08:16.779633991Z',NULL,NULL), ('fnfwwr','f6',1,'claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:18.743325853Z',NULL,'2026-10-07T23:08:18.748666484Z',NULL,NULL), ('frznhy','f1',1,'the recorded place opens at nine',NULL,'observation','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:15.858615505Z',NULL,'2026-10-07T23:08:15.863347469Z',NULL,NULL), ('k5hpqb','f1',1,'the prior task is finished',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:14.460168003Z',NULL,'2026-10-07T23:08:14.466514692Z',NULL,NULL), ('vzewt8','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-07','location','frznhy',NULL,NULL,'2026-10-07T23:08:12.11981695Z',NULL,'2026-10-07T23:08:12.123979524Z',NULL,NULL), ('vzewt8','f2',1,'visited the recorded place',NULL,'inference','open','active','2026-10-07',NULL,NULL,'vzewt8','f1','2026-10-07T23:08:12.35861018Z',NULL,'2026-10-07T23:08:12.362833795Z',NULL,NULL), ('wnm5nb','f1',1,'the instance works in one zone',NULL,'testimony','settled','active','2026-10-07',NULL,NULL,NULL,NULL,'2026-10-07T23:08:17.27076885Z',NULL,'2026-10-07T23:08:17.274495848Z',NULL,NULL);
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
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('7ew332','decide_by',1,'2026-12-01','f3'), ('7ew332','due_on',1,'2026-12-01','f3'), ('7ew332','merged_from',1,'thing:recorded-twin','f2'), ('dd87dd','depends_on',1,'k5hpqb','f1'), ('dd87dd','owner',1,'vzewt8','f1'), ('dd87dd','status',1,'review','f1'), ('efce87','columns',1,'someday, next, now, waiting, done, review','f1'), ('efce87','status',1,'now','f1'), ('fnfwwr','pairs_with',1,'bot:upgrade-fixture-lead','f5'), ('fnfwwr','reports_to',1,'2gfwm3','f4'), ('fnfwwr','role/upgrade-fixture-holder/claimed_at',1,'2026-10-07T23:08:18.717453968Z','f6'), ('fnfwwr','role/upgrade-fixture-holder/holder',1,'20mr','f6'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',1,'2026-10-07T23:08:11.48340917Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',2,'2026-10-07T23:08:11.722683539Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',3,'2026-10-07T23:08:11.864347872Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',4,'2026-10-07T23:08:12.007420139Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',5,'2026-10-07T23:08:12.175900881Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',6,'2026-10-07T23:08:12.425510591Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',7,'2026-10-07T23:08:12.607348833Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',8,'2026-10-07T23:08:12.86858078Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',9,'2026-10-07T23:08:13.386036694Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',10,'2026-10-07T23:08:13.685842387Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',11,'2026-10-07T23:08:13.834144615Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',12,'2026-10-07T23:08:14.043080674Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',13,'2026-10-07T23:08:14.315758748Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',14,'2026-10-07T23:08:14.515005728Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',15,'2026-10-07T23:08:14.715536035Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',16,'2026-10-07T23:08:15.022734813Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',17,'2026-10-07T23:08:15.232867377Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',18,'2026-10-07T23:08:15.631375618Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',19,'2026-10-07T23:08:15.921254744Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',20,'2026-10-07T23:08:16.260978773Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',21,'2026-10-07T23:08:16.629715734Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',22,'2026-10-07T23:08:16.859849491Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',23,'2026-10-07T23:08:17.14594115Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',24,'2026-10-07T23:08:17.323150045Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',25,'2026-10-07T23:08:17.602986795Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',26,'2026-10-07T23:08:17.751981214Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',27,'2026-10-07T23:08:17.896028031Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',28,'2026-10-07T23:08:18.202282322Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',29,'2026-10-07T23:08:18.416696274Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/claimed_at',30,'1970-01-01T00:00:00Z','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',1,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',2,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',3,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',4,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',5,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',6,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',7,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',8,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',9,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',10,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',11,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',12,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',13,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',14,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',15,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',16,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',17,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',18,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',19,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',20,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',21,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',22,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',23,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',24,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',25,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',26,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',27,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',28,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',29,'nsgb','f1'), ('fnfwwr','role/upgrade-fixture-recorder/holder',30,NULL,'f1'), ('fnfwwr','starred',1,'true','f3'), ('frznhy','read_from',1,'the recorded place\'s page','f1'), ('frznhy','read_ref',1,'opening hours','f1'), ('k5hpqb','status',1,'done','f1'), ('vzewt8','since',1,'2026-01-01','f1'), ('vzewt8','venue',1,'frznhy','f2'), ('wnm5nb','timezone',1,'America/New_York','f1');
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
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`) VALUES ('hsqv7d','45ghz2',6,'2026-10-07T23:08:18.416696274Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL), ('hsqv7d','534fh1',2,'2026-10-07T23:08:12.254926147Z','captured facts about: person:upgrade-fixture-person, thing:recorded-twin, project:upgrade-fixture-project, work:upgrade-fixture-prior, work:upgrade-fixture-task, … (13)','2026-10-07T23:08:17.526216432Z','capture',NULL,NULL), ('hsqv7d','e8xean',4,'2026-10-07T23:08:17.701312655Z','posted to mailboxes: assistant, … (3)','2026-10-07T23:08:17.990447102Z','post_message',NULL,NULL), ('hsqv7d','k7tp00',5,'2026-10-07T23:08:18.399741506Z','retired messages: r0wskp (1)',NULL,'mark_processed',NULL,NULL), ('hsqv7d','kav7qx',1,'2026-10-07T23:08:11.804482134Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing, thing:recorded-twin, project:upgrade-fixture-project, … (9)','2026-10-07T23:08:17.245153967Z','add_entity',NULL,NULL), ('hsqv7d','ykrzcp',3,'2026-10-07T23:08:13.471399476Z','merged entities: thing:recorded-twin (1)',NULL,'merge_entities',NULL,NULL);
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
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('7tf4tq','assistant',2,'will be read',NULL,'fnfwwr','2026-10-07T23:08:17.728680112Z','read',NULL,NULL,1,'hsqv7d',NULL,NULL,NULL), ('r0wskp','assistant',3,'will be processed',NULL,'fnfwwr','2026-10-07T23:08:17.871770561Z','processed',NULL,NULL,2,'hsqv7d',NULL,NULL,NULL), ('zrj56c','assistant',1,'left new',NULL,'fnfwwr','2026-10-07T23:08:17.578168398Z','new',NULL,NULL,0,'hsqv7d',NULL,NULL,NULL);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('7tf4tq','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-07T23:08:10.519012971Z'), ('0002_journal_entry','2026-10-07T23:08:10.528430915Z'), ('0003_minted','2026-10-07T23:08:10.537764321Z'), ('0004_mailbox','2026-10-07T23:08:10.547075436Z'), ('0005_message','2026-10-07T23:08:10.556199393Z'), ('0006_handover','2026-10-07T23:08:10.56511182Z'), ('0007_drop_minted','2026-10-07T23:08:10.572758061Z'), ('0008_entity','2026-10-07T23:08:10.582934523Z'), ('0009_entity_alias','2026-10-07T23:08:10.591144473Z'), ('0010_fact','2026-10-07T23:08:10.601545824Z'), ('0011_fact_event_metadata','2026-10-07T23:08:10.609335465Z'), ('0012_fact_event_ref','2026-10-07T23:08:10.617720984Z'), ('0013_type_field','2026-10-07T23:08:10.625567224Z'), ('0014_message_delivery','2026-10-07T23:08:10.633916914Z'), ('0015_type_field_origin','2026-10-07T23:08:10.642026543Z'), ('0016_field_write','2026-10-07T23:08:10.651723407Z'), ('0017_type_field_folds','2026-10-07T23:08:10.660631224Z'), ('0018_type_field_holds_wider','2026-10-07T23:08:10.669189672Z'), ('0019_entity_source_wider','2026-10-07T23:08:10.678227038Z'), ('0020_entity_crm_wider','2026-10-07T23:08:10.686615657Z'), ('0021_kind','2026-10-07T23:08:10.695819512Z'), ('0022_type_field_owner','2026-10-07T23:08:10.705659576Z'), ('0023_type_field_required','2026-10-07T23:08:10.714994281Z'), ('0024_rhythm_kind_takes_its_name','2026-10-07T23:08:10.722421293Z'), ('0025_type_field_one_of','2026-10-07T23:08:10.7314827Z'), ('0026_session_timezone','2026-10-07T23:08:10.740834145Z'), ('0027_fact_inserted_at','2026-10-07T23:08:10.749845042Z'), ('0028_fact_stale_after','2026-10-07T23:08:10.759114237Z'), ('0029_fact_by_source','2026-10-07T23:08:10.768815591Z'), ('0030_session_stated_day','2026-10-07T23:08:10.777945287Z'), ('0031_journal_entry_day','2026-10-07T23:08:10.787046483Z'), ('0032_entity_badge','2026-10-07T23:08:10.797097446Z'), ('0033_entity_merged_into','2026-10-07T23:08:10.805898693Z'), ('0034_fact_write','2026-10-07T23:08:10.815860086Z'), ('0035_fact_write_backfill','2026-10-07T23:08:10.823488417Z'), ('0036_fact_write_moment','2026-10-07T23:08:10.832307965Z'), ('0037_fact_happened_at','2026-10-07T23:08:10.842153898Z'), ('0038_fact_write_happened_at','2026-10-07T23:08:10.850421957Z'), ('0039_fact_recorded_at','2026-10-07T23:08:10.858450246Z'), ('0040_fact_write_recorded_at','2026-10-07T23:08:10.868907588Z'), ('0041_session_teaching','2026-10-07T23:08:10.877772655Z'), ('0042_entity_former_handle','2026-10-07T23:08:10.886630932Z'), ('0043_message_sender_mail_waiting','2026-10-07T23:08:10.907988742Z'), ('0044_fact_stands_for','2026-10-07T23:08:10.917378527Z'), ('0045_entity_former_handle_key_add','2026-10-07T23:08:10.950189596Z'), ('0045_entity_former_handle_key_drop','2026-10-07T23:08:10.942162015Z'), ('0045_entity_former_handle_ordinal','2026-10-07T23:08:10.934008516Z'), ('0045_entity_former_handle_width','2026-10-07T23:08:10.925592927Z'), ('0046_fact_status_archived','2026-10-07T23:08:10.956806391Z'), ('0047_fact_write_status_archived','2026-10-07T23:08:10.964006744Z'), ('0048_session_served_chars','2026-10-07T23:08:10.972324953Z'), ('0049_entity_archived','2026-10-07T23:08:10.980574863Z'), ('0050_entity_archived_at','2026-10-07T23:08:10.98929651Z'), ('0051_entity_write','2026-10-07T23:08:10.9973724Z'), ('0052_fact_happened_through','2026-10-07T23:08:11.006167817Z'), ('0053_fact_write_happened_through','2026-10-07T23:08:11.015355783Z'), ('0054_session_write','2026-10-07T23:08:11.023637502Z'), ('0055_message_posted_by_session','2026-10-07T23:08:11.032578889Z'), ('0056_session_stated_day','2026-10-07T23:08:11.041181507Z'), ('0057_message_quarantined_by','2026-10-07T23:08:11.049249736Z'), ('0058_message_quarantined_at','2026-10-07T23:08:11.057367477Z'), ('0059_message_quarantine_reason','2026-10-07T23:08:11.065446216Z'), ('0060_journal_entry_closing_focus','2026-10-07T23:08:11.073369497Z'), ('0061_displaced_type_field','2026-10-07T23:08:11.081873635Z'), ('0062_message_by_sender','2026-10-07T23:08:11.089835096Z');
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
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`) VALUES ('hsqv7d','nsgb','fnfwwr','claimed the upgrade-fixture-recorder role','2026-10-07T23:08:11.661221858Z','wrapped',NULL,NULL,23915,NULL), ('th5r9w','20mr','fnfwwr','claimed the upgrade-fixture-holder role','2026-10-07T23:08:18.825434241Z','active',NULL,NULL,0,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('nsgb','claim-direction','2026-10-07T23:08:12.281150059Z'), ('nsgb','claim-subject','2026-10-07T23:08:12.277499582Z'), ('nsgb','claims','2026-10-07T23:08:12.273689456Z'), ('nsgb','field-shadows-argument','2026-10-07T23:08:14.433775062Z'), ('nsgb','projects-skill','2026-10-07T23:08:13.778482852Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('hsqv7d',1), ('hsqv7d',2), ('hsqv7d',3), ('hsqv7d',4), ('hsqv7d',5), ('hsqv7d',6), ('hsqv7d',7), ('hsqv7d',8), ('hsqv7d',9), ('hsqv7d',10), ('hsqv7d',11), ('hsqv7d',12), ('hsqv7d',13), ('hsqv7d',14), ('hsqv7d',15), ('hsqv7d',16), ('hsqv7d',17), ('hsqv7d',18), ('hsqv7d',19), ('hsqv7d',20), ('hsqv7d',21), ('hsqv7d',22), ('hsqv7d',23), ('hsqv7d',24), ('hsqv7d',25), ('hsqv7d',26), ('hsqv7d',27), ('hsqv7d',28), ('hsqv7d',29), ('th5r9w',1);
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
