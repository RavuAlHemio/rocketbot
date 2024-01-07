CREATE OR REPLACE VIEW bim.numeric_line_rides_and_ridden_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", bim.char_to_bigint_or_null(r.line) line
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    rv.coupling_mode = 'R'
    AND bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
    AND bim.char_to_bigint_or_null(r.line) IS NOT NULL
;

UPDATE bim.schema_revision SET sch_rev=13;
