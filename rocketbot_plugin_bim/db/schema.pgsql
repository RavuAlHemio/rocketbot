CREATE SCHEMA bim;

CREATE SEQUENCE bim.rides__id AS bigint;

CREATE TABLE bim.rides
( id bigint NOT NULL DEFAULT nextval('bim.rides__id')
, company character varying(256) NOT NULL
, rider_username character varying(256) NOT NULL
, "timestamp" timestamp with time zone NOT NULL
, line character varying(32) NULL
, CONSTRAINT pkey_rides PRIMARY KEY (id)
);

CREATE TABLE bim.ride_vehicles
( ride_id bigint NOT NULL
, vehicle_number bigint NOT NULL
, as_part_of_fixed_coupling boolean NOT NULL
, CONSTRAINT fkey_ride_vehicles_ride_id FOREIGN KEY (ride_id) REFERENCES bim.rides (id)
, CONSTRAINT pkey_ride_vehicles PRIMARY KEY (ride_id, vehicle_number)
, CONSTRAINT check_ride_vehicles CHECK (vehicle_number >= 0)
);
