CREATE EXTENSION IF NOT EXISTS plpython3u;

CREATE SCHEMA bim;

CREATE SEQUENCE bim.rides__id AS bigint;

CREATE TABLE bim.rides
( id bigint NOT NULL DEFAULT nextval('bim.rides__id')
, company character varying(256) NOT NULL
, rider_username character varying(256) NOT NULL
, "timestamp" timestamp with time zone NOT NULL
, line character varying(32) NULL
, regular_price numeric(19, 5) NOT NULL DEFAULT 0.0
, actual_price numeric(19, 5) NOT NULL DEFAULT 0.0
, CONSTRAINT pkey_rides PRIMARY KEY (id)
);

CREATE TABLE bim.ride_vehicles
( ride_id bigint NOT NULL
, vehicle_number character varying(256) NOT NULL
, vehicle_type character varying(256) NULL
, spec_position bigint NOT NULL
, fixed_coupling_position bigint NOT NULL
, coupling_mode character(1) NOT NULL -- 'R' = explicit and actually ridden, 'E' = explicit, 'F' = as part of fixed coupling
, CONSTRAINT fkey_ride_vehicles_ride_id FOREIGN KEY (ride_id) REFERENCES bim.rides (id) ON DELETE CASCADE DEFERRABLE
, CONSTRAINT pkey_ride_vehicles PRIMARY KEY (ride_id, vehicle_number)
, CONSTRAINT check_ride_vehicles_coupling_mode CHECK (coupling_mode IN ('R', 'E', 'F'))
);

CREATE VIEW bim.rides_and_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line, r.regular_price, r.actual_price
    , rv.vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
;

CREATE VIEW bim.rides_and_ridden_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line, r.regular_price, r.actual_price
    , rv.vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE rv.coupling_mode = 'R'
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

CREATE OR REPLACE VIEW bim.rides_and_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line, r.regular_price, r.actual_price
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
;

CREATE OR REPLACE VIEW bim.rides_and_ridden_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", r.line, r.regular_price, r.actual_price
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    rv.coupling_mode = 'R'
    AND bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
;

CREATE OR REPLACE VIEW bim.numeric_line_rides_and_ridden_numeric_vehicles AS
SELECT r.id, r.company, r.rider_username, r."timestamp", bim.char_to_bigint_or_null(r.line) line, r.regular_price, r.actual_price
    , bim.char_to_bigint_or_null(rv.vehicle_number) vehicle_number, rv.vehicle_type, rv.spec_position, rv.coupling_mode, rv.fixed_coupling_position
FROM bim.rides r
INNER JOIN bim.ride_vehicles rv ON rv.ride_id = r.id
WHERE
    rv.coupling_mode = 'R'
    AND bim.char_to_bigint_or_null(rv.vehicle_number) IS NOT NULL
    AND bim.char_to_bigint_or_null(r.line) IS NOT NULL
;

CREATE OR REPLACE FUNCTION bim.natural_compare
( left_oper text
, right_oper text
) RETURNS integer
LANGUAGE plpgsql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    DECLARE
        left_index integer NOT NULL := 1;
        right_index integer NOT NULL := 1;
        smaller_length integer;
        left_chunks text[];
        right_chunks text[];
        left_chunk text;
        right_chunk text;
        left_number numeric;
        right_number numeric;
    BEGIN
        IF left_oper IS NULL OR right_oper IS NULL THEN
            RETURN NULL;
        END IF;

        WHILE left_index <= LENGTH(left_oper) AND right_index <= LENGTH(right_oper) LOOP
            -- try grabbing digits from left
            left_chunks := regexp_match(SUBSTRING(left_oper FROM left_index), '^[0-9]+');
            right_chunks := regexp_match(SUBSTRING(right_oper FROM right_index), '^[0-9]+');
            left_chunk := left_chunks[1];
            right_chunk := right_chunks[1];

            IF left_chunk IS NULL OR right_chunk IS NULL THEN
                EXIT;
            END IF;

            -- handle mixed cases first
            IF LENGTH(left_chunk) > 0 AND LENGTH(right_chunk) = 0 THEN
                -- sort digits first: left < right
                RETURN -1;
            END IF;
            IF LENGTH(left_chunk) = 0 AND LENGTH(right_chunk) > 0 THEN
                -- sort digits first: left > right
                RETURN 1;
            END IF;

            IF LENGTH(left_chunk) > 0 THEN
                -- handle numbers
                left_number = CAST(left_chunk AS numeric);
                right_number = CAST(right_chunk AS numeric);
                IF left_number < right_number THEN
                    RETURN -1;
                END IF;
                IF left_number > right_number THEN
                    RETURN 1;
                END IF;

                -- numbers are equal

                -- sort equal-but-not-identical numbers (e.g. due to leading zeroes) ASCIIbetically
                IF left_chunk < right_chunk THEN
                    RETURN -1;
                END IF;
                IF left_chunk > right_chunk THEN
                    RETURN 1;
                END IF;

                -- numbers are identical; skip over them and keep going
                left_index := left_index + LENGTH(left_chunk);
                right_index := right_index + LENGTH(right_chunk);
            END IF;

            -- grab non-digit characters from left
            left_chunks := regexp_match(SUBSTRING(left_oper FROM left_index), '^[^0-9]+');
            right_chunks := regexp_match(SUBSTRING(right_oper FROM right_index), '^[^0-9]+');
            left_chunk := left_chunks[1];
            right_chunk := right_chunks[1];

            IF left_chunk IS NULL OR right_chunk IS NULL THEN
                EXIT;
            END IF;

            -- compare ASCIIbetically
            IF left_chunk < right_chunk THEN
                RETURN -1;
            END IF;
            IF left_chunk > right_chunk THEN
                RETURN 1;
            END IF;

            -- still the same; loop over
            left_index := left_index + LENGTH(left_chunk);
            right_index := right_index + LENGTH(right_chunk);
        END LOOP;

        -- common prefix to both functions; compare lengths
        IF LENGTH(left_oper) < LENGTH(right_oper) THEN
            RETURN -1;
        END IF;
        IF LENGTH(left_oper) > LENGTH(right_oper) THEN
            RETURN 1;
        END IF;

        -- equal lengths as well
        RETURN 0;
    END;
