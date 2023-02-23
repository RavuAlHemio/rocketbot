ALTER TABLE bim.ride_vehicles ADD COLUMN coupling_mode character(1) NULL;

UPDATE bim.ride_vehicles
SET coupling_mode = CASE
    WHEN as_part_of_fixed_coupling THEN 'F'
    ELSE 'E'
END
;

ALTER TABLE bim.ride_vehicles ALTER COLUMN coupling_mode SET NOT NULL;
ALTER TABLE bim.ride_vehicles ADD CONSTRAINT check_ride_vehicles_coupling_mode (coupling_mode IN ('R', 'E', 'F'));

CREATE OR REPLACE VIEW bim.rides_and_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line
    , rv.vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
;

CREATE OR REPLACE VIEW bim.rides_and_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
;

ALTER TABLE bim.ride_vehicles DROP COLUMN as_part_of_fixed_coupling;

UPDATE bim.schema_revision SET sch_rev=6;
