CREATE SCHEMA bim;

CREATE SEQUENCE bim.rides__id AS bigint;

CREATE TABLE bim.rides
( id bigint NOT NULL DEFAULT nextval('bim.rides__id')
, company character varying(256) NOT NULL
, vehicle_number bigint NOT NULL
, rider_username character varying(256) NOT NULL
, "timestamp" timestamp with time zone NOT NULL
, line character varying(32) NULL
, CONSTRAINT pkey_rides PRIMARY KEY (id)
, CONSTRAINT check_rides CHECK (vehicle_number >= 0)
);