$$;

CREATE OR REPLACE FUNCTION bim.natural_lt(left_oper text, right_oper text) RETURNS boolean
LANGUAGE sql IMMUTABLE LEAKPROOF STRICT PARALLEL SAFE
RETURN bim.natural_compare(left_oper, right_oper) = -1;
CREATE OR REPLACE FUNCTION bim.natural_leq(left_oper text, right_oper text) RETURNS boolean
LANGUAGE sql IMMUTABLE LEAKPROOF STRICT PARALLEL SAFE
RETURN bim.natural_compare(left_oper, right_oper) <> 1;
CREATE OR REPLACE FUNCTION bim.natural_gt(left_oper text, right_oper text) RETURNS boolean
LANGUAGE sql IMMUTABLE LEAKPROOF STRICT PARALLEL SAFE
RETURN bim.natural_compare(left_oper, right_oper) = 1;
CREATE OR REPLACE FUNCTION bim.natural_geq(left_oper text, right_oper text) RETURNS boolean
LANGUAGE sql IMMUTABLE LEAKPROOF STRICT PARALLEL SAFE
RETURN bim.natural_compare(left_oper, right_oper) <> -1;

CREATE OPERATOR bim.<~<
( LEFTARG = text
, RIGHTARG = text
, FUNCTION = bim.natural_lt
, COMMUTATOR = OPERATOR(bim.>~>)
, NEGATOR = OPERATOR(bim.>~>=)
);
CREATE OPERATOR bim.<~<=
( LEFTARG = text
, RIGHTARG = text
, FUNCTION = bim.natural_leq
, COMMUTATOR = OPERATOR(bim.>~>=)
, NEGATOR = OPERATOR(bim.>~>)
);
CREATE OPERATOR bim.>~>
( LEFTARG = text
, RIGHTARG = text
, FUNCTION = bim.natural_gt
, COMMUTATOR = OPERATOR(bim.<~<)
, NEGATOR = OPERATOR(bim.<~<=)
);
CREATE OPERATOR bim.>~>=
( LEFTARG = text
, RIGHTARG = text
, FUNCTION = bim.natural_geq
, COMMUTATOR = OPERATOR(bim.<~<=)
, NEGATOR = OPERATOR(bim.<~<)
);

CREATE OPERATOR CLASS bim.natural_compare_class
FOR TYPE text
USING btree
AS  OPERATOR 1 bim.<~<
,   OPERATOR 2 bim.<~<=
,   OPERATOR 3 =
,   OPERATOR 4 bim.>~>=
,   OPERATOR 5 bim.>~>
,   FUNCTION 1 bim.natural_compare
;

CREATE OR REPLACE FUNCTION bim.to_transport_date
( tstamp timestamp with time zone
) RETURNS date
LANGUAGE sql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    -- times before 04:00 are counted towards the previous day
    SELECT
        CASE
            WHEN tstamp IS NULL THEN NULL
            WHEN EXTRACT(HOUR FROM tstamp) < 4 THEN CAST(tstamp - INTERVAL 'P1D' AS date)
            ELSE CAST(tstamp AS date)
        END transport_date
$$;

CREATE INDEX IF NOT EXISTS idx_ride_vehicles_ridden ON bim.ride_vehicles (ride_id, vehicle_number) WHERE coupling_mode = 'R';

CREATE OR REPLACE FUNCTION bim.ridden_vehicles_between_riders
( same_rider_also boolean
) RETURNS TABLE
( id bigint
, company character varying(256)
, vehicle_number character varying(256)
, "timestamp" timestamp with time zone
, old_rider character varying(256)
, new_rider character varying(256)
)
LANGUAGE plpython3u
STABLE STRICT
AS $$
if same_rider_also is None:
    return None

