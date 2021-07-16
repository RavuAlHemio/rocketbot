CREATE TABLE logger.channel
( channel_id character varying(256) NOT NULL PRIMARY KEY
, channel_name character varying(256) NOT NULL
);

CREATE TABLE logger.message
( message_id character varying(256) NOT NULL PRIMARY KEY
, channel_id character varying(256) NOT NULL REFERENCES logger.channel (channel_id)
, "timestamp" timestamp with time zone NOT NULL
, sender_username character varying(256) NOT NULL
, sender_nickname character varying(256) NULL
);

CREATE SEQUENCE logger.seq__message_revision__revision_id AS bigint;

CREATE TABLE logger.message_revision
( revision_id bigint NOT NULL PRIMARY KEY DEFAULT nextval('logger.seq__message_revision__revision_id')
, message_id character varying(256) NOT NULL REFERENCES logger.message (message_id)
, "timestamp" timestamp with time zone NOT NULL
, author_username character varying(256) NOT NULL
, body text NOT NULL
);
