ALTER TABLE bim.ride_vehicles ADD COLUMN coupling_mode character(1) NULL;

UPDATE bim.ride_vehicles
SET coupling_mode = CASE
    WHEN as_part_of_fixed_coupling THEN 'F'
    ELSE 'E'
END
;

ALTER TABLE bim.ride_vehicles ALTER COLUMN coupling_mode SET NOT NULL;
ALTER TABLE bim.ride_vehicles ADD CONSTRAINT check_ride_vehicles_coupling_mode CHECK (coupling_mode IN ('R', 'E', 'F'));

DROP VIEW bim.rides_and_vehicles;
CREATE VIEW bim.rides_and_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line
    , rv.vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
;

DROP VIEW bim.rides_and_numeric_vehicles;
CREATE VIEW bim.rides_and_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
;

ALTER TABLE bim.ride_vehicles DROP COLUMN as_part_of_fixed_coupling;

UPDATE bim.ride_vehicles rv
SET coupling_mode = 'R'
WHERE
    rv.coupling_mode = 'E'
    AND NOT EXISTS (
        SELECT 1
        FROM bim.ride_vehicles rv2
        WHERE rv2.ride_id = rv.ride_id
        AND rv2.vehicle_number <> rv.vehicle_number
        AND rv2.coupling_mode <> 'F'
    )
;

UPDATE bim.schema_revision SET sch_rev=6;
