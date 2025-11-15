-- MySQL dump 10.13  Distrib 8.4.5, for macos15.2 (arm64)
--
-- Host: 127.0.0.1    Database: violet
-- ------------------------------------------------------
-- Server version	8.0.30

/*!40101 SET @OLD_CHARACTER_SET_CLIENT=@@CHARACTER_SET_CLIENT */;
/*!40101 SET @OLD_CHARACTER_SET_RESULTS=@@CHARACTER_SET_RESULTS */;
/*!40101 SET @OLD_COLLATION_CONNECTION=@@COLLATION_CONNECTION */;
/*!50503 SET NAMES utf8mb4 */;
/*!40103 SET @OLD_TIME_ZONE=@@TIME_ZONE */;
/*!40103 SET TIME_ZONE='+00:00' */;
/*!40014 SET @OLD_UNIQUE_CHECKS=@@UNIQUE_CHECKS, UNIQUE_CHECKS=0 */;
/*!40014 SET @OLD_FOREIGN_KEY_CHECKS=@@FOREIGN_KEY_CHECKS, FOREIGN_KEY_CHECKS=0 */;
/*!40101 SET @OLD_SQL_MODE=@@SQL_MODE, SQL_MODE='NO_AUTO_VALUE_ON_ZERO' */;
/*!40111 SET @OLD_SQL_NOTES=@@SQL_NOTES, SQL_NOTES=0 */;

--
-- Table structure for table `id_meta_info`
--

