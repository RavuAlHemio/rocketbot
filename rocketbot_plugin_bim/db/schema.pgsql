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
, vehicle_type character varying(256) NULL
, spec_position bigint NOT NULL
, as_part_of_fixed_coupling boolean NOT NULL
, fixed_coupling_position bigint NOT NULL
, CONSTRAINT fkey_ride_vehicles_ride_id FOREIGN KEY (ride_id) REFERENCES bim.rides (id) ON DELETE CASCADE DEFERRABLE
, CONSTRAINT pkey_ride_vehicles PRIMARY KEY (ride_id, vehicle_number)
, CONSTRAINT check_ride_vehicles CHECK (vehicle_number >= 0)
);

CREATE VIEW bim.rides_and_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line
    , rv.vehicle_number, rv.vehicle_type, rv.spec_position, rv.as_part_of_fixed_coupling, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
;

CREATE TABLE bim.schema_revision
( sch_rev bigint NOT NULL
);
INSERT INTO bim.schema_revision (sch_rev) VALUES (2);
