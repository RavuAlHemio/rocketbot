CREATE SEQUENCE linguistics.seq__german_genders__id AS bigint;

CREATE TABLE linguistics.german_genders
( id bigint NOT NULL PRIMARY KEY DEFAULT nextval('linguistics.seq__german_genders__id')
, word varchar(256) NOT NULL
, gender_flags bigint NOT NULL
);
CREATE INDEX idx_german_genders__word ON linguistics.german_genders (word);
CREATE INDEX idx_german_genders__lower_word ON linguistics.german_genders (LOWER(word));
