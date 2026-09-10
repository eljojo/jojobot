-- What was genuinely waiting in the sender's own box when they sent this
-- message. NULL for every row written before this column existed, and for
-- one jojobot could not determine at send time — neither reads as zero.
ALTER TABLE message ADD COLUMN sender_mail_waiting_at_send BIGINT NULL;
