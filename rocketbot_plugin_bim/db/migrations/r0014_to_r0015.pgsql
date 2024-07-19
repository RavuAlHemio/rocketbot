ALTER TABLE bim.rides ADD COLUMN regular_price numeric(19, 5) NOT NULL DEFAULT 0.0;
ALTER TABLE bim.rides ADD COLUMN actual_price numeric(19, 5) NOT NULL DEFAULT 0.0;

DROP VIEW bim.rides_and_vehicles;
CREATE VIEW bim.rides_and_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line, r.regular_price, r.actual_price
    , rv.vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
;

DROP VIEW bim.rides_and_ridden_vehicles;
CREATE VIEW bim.rides_and_ridden_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line, r.regular_price, r.actual_price
    , rv.vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE rv.coupling_mode = 'R'
;

DROP VIEW bim.rides_and_numeric_vehicles;
CREATE VIEW bim.rides_and_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line, r.regular_price, r.actual_price
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
;

DROP VIEW bim.rides_and_ridden_numeric_vehicles;
CREATE VIEW bim.rides_and_ridden_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line, r.regular_price, r.actual_price
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    rv.coupling_mode = 'R'
    AND bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
;

DROP VIEW bim.numeric_line_rides_and_ridden_numeric_vehicles;
CREATE VIEW bim.numeric_line_rides_and_ridden_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", bim.char_to_bigint_or_null(r.line) line, r.regular_price, r.actual_price
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    rv.coupling_mode = 'R'
    AND bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
    AND bim.char_to_bigint_or_null(r.line) IS NOT NULL
;

UPDATE bim.schema_revision SET sch_rev=15;
