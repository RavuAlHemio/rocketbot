CREATE SEQUENCE quotes.quotes_message_id_seq AS bigint;

CREATE TABLE quotes.quotes
( quote_id bigint NOT NULL DEFAULT nextval('quotes.quotes_message_id_seq')
, "timestamp" timestamp with time zone NOT NULL
, channel character varying(255) NOT NULL
, author character varying(255) NOT NULL
, message_type character varying(1) NOT NULL
, body text NOT NULL
, CONSTRAINT quotes_pkey PRIMARY KEY (quote_id)
);

CREATE SEQUENCE quotes.quote_votes_vote_id_seq AS bigint;

CREATE TABLE quotes.quote_votes
( vote_id bigint NOT NULL DEFAULT nextval('quotes.quote_votes_vote_id_seq')
, quote_id bigint NOT NULL
, voter_lowercase character varying(255) NOT NULL,
, points smallint NOT NULL
, CONSTRAINT quote_votes_pkey PRIMARY KEY (vote_id)
, CONSTRAINT quote_votes_quote_id_voter_lowercase_key UNIQUE (quote_id, voter_lowercase)
, CONSTRAINT quote_votes_quote_id_fkey FOREIGN KEY (quote_id) REFERENCES quotes.quotes(quote_id)
);