company_to_vehicle_to_last_rider = {}
for row in plpy.cursor('SELECT rav.id, rav.company, rav.vehicle_number, rav."timestamp", rav.rider_username FROM bim.rides_and_vehicles rav WHERE rav.coupling_mode = \'R\' ORDER BY rav."timestamp", rav.id'):
    company = row["company"]
    vehicle_number = row["vehicle_number"]

    try:
        vehicle_to_last_rider = company_to_vehicle_to_last_rider[company]
    except KeyError:
        vehicle_to_last_rider = {}
        company_to_vehicle_to_last_rider[company] = vehicle_to_last_rider

    old_rider = vehicle_to_last_rider.get(vehicle_number)
    new_rider = row["rider_username"]

    if not same_rider_also and old_rider == new_rider:
        continue

    vehicle_to_last_rider[vehicle_number] = new_rider
    yield (row["id"], company, vehicle_number, row["timestamp"], old_rider, new_rider)
$$;

CREATE VIEW bim.rides_vehicle_arrays_ridden_fixed AS
SELECT
  r.id,
  r."timestamp",
  r.company,
  r.rider_username,
  JSONB_AGG(
    JSONB_BUILD_OBJECT('n', rv.vehicle_number, 'm', rv.coupling_mode)
    ORDER BY
      rv.spec_position,
      rv.fixed_coupling_position
  ) vehicles
FROM
  bim.rides r
  INNER JOIN bim.ride_vehicles rv
    ON rv.ride_id = r.id
WHERE
  EXISTS (
    SELECT 1
    FROM bim.ride_vehicles rv2
    WHERE rv2.ride_id = r.id
    AND rv2.coupling_mode = 'R'
  )
  AND EXISTS (
    SELECT 1
    FROM bim.ride_vehicles rv3
    WHERE rv3.ride_id = r.id
    AND rv3.coupling_mode = 'F'
  )
GROUP BY
  r.id,
  r."timestamp",
  r.company,
  r.rider_username
;

CREATE OR REPLACE FUNCTION bim.current_monopolies
(
) RETURNS TABLE
( company character varying(256)
, rider_username character varying(256)
, vehicles character varying(256)[]
)
LANGUAGE plpython3u
STABLE STRICT
AS $$
import json

class Bag:
    def __init__(self, initial_values=None):
        self._list = []
        self._set = set()

        if initial_values is not None:
            for initial_value in initial_value:
                self.add(initial_value)

    def __len__(self):
        return len(self._list)

    def __iter__(self):
        return iter(self._list)

    def add(self, value):
        if value in self._set:
            return
        self._set.add(value)
        self._list.append(value)

    def __contains__(self, value):
        return value in self._set

company_to_vehicle_to_coupling = {}
company_to_vehicle_to_last_rider = {}
for row in plpy.cursor('SELECT id, "timestamp", company, rider_username, vehicles FROM bim.rides_vehicle_arrays_ridden_fixed ORDER BY "timestamp", id'):
    company = row["company"]
    rider_username = row["rider_username"]
    vehicles = json.loads(row["vehicles"])

    try:
        vehicle_to_coupling = company_to_vehicle_to_coupling[company]
    except KeyError:
        vehicle_to_coupling = {}
        company_to_vehicle_to_coupling[company] = vehicle_to_coupling

    try:
        vehicle_to_last_rider = company_to_vehicle_to_last_rider[company]
    except KeyError:
        vehicle_to_last_rider = {}
        company_to_vehicle_to_last_rider[company] = vehicle_to_last_rider

    vehicle_bag = Bag()
    for vehicle_dict in vehicles:
        vehicle_bag.add(vehicle_dict["n"])
    for vehicle in vehicle_bag:
        vehicle_to_coupling[vehicle] = vehicle_bag

    for vehicle_dict in vehicles:
        if vehicle_dict["m"] == "R":
            vehicle_to_last_rider[vehicle_dict["n"]] = rider_username

for (company, vehicle_to_coupling) in company_to_vehicle_to_coupling.items():
    vehicle_to_last_rider = company_to_vehicle_to_last_rider[company]

    known_vehicles = set()
    for (vehicle, vehicle_bag) in vehicle_to_coupling.items():
        if not vehicle_bag:
            continue
        if vehicle in known_vehicles:
            continue
        known_vehicles.update(vehicle_bag)

        coupled_vehicles = list(vehicle_bag)
        first_rider = vehicle_to_last_rider.get(coupled_vehicles[0])
        if first_rider is None:
            continue
        is_monopoly = True
        for next_vehicle in coupled_vehicles[1:]:
            next_rider = vehicle_to_last_rider.get(next_vehicle)
            if next_rider != first_rider:
                is_monopoly = False
                break

        if is_monopoly:
            yield (company, first_rider, coupled_vehicles)
$$;

CREATE TABLE bim.schema_revision
( sch_rev bigint NOT NULL
);
INSERT INTO bim.schema_revision (sch_rev) VALUES (19);
