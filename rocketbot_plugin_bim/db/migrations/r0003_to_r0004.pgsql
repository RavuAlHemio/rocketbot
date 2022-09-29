DROP VIEW bim.rides_and_vehicles;

CREATE TABLE bim.ride_vehicles2
( ride_id bigint NOT NULL
, vehicle_number character varying(256) NOT NULL
, vehicle_type character varying(256) NULL
, spec_position bigint NOT NULL
, as_part_of_fixed_coupling boolean NOT NULL
, fixed_coupling_position bigint NOT NULL
, CONSTRAINT fkey_ride_vehicles2_ride_id FOREIGN KEY (ride_id) REFERENCES bim.rides (id) ON DELETE CASCADE DEFERRABLE
, CONSTRAINT pkey_ride_vehicles2 PRIMARY KEY (ride_id, vehicle_number)
);
INSERT INTO bim.ride_vehicles2 SELECT * FROM bim.ride_vehicles;
DROP TABLE bim.ride_vehicles;
ALTER TABLE bim.ride_vehicles2 RENAME TO ride_vehicles;

CREATE VIEW bim.rides_and_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line
    , rv.vehicle_number, rv.vehicle_type, rv.spec_position, rv.as_part_of_fixed_coupling, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
;

CREATE OR REPLACE FUNCTION bim.char_to_bigint_or_null
( val character varying
) RETURNS bigint
LANGUAGE plpgsql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    BEGIN
        IF val IS NULL THEN
            RETURN NULL;
        END IF;
        RETURN CAST(val AS bigint);
    EXCEPTION
        WHEN invalid_text_representation OR numeric_value_out_of_range THEN
            RETURN NULL;
    END;
$$;

CREATE VIEW bim.rides_and_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.as_part_of_fixed_coupling, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
;

UPDATE bim.schema_revision SET sch_rev=4;
