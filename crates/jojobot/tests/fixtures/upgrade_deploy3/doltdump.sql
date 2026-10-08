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
INSERT INTO `entity` (`id`,`kind`,`name`,`source`,`crm`,`parent`,`boot`,`prose`,`badge`,`merged_into`,`archived_reason`,`archived_at`) VALUES ('bot:assistant','bot','Assistant','jojobot',NULL,NULL,'on-demand','','4qjqen',NULL,NULL,NULL), ('bot:upgrade-fixture-lead','bot','A Recorded Lead','test',NULL,NULL,'on-demand','','naxbje',NULL,NULL,NULL), ('person:upgrade-fixture-person','person','A Recorded Person','test',NULL,NULL,'on-demand','','cmf71a',NULL,NULL,NULL), ('place:upgrade-fixture-place','place','A Recorded Place','test',NULL,NULL,'on-demand','','r1bryq',NULL,NULL,NULL), ('project:upgrade-fixture-project','project','A Recorded Project','test',NULL,NULL,'on-demand','','37w6ea',NULL,NULL,NULL), ('thing:blue-kite','thing','A Recorded Spare Kite','test',NULL,NULL,'on-demand','','pbktys',NULL,NULL,NULL), ('thing:recorded-twin','thing','A Spare Copy','test',NULL,NULL,'on-demand','','psasb5','thing:upgrade-fixture-thing',NULL,NULL), ('thing:red-kite','thing','A Recorded Kite','test',NULL,NULL,'on-demand','','j7266f',NULL,NULL,NULL), ('thing:upgrade-fixture-thing','thing','A Recorded Thing','test',NULL,NULL,'on-demand','','2337tq',NULL,NULL,NULL), ('topic:instance','topic','This instance','test',NULL,NULL,'on-demand','','1ezrbg',NULL,NULL,NULL), ('work:upgrade-fixture-prior','work','A Recorded Prior Task','test',NULL,'37w6ea','on-demand','','3px63q',NULL,NULL,NULL), ('work:upgrade-fixture-task','work','A Recorded Task','test',NULL,'37w6ea','on-demand','','5xjrgy',NULL,NULL,NULL);
DROP TABLE IF EXISTS `entity_alias`;
CREATE TABLE `entity_alias` (
  `entity` varchar(191) NOT NULL,
  `ordinal` int NOT NULL,
  `alias` text NOT NULL,
  PRIMARY KEY (`entity`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `entity_alias` (`entity`,`ordinal`,`alias`) VALUES ('2337tq',1,'A Spare Copy');
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
INSERT INTO `entity_write` (`entity`,`ordinal`,`written_at`) VALUES ('bot:assistant',1,'2026-10-08T05:21:37.189157046Z'), ('bot:upgrade-fixture-lead',1,'2026-10-08T05:21:41.076833052Z'), ('person:upgrade-fixture-person',1,'2026-10-08T05:21:37.61629043Z'), ('place:upgrade-fixture-place',1,'2026-10-08T05:21:37.685154071Z'), ('project:upgrade-fixture-project',1,'2026-10-08T05:21:39.267725664Z'), ('thing:blue-kite',1,'2026-10-08T05:21:38.914058668Z'), ('thing:recorded-twin',1,'2026-10-08T05:21:38.220501823Z'), ('thing:recorded-twin',2,'2026-10-08T05:21:38.553295013Z'), ('thing:red-kite',1,'2026-10-08T05:21:38.699630805Z'), ('thing:upgrade-fixture-thing',1,'2026-10-08T05:21:37.747937485Z'), ('thing:upgrade-fixture-thing',2,'2026-10-08T05:21:38.546087783Z'), ('topic:instance',1,'2026-10-08T05:21:41.935245546Z'), ('work:upgrade-fixture-prior',1,'2026-10-08T05:21:39.332007984Z'), ('work:upgrade-fixture-task',1,'2026-10-08T05:21:39.398634206Z');
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
INSERT INTO `fact` (`entity`,`id`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`event_kind`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`happened_at`,`happened_through`) VALUES ('1ezrbg','f1','the instance works in one zone',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:42.120898729Z',NULL,NULL,NULL), ('2337tq','f1','the twin carried a note',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:38.401115521Z',NULL,NULL,NULL), ('2337tq','f2','recorded as one thing',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:38.547215683Z',NULL,NULL,NULL), ('2337tq','f3','a day kept under decide_by',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:40.017313574Z',NULL,NULL,NULL), ('37w6ea','f1','the recorded project is under way',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:39.467467136Z',NULL,NULL,NULL), ('3px63q','f1','the prior task is finished',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:39.675307329Z',NULL,NULL,NULL), ('4qjqen','f1','claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:37.509847484Z',NULL,NULL,NULL), ('4qjqen','f2','a recorded thought, live in the room',NULL,'inference','open','active','2026-10-08','connection','2337tq',NULL,NULL,NULL,'2026-10-08T05:21:40.151674385Z',NULL,NULL,NULL), ('4qjqen','f3','the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:40.43382223Z',NULL,NULL,NULL), ('4qjqen','f4','the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:41.507203149Z',NULL,NULL,NULL), ('4qjqen','f5','the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:41.661205882Z',NULL,NULL,NULL), ('4qjqen','f6','claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:42.916360413Z',NULL,NULL,NULL), ('5xjrgy','f1','the recorded task is in review',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:39.793643811Z',NULL,NULL,NULL), ('cmf71a','f1','lives at the recorded place',NULL,'testimony','settled','active','2026-10-08','location','r1bryq',NULL,NULL,NULL,'2026-10-08T05:21:37.814415463Z',NULL,NULL,NULL), ('cmf71a','f2','visited the recorded place',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,'cmf71a','f1','2026-10-08T05:21:37.984408014Z',NULL,NULL,NULL), ('j7266f','f1','where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:38.787921706Z',NULL,NULL,NULL), ('pbktys','f1','where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:38.999907429Z',NULL,NULL,NULL), ('r1bryq','f1','the recorded place opens at nine',NULL,'observation','settled','active','2026-10-08',NULL,NULL,NULL,NULL,NULL,'2026-10-08T05:21:40.599570233Z',NULL,NULL,NULL);
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
INSERT INTO `fact_write` (`entity`,`fact_id`,`ordinal`,`content`,`details`,`provenance`,`standing`,`status`,`recorded_at`,`edge_shape`,`edge_object`,`derived_from`,`derived_from_id`,`inserted_at`,`stale_after`,`written_at`,`happened_at`,`happened_through`,`session`) VALUES ('1ezrbg','f1',1,'the instance works in one zone',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:42.120898729Z',NULL,'2026-10-08T05:21:42.124949399Z',NULL,NULL,'82z6'), ('2337tq','f1',1,'the twin carried a note',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:38.401115521Z',NULL,'2026-10-08T05:21:38.406233702Z',NULL,NULL,'82z6'), ('2337tq','f2',1,'recorded as one thing',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:38.547215683Z',NULL,'2026-10-08T05:21:38.549485693Z',NULL,NULL,NULL), ('2337tq','f3',1,'a day kept under decide_by',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:40.017313574Z',NULL,'2026-10-08T05:21:40.021694494Z',NULL,NULL,'82z6'), ('37w6ea','f1',1,'the recorded project is under way',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:39.467467136Z',NULL,'2026-10-08T05:21:39.471308527Z',NULL,NULL,'82z6'), ('3px63q','f1',1,'the prior task is finished',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:39.675307329Z',NULL,'2026-10-08T05:21:39.681680569Z',NULL,NULL,'82z6'), ('4qjqen','f1',1,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:37.509847484Z',NULL,'2026-10-08T05:21:37.515652771Z',NULL,NULL,NULL), ('4qjqen','f1',2,'claimed the upgrade-fixture-recorder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:37.509847484Z',NULL,'2026-10-08T05:21:42.71886764Z',NULL,NULL,NULL), ('4qjqen','f2',1,'a recorded thought, live in the room',NULL,'inference','open','active','2026-10-08','connection','2337tq',NULL,NULL,'2026-10-08T05:21:40.151674385Z',NULL,'2026-10-08T05:21:40.155796266Z',NULL,NULL,'82z6'), ('4qjqen','f3',1,'the recorder keeps its recording rule',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:40.43382223Z',NULL,'2026-10-08T05:21:40.43865489Z',NULL,NULL,'82z6'), ('4qjqen','f4',1,'the assistant reports to the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:41.507203149Z',NULL,'2026-10-08T05:21:41.511679288Z',NULL,NULL,'ymgk'), ('4qjqen','f5',1,'the assistant pairs with the recorded lead',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:41.661205882Z',NULL,'2026-10-08T05:21:41.665340231Z',NULL,NULL,'82z6'), ('4qjqen','f6',1,'claimed the upgrade-fixture-holder role',NULL,'inference','open','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:42.916360413Z',NULL,'2026-10-08T05:21:42.920829484Z',NULL,NULL,NULL), ('5xjrgy','f1',1,'the recorded task is in review',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:39.793643811Z',NULL,'2026-10-08T05:21:39.800036911Z',NULL,NULL,'82z6'), ('cmf71a','f1',1,'lives at the recorded place',NULL,'testimony','settled','active','2026-10-08','location','r1bryq',NULL,NULL,'2026-10-08T05:21:37.814415463Z',NULL,'2026-10-08T05:21:37.818631819Z',NULL,NULL,'82z6'), ('cmf71a','f2',1,'visited the recorded place',NULL,'inference','open','active','2026-10-08',NULL,NULL,'cmf71a','f1','2026-10-08T05:21:37.984408014Z',NULL,'2026-10-08T05:21:37.991050572Z',NULL,NULL,'82z6'), ('j7266f','f1',1,'where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:38.787921706Z',NULL,'2026-10-08T05:21:38.794295666Z',NULL,NULL,'82z6'), ('pbktys','f1',1,'where it stands',NULL,'testimony','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:38.999907429Z',NULL,'2026-10-08T05:21:39.00505413Z',NULL,NULL,'82z6'), ('r1bryq','f1',1,'the recorded place opens at nine',NULL,'observation','settled','active','2026-10-08',NULL,NULL,NULL,NULL,'2026-10-08T05:21:40.599570233Z',NULL,'2026-10-08T05:21:40.604164564Z',NULL,NULL,'82z6');
DROP TABLE IF EXISTS `field_link`;
CREATE TABLE `field_link` (
  `entity` varchar(191) NOT NULL,
  `key` varchar(191) NOT NULL,
  `ordinal` bigint NOT NULL,
  `target` varchar(191) NOT NULL,
  PRIMARY KEY (`entity`,`key`,`ordinal`,`target`),
  KEY `by_target` (`target`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `field_link` (`entity`,`key`,`ordinal`,`target`) VALUES ('4qjqen','pairs_with',1,'naxbje'), ('4qjqen','reports_to',1,'naxbje'), ('5xjrgy','depends_on',1,'3px63q'), ('5xjrgy','owner',1,'cmf71a'), ('cmf71a','venue',1,'r1bryq');
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
INSERT INTO `field_write` (`entity`,`key`,`ordinal`,`value`,`fact_id`) VALUES ('1ezrbg','timezone',1,'America/New_York','f1'), ('2337tq','decide_by',1,'2026-12-01','f3'), ('2337tq','due_on',1,'2026-12-01','f3'), ('2337tq','merged_from',1,'thing:recorded-twin','f2'), ('37w6ea','columns',1,'someday, next, now, waiting, done, review','f1'), ('37w6ea','status',1,'now','f1'), ('3px63q','status',1,'done','f1'), ('4qjqen','pairs_with',1,'@#naxbje','f5'), ('4qjqen','reports_to',1,'naxbje','f4'), ('4qjqen','role/upgrade-fixture-holder/claimed_at',1,'2026-10-08T05:21:42.888602363Z','f6'), ('4qjqen','role/upgrade-fixture-holder/holder',1,'h66y','f6'), ('4qjqen','role/upgrade-fixture-recorder/claimed_at',1,'2026-10-08T05:21:37.500898744Z','f1'), ('4qjqen','role/upgrade-fixture-recorder/claimed_at',2,'1970-01-01T00:00:00Z','f1'), ('4qjqen','role/upgrade-fixture-recorder/holder',1,'82z6','f1'), ('4qjqen','role/upgrade-fixture-recorder/holder',2,NULL,'f1'), ('4qjqen','starred',1,'true','f3'), ('5xjrgy','depends_on',1,'3px63q','f1'), ('5xjrgy','owner',1,'cmf71a','f1'), ('5xjrgy','status',1,'review','f1'), ('cmf71a','since',1,'2026-01-01','f1'), ('cmf71a','venue',1,'r1bryq','f2'), ('j7266f','status',1,'now','f1'), ('pbktys','status',1,'done','f1'), ('r1bryq','read_from',1,'the recorded place\'s page','f1'), ('r1bryq','read_ref',1,'opening hours','f1');
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
INSERT INTO `journal_entry` (`session`,`id`,`ordinal`,`at`,`text`,`touched`,`beat`,`happened_on`,`closing_focus`) VALUES ('3rgfg8','3nkc8k',1,'2026-10-08T05:21:37.659844552Z','brought entities into being: person:upgrade-fixture-person, place:upgrade-fixture-place, thing:upgrade-fixture-thing, thing:recorded-twin, thing:red-kite, … (11)','2026-10-08T05:21:42.087260139Z','add_entity',NULL,NULL), ('3rgfg8','cxtwqv',4,'2026-10-08T05:21:42.381681894Z','posted to mailboxes: assistant, … (3)','2026-10-08T05:21:42.514515066Z','post_message',NULL,NULL), ('3rgfg8','g2nye0',5,'2026-10-08T05:21:42.627178559Z','retired messages: qqby6p (1)',NULL,'mark_processed',NULL,NULL), ('3rgfg8','gmcxdv',2,'2026-10-08T05:21:37.877344089Z','captured facts about: person:upgrade-fixture-person, thing:recorded-twin, thing:red-kite, thing:blue-kite, project:upgrade-fixture-project, … (14)','2026-10-08T05:21:42.282907662Z','capture',NULL,NULL), ('3rgfg8','jqrvqb',6,'2026-10-08T05:21:42.643527479Z','recorded the upgrade fixture',NULL,NULL,NULL,NULL), ('3rgfg8','xr0gr8',3,'2026-10-08T05:21:38.665557265Z','merged entities: thing:recorded-twin (1)',NULL,'merge_entities',NULL,NULL), ('45tvp3','fc1mpx',1,'2026-10-08T05:21:41.60661376Z','captured facts about: bot:assistant (1)',NULL,'capture',NULL,NULL);
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
INSERT INTO `message` (`id`,`mailbox`,`ordinal`,`body`,`subject`,`sender`,`sent_at`,`state`,`notes`,`in_reply_to`,`sender_mail_waiting_at_send`,`posted_by_session`,`quarantined_by`,`quarantined_at`,`quarantine_reason`) VALUES ('5zfy3n','assistant',1,'left new',NULL,'4qjqen','2026-10-08T05:21:42.344490544Z','new',NULL,NULL,0,'3rgfg8',NULL,NULL,NULL), ('ncdehd','assistant',2,'will be read',NULL,'4qjqen','2026-10-08T05:21:42.414564375Z','read',NULL,NULL,1,'3rgfg8',NULL,NULL,NULL), ('qqby6p','assistant',3,'will be processed',NULL,'4qjqen','2026-10-08T05:21:42.480200386Z','processed',NULL,NULL,2,'3rgfg8',NULL,NULL,NULL);
DROP TABLE IF EXISTS `message_delivery`;
CREATE TABLE `message_delivery` (
  `message_id` varchar(64) NOT NULL,
  `taken_by` varchar(16) NOT NULL,
  PRIMARY KEY (`message_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `message_delivery` (`message_id`,`taken_by`) VALUES ('ncdehd','reading');
DROP TABLE IF EXISTS `schema_migration`;
CREATE TABLE `schema_migration` (
  `version` varchar(64) NOT NULL,
  `applied_at` varchar(48) NOT NULL,
  PRIMARY KEY (`version`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `schema_migration` (`version`,`applied_at`) VALUES ('0001_session','2026-10-08T05:21:36.393873477Z'), ('0002_journal_entry','2026-10-08T05:21:36.403537689Z'), ('0003_minted','2026-10-08T05:21:36.41238886Z'), ('0004_mailbox','2026-10-08T05:21:36.420696709Z'), ('0005_message','2026-10-08T05:21:36.429196689Z'), ('0006_handover','2026-10-08T05:21:36.436701408Z'), ('0007_drop_minted','2026-10-08T05:21:36.443677856Z'), ('0008_entity','2026-10-08T05:21:36.453098147Z'), ('0009_entity_alias','2026-10-08T05:21:36.460876237Z'), ('0010_fact','2026-10-08T05:21:36.472931701Z'), ('0011_fact_event_metadata','2026-10-08T05:21:36.482186352Z'), ('0012_fact_event_ref','2026-10-08T05:21:36.490852061Z'), ('0013_type_field','2026-10-08T05:21:36.499058231Z'), ('0014_message_delivery','2026-10-08T05:21:36.507401201Z'), ('0015_type_field_origin','2026-10-08T05:21:36.516415592Z'), ('0016_field_write','2026-10-08T05:21:36.529030977Z'), ('0017_type_field_folds','2026-10-08T05:21:36.539486719Z'), ('0018_type_field_holds_wider','2026-10-08T05:21:36.54857363Z'), ('0019_entity_source_wider','2026-10-08T05:21:36.557785391Z'), ('0020_entity_crm_wider','2026-10-08T05:21:36.566319611Z'), ('0021_kind','2026-10-08T05:21:36.574520381Z'), ('0022_type_field_owner','2026-10-08T05:21:36.58310425Z'), ('0023_type_field_required','2026-10-08T05:21:36.59074218Z'), ('0024_rhythm_kind_takes_its_name','2026-10-08T05:21:36.596712997Z'), ('0025_type_field_one_of','2026-10-08T05:21:36.604244266Z'), ('0026_session_timezone','2026-10-08T05:21:36.612238985Z'), ('0027_fact_inserted_at','2026-10-08T05:21:36.620510235Z'), ('0028_fact_stale_after','2026-10-08T05:21:36.628703604Z'), ('0029_fact_by_source','2026-10-08T05:21:36.637052645Z'), ('0030_session_stated_day','2026-10-08T05:21:36.648427208Z'), ('0031_journal_entry_day','2026-10-08T05:21:36.657973659Z'), ('0032_entity_badge','2026-10-08T05:21:36.66668712Z'), ('0033_entity_merged_into','2026-10-08T05:21:36.67533913Z'), ('0034_fact_write','2026-10-08T05:21:36.68419506Z'), ('0035_fact_write_backfill','2026-10-08T05:21:36.691092118Z'), ('0036_fact_write_moment','2026-10-08T05:21:36.700259299Z'), ('0037_fact_happened_at','2026-10-08T05:21:36.708499149Z'), ('0038_fact_write_happened_at','2026-10-08T05:21:36.71887077Z'), ('0039_fact_recorded_at','2026-10-08T05:21:36.728471693Z'), ('0040_fact_write_recorded_at','2026-10-08T05:21:36.740283196Z'), ('0041_session_teaching','2026-10-08T05:21:36.753287452Z'), ('0042_entity_former_handle','2026-10-08T05:21:36.762484323Z'), ('0043_message_sender_mail_waiting','2026-10-08T05:21:36.772971625Z'), ('0044_fact_stands_for','2026-10-08T05:21:36.784208228Z'), ('0045_entity_former_handle_key_add','2026-10-08T05:21:36.831665484Z'), ('0045_entity_former_handle_key_drop','2026-10-08T05:21:36.821504692Z'), ('0045_entity_former_handle_ordinal','2026-10-08T05:21:36.810991559Z'), ('0045_entity_former_handle_width','2026-10-08T05:21:36.800287007Z'), ('0046_fact_status_archived','2026-10-08T05:21:36.839686903Z'), ('0047_fact_write_status_archived','2026-10-08T05:21:36.846431231Z'), ('0048_session_served_chars','2026-10-08T05:21:36.854583331Z'), ('0049_entity_archived','2026-10-08T05:21:36.863102921Z'), ('0050_entity_archived_at','2026-10-08T05:21:36.871375181Z'), ('0051_entity_write','2026-10-08T05:21:36.87930968Z'), ('0052_fact_happened_through','2026-10-08T05:21:36.88757283Z'), ('0053_fact_write_happened_through','2026-10-08T05:21:36.896476951Z'), ('0054_session_write','2026-10-08T05:21:36.904939281Z'), ('0055_message_posted_by_session','2026-10-08T05:21:36.913574811Z'), ('0056_session_stated_day','2026-10-08T05:21:36.922080601Z'), ('0057_message_quarantined_by','2026-10-08T05:21:36.93037656Z'), ('0058_message_quarantined_at','2026-10-08T05:21:36.9389827Z'), ('0059_message_quarantine_reason','2026-10-08T05:21:36.948700532Z'), ('0060_journal_entry_closing_focus','2026-10-08T05:21:36.960970087Z'), ('0061_displaced_type_field','2026-10-08T05:21:36.971332809Z'), ('0062_message_by_sender','2026-10-08T05:21:36.979425389Z'), ('0063_field_link','2026-10-08T05:21:36.987606408Z'), ('0064_session_wrap_window','2026-10-08T05:21:36.997064519Z'), ('0065_fact_write_session','2026-10-08T05:21:37.006591791Z');
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
INSERT INTO `session` (`id`,`sid`,`bot`,`focus`,`started_at`,`state`,`timezone`,`started_on`,`served_chars`,`stated_day`,`wrap_window`) VALUES ('3rgfg8','82z6','4qjqen','claimed the upgrade-fixture-recorder role','2026-10-08T05:21:37.578278755Z','wrapped',NULL,NULL,25539,NULL,NULL), ('45tvp3','ymgk','naxbje','working','2026-10-08T05:21:41.58327546Z','active',NULL,NULL,2865,NULL,NULL), ('qkkx7p','h66y','4qjqen','claimed the upgrade-fixture-holder role','2026-10-08T05:21:42.998082275Z','active',NULL,NULL,0,NULL,NULL);
DROP TABLE IF EXISTS `session_teaching`;
CREATE TABLE `session_teaching` (
  `sid` varchar(4) NOT NULL,
  `domain` varchar(64) NOT NULL,
  `taught_at` varchar(48) NOT NULL,
  PRIMARY KEY (`sid`,`domain`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_teaching` (`sid`,`domain`,`taught_at`) VALUES ('82z6','claim-direction','2026-10-08T05:21:37.904083749Z'), ('82z6','claim-subject','2026-10-08T05:21:37.900297845Z'), ('82z6','claims','2026-10-08T05:21:37.89659652Z'), ('82z6','field-shadows-argument','2026-10-08T05:21:38.893758819Z'), ('82z6','projects-skill','2026-10-08T05:21:39.313566644Z'), ('82z6','session-id','2026-10-08T05:21:42.659835788Z'), ('ymgk','claim-subject','2026-10-08T05:21:41.638084771Z'), ('ymgk','claims','2026-10-08T05:21:41.634388351Z');
DROP TABLE IF EXISTS `session_write`;
CREATE TABLE `session_write` (
  `session` varchar(64) NOT NULL,
  `ordinal` bigint NOT NULL,
  PRIMARY KEY (`session`,`ordinal`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_bin;
INSERT INTO `session_write` (`session`,`ordinal`) VALUES ('3rgfg8',1), ('3rgfg8',2), ('3rgfg8',3), ('3rgfg8',4), ('3rgfg8',5), ('3rgfg8',6), ('3rgfg8',7), ('3rgfg8',8), ('3rgfg8',9), ('3rgfg8',10), ('3rgfg8',11), ('3rgfg8',12), ('3rgfg8',13), ('3rgfg8',14), ('3rgfg8',15), ('3rgfg8',16), ('3rgfg8',17), ('3rgfg8',18), ('3rgfg8',19), ('3rgfg8',20), ('3rgfg8',21), ('3rgfg8',22), ('3rgfg8',23), ('3rgfg8',24), ('3rgfg8',25), ('3rgfg8',26), ('3rgfg8',27), ('3rgfg8',28), ('3rgfg8',29), ('3rgfg8',30), ('3rgfg8',31), ('3rgfg8',32), ('45tvp3',1), ('45tvp3',2), ('qkkx7p',1);
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
