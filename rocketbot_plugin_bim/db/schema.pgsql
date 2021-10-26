CREATE SCHEMA bim;

CREATE TABLE bim.last_rides
( company character varying(256) NOT NULL
, vehicle_number bigint NOT NULL
, rider_username character varying(256) NOT NULL
, ride_count bigint NOT NULL
, last_ride timestamp with time zone NOT NULL
, last_line character varying(32) NULL
, CONSTRAINT pkey_last_rides PRIMARY KEY (company, vehicle_number, rider_username)
, CONSTRAINT check_last_rides CHECK (vehicle_number >= 0)
);