DROP TABLE IF EXISTS `id_meta_info`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `id_meta_info` (
  `id` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT 'ID标识',
  `max_id` bigint DEFAULT NULL COMMENT '最大ID',
  `step` int DEFAULT NULL COMMENT '步长',
  `update_time` bigint NOT NULL COMMENT '更新时间',
  `version` int DEFAULT NULL COMMENT '版本号',
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_chat`
--

DROP TABLE IF EXISTS `im_chat`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_chat` (
  `chat_id` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '聊天ID',
  `chat_type` int NOT NULL COMMENT '聊天类型：0单聊，1群聊，2机器人，3公众号',
  `owner_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL,
  `to_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '对方用户ID或群组ID',
  `is_mute` smallint NOT NULL COMMENT '是否免打扰（1免打扰）',
  `is_top` smallint NOT NULL COMMENT '是否置顶（1置顶）',
  `sequence` bigint DEFAULT NULL COMMENT '消息序列号',
  `read_sequence` bigint DEFAULT NULL COMMENT '已读消息序列',
  `remark` varchar(200) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '群聊备注，仅自己可见',
  `create_time` bigint DEFAULT NULL COMMENT '创建时间',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间',
  `del_flag` smallint DEFAULT NULL COMMENT '删除标识（1正常，0删除）',
  `version` bigint DEFAULT NULL COMMENT '版本信息',
  PRIMARY KEY (`chat_id`,`owner_id`),
  KEY `idx_chat_owner_to` (`owner_id`,`to_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_friendship`
--

DROP TABLE IF EXISTS `im_friendship`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_friendship` (
  `owner_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '用户ID',
  `to_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '好友用户ID',
  `remark` varchar(50) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '备注',
  `del_flag` int DEFAULT NULL COMMENT '删除标识（1正常，0删除）',
  `black` int DEFAULT NULL COMMENT '黑名单状态（1正常，2拉黑）',
  `create_time` bigint DEFAULT NULL COMMENT '创建时间',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间',
  `sequence` bigint DEFAULT NULL COMMENT '序列号',
  `black_sequence` bigint DEFAULT NULL COMMENT '黑名单序列号',
  `add_source` varchar(20) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '好友来源',
  `extra` varchar(1000) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '扩展字段',
  `version` bigint DEFAULT NULL COMMENT '版本信息',
  PRIMARY KEY (`owner_id`,`to_id`),
  KEY `idx_owner_id` (`owner_id`),
  KEY `idx_to_id` (`to_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_friendship_request`
--

DROP TABLE IF EXISTS `im_friendship_request`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_friendship_request` (
  `id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '请求ID',
  `from_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '请求发起者',
  `to_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '请求接收者',
  `remark` varchar(50) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '备注',
  `read_status` int DEFAULT NULL COMMENT '是否已读（1已读）',
  `add_source` varchar(20) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '好友来源',
  `message` varchar(50) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '好友验证信息',
  `approve_status` int DEFAULT NULL COMMENT '审批状态（1同意，2拒绝）',
  `create_time` bigint DEFAULT NULL COMMENT '创建时间',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间',
  `sequence` bigint DEFAULT NULL COMMENT '序列号',
  `del_flag` smallint DEFAULT NULL COMMENT '删除标识（1正常，0删除）',
  `version` bigint DEFAULT NULL COMMENT '版本信息',
  PRIMARY KEY (`id`),
  KEY `idx_from_id` (`from_id`),
  KEY `idx_to_id` (`to_id`),
  KEY `idx_to_id_status` (`to_id`,`approve_status`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_group`
--

DROP TABLE IF EXISTS `im_group`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_group` (
  `group_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '群组ID',
  `owner_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '群主用户ID',
  `group_type` int NOT NULL COMMENT '群类型（1私有群，2公开群）',
  `group_name` varchar(100) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '群名称',
  `mute` smallint DEFAULT NULL COMMENT '是否全员禁言（1不禁言，0禁言）',
  `apply_join_type` int NOT NULL COMMENT '申请加群方式（0禁止申请，1需要审批，2允许自由加入）',
  `avatar` varchar(300) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '群头像',
  `max_member_count` int DEFAULT NULL COMMENT '最大成员数',
  `introduction` varchar(100) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '群简介',
  `notification` varchar(1000) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '群公告',
  `status` int DEFAULT NULL COMMENT '群状态（1正常，0解散）',
  `sequence` bigint DEFAULT NULL COMMENT '消息序列号',
  `create_time` bigint DEFAULT NULL COMMENT '创建时间',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间',
  `extra` varchar(1000) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '扩展字段',
  `version` bigint DEFAULT NULL COMMENT '版本信息',
  `del_flag` smallint NOT NULL COMMENT '删除标识（1正常，0删除）',
  `verifier` smallint DEFAULT NULL COMMENT '开启群验证（1验证，0不验证）',
  PRIMARY KEY (`group_id`),
  KEY `idx_owner_id` (`owner_id`),
  KEY `idx_status` (`status`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_group_member`
--

DROP TABLE IF EXISTS `im_group_member`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_group_member` (
  `group_member_id` varchar(100) COLLATE utf8mb4_unicode_ci NOT NULL,
  `group_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '群组ID',
  `member_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '成员用户ID',
  `role` int NOT NULL COMMENT '群成员角色（0普通成员，1管理员，2群主）',
  `speak_date` bigint DEFAULT NULL COMMENT '最后发言时间',
  `mute` smallint NOT NULL COMMENT '是否禁言（1不禁言，0禁言）',
  `alias` varchar(100) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '群昵称',
  `join_time` bigint DEFAULT NULL COMMENT '加入时间',
  `leave_time` bigint DEFAULT NULL COMMENT '离开时间',
  `join_type` varchar(50) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '加入类型',
  `extra` varchar(1000) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '扩展字段',
  `del_flag` smallint NOT NULL COMMENT '删除标识（1正常，0删除）',
  `create_time` bigint DEFAULT NULL COMMENT '创建时间',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间',
  `version` bigint DEFAULT NULL COMMENT '版本信息',
  PRIMARY KEY (`group_member_id`),
  KEY `idx_group_id` (`group_id`),
  KEY `idx_igm_member_group` (`member_id`,`group_id`),
  KEY `idx_member_id` (`member_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_group_message`
--

DROP TABLE IF EXISTS `im_group_message`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_group_message` (
  `message_id` varchar(512) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '消息ID',
  `group_id` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '群组ID',
  `from_id` varchar(20) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '发送者用户ID',
  `message_body` text COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '消息内容',
  `message_time` bigint NOT NULL COMMENT '发送时间',
  `message_content_type` int NOT NULL COMMENT '消息类型',
  `extra` text COLLATE utf8mb4_unicode_ci COMMENT '扩展字段',
  `del_flag` smallint NOT NULL COMMENT '删除标识（1正常，0删除）',
  `sequence` bigint DEFAULT NULL COMMENT '消息序列',
  `message_random` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '随机标识',
  `create_time` bigint NOT NULL COMMENT '创建时间',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间',
  `version` bigint DEFAULT NULL COMMENT '版本信息',
  `reply_to` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '被引用的消息 ID',
  PRIMARY KEY (`message_id`),
  KEY `idx_group_msg_group` (`group_id`),
  KEY `idx_from_id` (`from_id`),
  KEY `idx_sequence` (`sequence`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_outbox`
--

DROP TABLE IF EXISTS `im_outbox`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_outbox` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT COMMENT '主键',
  `message_id` varchar(64) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '业务消息 ID（用于回溯/去重/关联业务数据）',
  `payload` text COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '要发送的 JSON 负载（建议尽量轻量：可仅包含 messageId + 必要路由信息）',
  `exchange` varchar(128) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '目标交换机名称',
  `routing_key` varchar(128) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '目标路由键（或 queue 名称）',
  `attempts` int NOT NULL DEFAULT '0' COMMENT '累积投递次数',
  `status` varchar(20) COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT 'PENDING' COMMENT '投递状态：PENDING(待投递) / SENT(已确认) / FAILED(失败，需要人工介入) / DLX(死信)',
  `last_error` text COLLATE utf8mb4_unicode_ci COMMENT '投递失败时的错误信息',
  `created_at` bigint DEFAULT NULL COMMENT '创建时间',
  `updated_at` bigint DEFAULT NULL COMMENT '更新时间',
  `next_try_at` bigint DEFAULT NULL COMMENT '下一次重试时间（用以调度延迟重试）',
  PRIMARY KEY (`id`),
  KEY `idx_outbox_message_id` (`message_id`),
  KEY `idx_outbox_status` (`status`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci COMMENT='Outbox table: 持久化要投递到 MQ 的消息，支持重试/幂等/确认回写';
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_single_message`
--

DROP TABLE IF EXISTS `im_single_message`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_single_message` (
  `message_id` varchar(512) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '消息ID',
  `from_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '发送者用户ID',
  `to_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '接收者用户ID',
  `message_body` text COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '消息内容',
  `message_time` bigint NOT NULL COMMENT '发送时间',
  `message_content_type` int NOT NULL COMMENT '消息类型',
  `read_status` int NOT NULL COMMENT '阅读状态（1已读）',
  `extra` text COLLATE utf8mb4_unicode_ci COMMENT '扩展字段',
  `del_flag` smallint NOT NULL COMMENT '删除标识（1正常，0删除）',
  `sequence` bigint NOT NULL COMMENT '消息序列',
  `message_random` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '随机标识',
  `create_time` bigint DEFAULT NULL COMMENT '创建时间',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间',
  `version` bigint DEFAULT NULL COMMENT '版本信息',
  `reply_to` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '被引用的消息 ID',
  `to_type` enum('User','Group') COLLATE utf8mb4_unicode_ci DEFAULT 'User' COMMENT '接收者类型：User=用户，Group=群组',
  `file_url` varchar(512) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '文件URL',
  `file_name` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '文件名',
  `file_type` varchar(64) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '文件类型',
  PRIMARY KEY (`message_id`),
  KEY `idx_private_from` (`from_id`),
  KEY `idx_private_to` (`to_id`),
  KEY `idx_sequence` (`sequence`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `im_user_data`
--

DROP TABLE IF EXISTS `im_user_data`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `im_user_data` (
  `user_id` varchar(50) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '用户ID',
  `name` varchar(100) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '昵称',
  `avatar` varchar(1024) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '头像',
  `gender` int DEFAULT NULL COMMENT '性别',
  `birthday` varchar(50) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '生日',
  `location` varchar(50) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '地址',
  `self_signature` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '个性签名',
  `friend_allow_type` int NOT NULL COMMENT '加好友验证类型（1无需验证，2需要验证）',
  `forbidden_flag` int NOT NULL COMMENT '禁用标识（1禁用）',
  `disable_add_friend` int NOT NULL COMMENT '管理员禁止添加好友：0未禁用，1已禁用',
  `silent_flag` int NOT NULL COMMENT '禁言标识（1禁言）',
  `user_type` int NOT NULL COMMENT '用户类型（1普通用户，2客服，3机器人）',
  `del_flag` smallint NOT NULL COMMENT '删除标识（1正常，0删除）',
  `extra` varchar(1000) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '扩展字段',
  `create_time` bigint DEFAULT NULL COMMENT '创建时间',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间',
  `version` bigint DEFAULT NULL COMMENT '版本信息',
  PRIMARY KEY (`user_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `subscriptions`
--

DROP TABLE IF EXISTS `subscriptions`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `subscriptions` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `subscription_id` varchar(64) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT '订阅ID，格式：sub_{uuid}',
  `user_id` bigint unsigned NOT NULL COMMENT '用户ID',
  `device_info` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '设备信息（可选）',
  `created_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `updated_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
  `expires_at` timestamp NULL DEFAULT NULL COMMENT '过期时间（可选，用于自动清理）',
  PRIMARY KEY (`id`),
  UNIQUE KEY `subscription_id` (`subscription_id`),
  KEY `idx_subscription_id` (`subscription_id`),
  KEY `idx_user_id` (`user_id`),
  KEY `idx_expires_at` (`expires_at`),
  CONSTRAINT `subscriptions_ibfk_1` FOREIGN KEY (`user_id`) REFERENCES `users` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=7 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `users`
--

DROP TABLE IF EXISTS `users`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!50503 SET character_set_client = utf8mb4 */;
CREATE TABLE `users` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `open_id` varchar(32) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '外部唯一标识符（雪花算法生成的数字字符串，最多20字符）',
  `name` varchar(100) COLLATE utf8mb4_unicode_ci NOT NULL,
  `email` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL,
  `file_name` varchar(256) COLLATE utf8mb4_unicode_ci DEFAULT 'eb3dad2d-4b7f-44c2-9af5-50ad9f76ff81.png' COMMENT '头像文件名',
  `abstract` varchar(128) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '个性签名',
  `phone` varchar(11) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT '手机号',
  `status` tinyint DEFAULT '1' COMMENT '状态：1正常 2禁用 3删除',
  `gender` tinyint DEFAULT '3' COMMENT '性别：1男 2女 3未知',
  `password_hash` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL,
  `created_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  `version` bigint DEFAULT '1' COMMENT '版本号',
  `del_flag` tinyint DEFAULT '1' COMMENT '删除标志：1=正常，0=删除',
  `create_time` bigint DEFAULT NULL COMMENT '创建时间戳（毫秒）',
  `update_time` bigint DEFAULT NULL COMMENT '更新时间戳（毫秒）',
  PRIMARY KEY (`id`),
  UNIQUE KEY `email` (`email`),
  UNIQUE KEY `idx_name_unique` (`name`),
  UNIQUE KEY `idx_users_name_unique` (`name`),
  UNIQUE KEY `open_id` (`open_id`),
  UNIQUE KEY `open_id_2` (`open_id`),
  UNIQUE KEY `idx_users_phone_unique` (`phone`),
  KEY `idx_email` (`email`),
  KEY `idx_open_id` (`open_id`),
  KEY `idx_phone` (`phone`),
  KEY `idx_status` (`status`)
) ENGINE=InnoDB AUTO_INCREMENT=8 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
/*!40101 SET character_set_client = @saved_cs_client */;
/*!50003 SET @saved_cs_client      = @@character_set_client */ ;
/*!50003 SET @saved_cs_results     = @@character_set_results */ ;
/*!50003 SET @saved_col_connection = @@collation_connection */ ;
/*!50003 SET character_set_client  = utf8mb4 */ ;
/*!50003 SET character_set_results = utf8mb4 */ ;
/*!50003 SET collation_connection  = utf8mb4_0900_ai_ci */ ;
/*!50003 SET @saved_sql_mode       = @@sql_mode */ ;
/*!50003 SET sql_mode              = 'ONLY_FULL_GROUP_BY,STRICT_TRANS_TABLES,NO_ZERO_IN_DATE,NO_ZERO_DATE,ERROR_FOR_DIVISION_BY_ZERO,NO_ENGINE_SUBSTITUTION' */ ;
DELIMITER ;;
/*!50003 CREATE*/ /*!50017 DEFINER=`root`@`%`*/ /*!50003 TRIGGER `users_before_insert` BEFORE INSERT ON `users` FOR EACH ROW BEGIN
    IF NEW.create_time IS NULL THEN
        SET NEW.create_time = UNIX_TIMESTAMP(NOW()) * 1000;
    END IF;
    IF NEW.update_time IS NULL THEN
        SET NEW.update_time = UNIX_TIMESTAMP(NOW()) * 1000;
    END IF;
    IF NEW.del_flag IS NULL THEN
        SET NEW.del_flag = CASE WHEN NEW.status = 1 THEN 1 ELSE 0 END;
    END IF;
    IF NEW.version IS NULL THEN
        SET NEW.version = 1;
    END IF;
END */;;
DELIMITER ;
/*!50003 SET sql_mode              = @saved_sql_mode */ ;
/*!50003 SET character_set_client  = @saved_cs_client */ ;
/*!50003 SET character_set_results = @saved_cs_results */ ;
/*!50003 SET collation_connection  = @saved_col_connection */ ;
/*!50003 SET @saved_cs_client      = @@character_set_client */ ;
/*!50003 SET @saved_cs_results     = @@character_set_results */ ;
/*!50003 SET @saved_col_connection = @@collation_connection */ ;
/*!50003 SET character_set_client  = utf8mb4 */ ;
/*!50003 SET character_set_results = utf8mb4 */ ;
/*!50003 SET collation_connection  = utf8mb4_0900_ai_ci */ ;
/*!50003 SET @saved_sql_mode       = @@sql_mode */ ;
/*!50003 SET sql_mode              = 'ONLY_FULL_GROUP_BY,STRICT_TRANS_TABLES,NO_ZERO_IN_DATE,NO_ZERO_DATE,ERROR_FOR_DIVISION_BY_ZERO,NO_ENGINE_SUBSTITUTION' */ ;
DELIMITER ;;
/*!50003 CREATE*/ /*!50017 DEFINER=`root`@`%`*/ /*!50003 TRIGGER `users_before_update` BEFORE UPDATE ON `users` FOR EACH ROW BEGIN
    IF NEW.update_time IS NOT NULL THEN
        SET NEW.update_time = UNIX_TIMESTAMP(NOW()) * 1000;
    END IF;
    IF NEW.version IS NOT NULL AND OLD.version IS NOT NULL AND NEW.version = OLD.version THEN
        SET NEW.version = OLD.version + 1;
    END IF;
END */;;
DELIMITER ;
/*!50003 SET sql_mode              = @saved_sql_mode */ ;
/*!50003 SET character_set_client  = @saved_cs_client */ ;
/*!50003 SET character_set_results = @saved_cs_results */ ;
/*!50003 SET collation_connection  = @saved_col_connection */ ;
/*!40103 SET TIME_ZONE=@OLD_TIME_ZONE */;

/*!40101 SET SQL_MODE=@OLD_SQL_MODE */;
/*!40014 SET FOREIGN_KEY_CHECKS=@OLD_FOREIGN_KEY_CHECKS */;
/*!40014 SET UNIQUE_CHECKS=@OLD_UNIQUE_CHECKS */;
/*!40101 SET CHARACTER_SET_CLIENT=@OLD_CHARACTER_SET_CLIENT */;
/*!40101 SET CHARACTER_SET_RESULTS=@OLD_CHARACTER_SET_RESULTS */;
/*!40101 SET COLLATION_CONNECTION=@OLD_COLLATION_CONNECTION */;
/*!40111 SET SQL_NOTES=@OLD_SQL_NOTES */;

-- Dump completed on 2025-11-16  0:55:37
