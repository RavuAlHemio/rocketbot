CREATE TABLE linguistics.german_genders
( word varchar(256) NOT NULL PRIMARY KEY
, masculine bool NOT NULL
, feminine bool NOT NULL
, neuter bool NOT NULL
, singulare_tantum bool NOT NULL
, plurale_tantum bool NOT NULL
);
