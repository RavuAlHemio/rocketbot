CREATE OR REPLACE FUNCTION bim.same_chars
( val character varying
, min_length bigint
) RETURNS boolean
LANGUAGE plpgsql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    DECLARE
        digit character;
    BEGIN
        IF val IS NULL OR min_length IS NULL THEN
            RETURN NULL;
        END IF;
        IF LENGTH(val) < min_length THEN
            RETURN FALSE;
        END IF;
        IF LENGTH(val) < 2 THEN
            RETURN TRUE;
        END IF;

        digit := SUBSTRING(val FROM 1 FOR 1);
        FOR i IN 2..LENGTH(val) LOOP
            IF SUBSTRING(val FROM i FOR 1) <> digit THEN
                RETURN FALSE;
            END IF;
        END LOOP;
        RETURN TRUE;
    END;
$$;

CREATE OR REPLACE FUNCTION bim.smallest_factor
( val bigint
) RETURNS bigint
LANGUAGE plpgsql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    DECLARE
        test_num bigint NOT NULL := 2;
    BEGIN
        IF val IS NULL THEN
            RETURN NULL;
        END IF;
        IF val < 2 THEN
            RETURN val;
        END IF;
        WHILE val > test_num LOOP
            IF MOD(val, test_num) = 0 THEN
                RETURN test_num;
            END IF;
            test_num := test_num + 1;
        END LOOP;
        RETURN val;
    END;
$$;

CREATE OR REPLACE FUNCTION bim.is_prime
( val bigint
) RETURNS boolean
LANGUAGE sql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    SELECT val > 1 AND bim.smallest_factor(val) = val
$$;

CREATE OR REPLACE FUNCTION bim.is_in_sequence
( val bigint
, downward boolean
) RETURNS boolean
LANGUAGE plpgsql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    DECLARE
        last_digit bigint;
        current_digit bigint;
    BEGIN
        IF val IS NULL OR downward IS NULL THEN
            RETURN NULL;
        END IF;
        IF val < 0 THEN
            val := -val;
        END IF;
        last_digit := MOD(val, 10);
        val := DIV(val, 10);
        WHILE val > 0 LOOP
            current_digit := MOD(val, 10);
            IF downward AND current_digit <> last_digit + 1 THEN
                RETURN FALSE;
            ELSIF NOT downward AND current_digit <> last_digit - 1 THEN
                RETURN FALSE;
            END IF;
            last_digit := current_digit;
            val := DIV(val, 10);
        END LOOP;
        RETURN TRUE;
    END;
$$;

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

CREATE OR REPLACE FUNCTION bim.longest_sequence_of
( rider character varying
, max_timestamp timestamp with time zone
) RETURNS bigint
LANGUAGE plpgsql
STABLE
AS $$
    DECLARE
        longest_sequence bigint NOT NULL := 0;
        current_sequence bigint NOT NULL := 1;
        comp character varying;
        vehicle_num bigint;
        last_vehicle bigint;
    BEGIN
        IF rider IS NULL THEN RETURN NULL; END IF;
        FOR comp IN
            SELECT DISTINCT company
            FROM bim.rides
            WHERE rider_username = rider
            AND (
                max_timestamp IS NULL
                OR max_timestamp >= "timestamp"
            )
        LOOP
            current_sequence := 1;
            last_vehicle := -2;
            FOR vehicle_num IN
                SELECT DISTINCT vehicle_number
                FROM bim.rides_and_numeric_vehicles
                WHERE company = comp
                AND rider_username = rider
                AND (
                    max_timestamp IS NULL
                    OR max_timestamp >= "timestamp"
                )
                ORDER BY vehicle_number
            LOOP
                IF last_vehicle + 1 = vehicle_num THEN
                    current_sequence := current_sequence + 1;
                ELSE
                    IF longest_sequence < current_sequence THEN
                        longest_sequence := current_sequence;
                    END IF;
                    current_sequence := 1;
                END IF;
                last_vehicle := vehicle_num;
            END LOOP;
            IF longest_sequence < current_sequence THEN
                longest_sequence := current_sequence;
            END IF;
        END LOOP;
        RETURN longest_sequence;
    END;
$$;

CREATE OR REPLACE FUNCTION bim.sequence_of_reached
( rider character varying
, sequence_length bigint
) RETURNS timestamp with time zone
LANGUAGE plpgsql
STABLE STRICT
AS $$
    DECLARE
        current_sequence bigint NOT NULL := 1;
        comp character varying;
        vehicle_num bigint;
        last_vehicle bigint;
        ts timestamp with time zone;
        last_timestamps timestamp with time zone[];
        temp_ts timestamp with time zone;
        max_timestamps timestamp with time zone[];
    BEGIN
        IF rider IS NULL THEN RETURN NULL; END IF;
        IF sequence_length IS NULL THEN RETURN NULL; END IF;
        FOR comp IN
            SELECT DISTINCT company
            FROM bim.rides
            WHERE rider_username = rider
        LOOP
            current_sequence := 1;
            last_vehicle := -2;
            last_timestamps := CAST(ARRAY[] AS timestamp with time zone[]);
            FOR vehicle_num, ts IN
                SELECT vehicle_number, MIN("timestamp")
                FROM bim.rides_and_numeric_vehicles
                WHERE company = comp
                AND rider_username = rider
                GROUP BY vehicle_number
                ORDER BY vehicle_number
            LOOP
                IF last_vehicle + 1 = vehicle_num THEN
                    current_sequence := current_sequence + 1;
                    last_timestamps := ARRAY_APPEND(last_timestamps, ts);
                    IF ARRAY_LENGTH(last_timestamps, 1) > sequence_length THEN
                        last_timestamps := last_timestamps[ARRAY_LENGTH(last_timestamps, 1) - sequence_length + 1:];
                    END IF;
                ELSE
                    current_sequence := 1;
                    last_timestamps := ARRAY[ts];
                END IF;
                IF current_sequence >= sequence_length THEN
                    SELECT MAX(tstamp) INTO temp_ts FROM UNNEST(last_timestamps) tstamp;
                    max_timestamps := ARRAY_APPEND(max_timestamps, temp_ts);
                END IF;
                last_vehicle := vehicle_num;
            END LOOP;
        END LOOP;
        SELECT MIN(tstamp) INTO temp_ts FROM UNNEST(max_timestamps) tstamp;
        RETURN temp_ts;
    END;
$$;

CREATE OR REPLACE FUNCTION bim.rider_rides_per_timespan_reached
( rider character varying
, timespan interval
, min_count bigint
) RETURNS timestamp with time zone
LANGUAGE sql
STABLE STRICT
AS $$
    SELECT
        MIN(rwin."timestamp") min_timestamp
    FROM
        (
            SELECT
                r."timestamp",
                COUNT(*) OVER (
                    ORDER BY r."timestamp"
                    RANGE BETWEEN timespan PRECEDING AND CURRENT ROW
                ) ride_count
            FROM bim.rides r
            WHERE r.rider_username = rider
        ) rwin
    WHERE
        rwin.ride_count >= min_count
$$;

CREATE OR REPLACE VIEW bim.ride_by_rider_vehicle_timestamp AS
    SELECT
        rider_username, company, vehicle_number, "timestamp",
        RANK() OVER (PARTITION BY rider_username, company, vehicle_number ORDER BY "timestamp") nth
    FROM bim.rides_and_vehicles
;
CREATE OR REPLACE VIEW bim.ride_by_rider_line_timestamp AS
    SELECT
        rider_username, company, line, "timestamp",
        RANK() OVER (PARTITION BY rider_username, company, line ORDER BY "timestamp") nth
    FROM bim.rides
;
CREATE OR REPLACE VIEW bim.ride_by_rider_vehicle_line_timestamp AS
    SELECT
        rider_username, company, vehicle_number, line, "timestamp",
        RANK() OVER (PARTITION BY rider_username, company, vehicle_number, line ORDER BY "timestamp") nth
    FROM bim.rides_and_vehicles
    WHERE line IS NOT NULL
;
CREATE OR REPLACE VIEW bim.first_ride_by_rider_vehicle_line AS
    SELECT
        rider_username, company, vehicle_number, line, MIN("timestamp") min_timestamp,
        RANK() OVER (PARTITION BY rider_username, company, vehicle_number ORDER BY MIN("timestamp")) nth
    FROM bim.rides_and_vehicles
    WHERE line IS NOT NULL
    GROUP BY rider_username, company, vehicle_number, line
;
CREATE OR REPLACE VIEW bim.ride_by_rider_day AS
    SELECT
        rider_username, bim.to_transport_date("timestamp"), "timestamp", company, vehicle_number,
        RANK() OVER (PARTITION BY rider_username, bim.to_transport_date("timestamp") ORDER BY "timestamp", company, vehicle_number) nth
    FROM bim.rides_and_vehicles
;
CREATE OR REPLACE VIEW bim.ride_by_rider_week AS
    SELECT
        rider_username, bim.to_transport_date("timestamp"), "timestamp", company, vehicle_number,
        RANK() OVER (PARTITION BY rider_username, bim.to_transport_date("timestamp") ORDER BY "timestamp", company, vehicle_number) nth
    FROM bim.rides_and_vehicles
;

CREATE OR REPLACE VIEW bim.first_vehicle_rides AS
    SELECT
        rav_a.rider_username, rav_a.company, rav_a.vehicle_number, rav_a."timestamp",
        RANK() OVER (PARTITION BY rider_username ORDER BY rav_a."timestamp", rav_a.company, rav_a.vehicle_number) nth_first
    FROM
        bim.rides_and_vehicles rav_a
    WHERE
        NOT EXISTS (
            SELECT 1 FROM bim.rides_and_vehicles rav_b
            WHERE
                rav_b.company = rav_a.company
                AND rav_b.vehicle_number = rav_a.vehicle_number
                AND rav_b."timestamp" < rav_a."timestamp"
        )
;

CREATE OR REPLACE FUNCTION bim.same_vehicle_consecutive_days_reached
( rider character varying
, day_count bigint
) RETURNS timestamp with time zone
LANGUAGE plpgsql
STABLE STRICT
AS $$
    DECLARE
        comp character varying;
        veh_num character varying;
        current_day_sequence bigint;
        days_elapsed bigint;
        previous_timestamp timestamp with time zone;
        ts timestamp with time zone;
        min_timestamp timestamp with time zone;
    BEGIN
        IF rider IS NULL THEN RETURN NULL; END IF;
        IF day_count IS NULL THEN RETURN NULL; END IF;
        min_timestamp := NULL;
        FOR comp, veh_num IN
            SELECT DISTINCT rav1.company, rav1.vehicle_number
            FROM bim.rides_and_vehicles rav1
            WHERE rav1.rider_username = rider
            ORDER BY rav1.company, rav1.vehicle_number
        LOOP
            current_day_sequence := 1;
            previous_timestamp := NULL;
            FOR ts IN
                SELECT rav2."timestamp"
                FROM bim.rides_and_vehicles rav2
                WHERE rav2.company = comp
                AND rav2.rider_username = rider
                AND rav2.vehicle_number = veh_num
                ORDER BY "timestamp"
            LOOP
                IF previous_timestamp IS NOT NULL THEN
                    days_elapsed := bim.to_transport_date(ts) - bim.to_transport_date(previous_timestamp);
                    IF days_elapsed = 1 THEN
                        current_day_sequence := current_day_sequence + 1;
                        IF current_day_sequence >= day_count AND (min_timestamp IS NULL OR min_timestamp > ts) THEN
                            min_timestamp := ts;
                        END IF;
                    ELSIF days_elapsed > 1 THEN
                        -- discontinuity
                        current_day_sequence := 1;
                    END IF;
                    -- if days_elapsed = 0, it's not a discontinuity, but don't count it as another day either
                END IF;
                previous_timestamp := ts;
            END LOOP;
        END LOOP;
        RETURN min_timestamp;
    END;
$$;

CREATE OR REPLACE FUNCTION bim.repdigit_unique_vehicle_ride
( rider character varying
, min_digits bigint
, nth_ride bigint
) RETURNS timestamp with time zone
LANGUAGE sql
STABLE STRICT
AS $$
    WITH
        first_repdigit_ride(rider_username, company, vehicle_number, min_timestamp) AS (
            SELECT
                rider_username, company, vehicle_number, MIN("timestamp")
            FROM bim.rides_and_vehicles
            WHERE
                bim.same_chars(vehicle_number, min_digits)
            GROUP BY
                rider_username, company, vehicle_number
        ),
        first_repdigit_ride_ranked(rider_username, company, vehicle_number, min_timestamp, nth) AS (
            SELECT
                rider_username, company, vehicle_number, min_timestamp,
                RANK() OVER (PARTITION BY rider_username ORDER BY min_timestamp, company, vehicle_number) nth
            FROM first_repdigit_ride
        )
    SELECT min_timestamp
    FROM first_repdigit_ride_ranked
    WHERE rider_username = rider
    AND nth = nth_ride
$$;

CREATE OR REPLACE VIEW bim.rider_first_palindrome_vehicle_ride AS
    WITH first_palindrome_ride(rider_username, company, vehicle_number, min_timestamp) AS (
        SELECT
            rider_username, company, vehicle_number, MIN("timestamp")
        FROM bim.rides_and_vehicles
        WHERE
            LENGTH(vehicle_number) > 2
            AND vehicle_number = reverse(vehicle_number)
            AND NOT bim.same_chars(vehicle_number, 0)
        GROUP BY
            rider_username, company, vehicle_number
    )
    SELECT
        rider_username, company, vehicle_number, min_timestamp,
        RANK() OVER (PARTITION BY rider_username ORDER BY min_timestamp, company, vehicle_number) nth
    FROM first_palindrome_ride
;

CREATE OR REPLACE FUNCTION bim.same_ride_to_minute_interval_ago
( interval_ago interval
) RETURNS TABLE
( rider_username character varying(256)
, company character varying(256)
, line character varying(32)
, vehicle_number character varying(256)
, "timestamp" timestamp with time zone
)
LANGUAGE sql
STABLE STRICT
AS $$
    WITH ride_vehicles_interval_ago(rider_username, company, line, vehicle_number, timestamp_exact, timestamp_minute, timestamp_minute_day_ago) AS (
        SELECT
            rider_username, company, line, vehicle_number, "timestamp", DATE_TRUNC('minute', "timestamp"), DATE_TRUNC('minute', "timestamp" - interval_ago)
        FROM bim.rides_and_vehicles
    )
    SELECT
        later.rider_username, later.company, later.line, later.vehicle_number, later.timestamp_exact
    FROM
        ride_vehicles_interval_ago later
    WHERE
        EXISTS (
            SELECT 1
            FROM ride_vehicles_interval_ago earlier
            WHERE earlier.rider_username = later.rider_username
            AND earlier.company = later.company
            AND earlier.line = later.line
            AND earlier.vehicle_number = later.vehicle_number
            AND earlier.timestamp_minute = later.timestamp_minute_day_ago
        )
$$;

CREATE TABLE IF NOT EXISTS bim.not_thursday_timestamp
( rider_username character varying(256)
, "timestamp" timestamp with time zone
, CONSTRAINT pkey_not_thursday_timestamp PRIMARY KEY (rider_username)
);

CREATE OR REPLACE VIEW bim.riders AS
SELECT DISTINCT rider_username AS rider_username FROM bim.rides;

-- for every combination of user and ride, this view contains the number
-- of vehicles the user was the last rider of at the time of that ride
CREATE OR REPLACE VIEW bim.last_rides AS
SELECT r1.rider_username, r2.id ride_id, r2."timestamp", (
    SELECT COUNT(*) vehicle_count
    FROM bim.rides_and_vehicles rav1
    WHERE rav1.rider_username = r1.rider_username
    AND rav1.id <= r2.id
    AND NOT EXISTS (
        -- same vehicle, later timestamp
        SELECT 1
        FROM bim.rides_and_vehicles rav2
        WHERE rav2.company = rav1.company
        AND rav2.vehicle_number = rav1.vehicle_number
        AND rav2."timestamp" > rav1."timestamp"
        AND rav2.id <= r2.id
    )
)
FROM bim.riders r1
    CROSS JOIN bim.rides r2;

CREATE OR REPLACE PROCEDURE bim.refresh_achievements()
LANGUAGE plpgsql
AS $$
    DECLARE
        rider character varying;
        ts timestamp with time zone;
    BEGIN
        REFRESH MATERIALIZED VIEW bim.rider_achievements WITH DATA;

        -- admin should tactically remove entries from the not_thursday_timestamp table
        -- for trolling purposes
        FOR rider, ts IN
            SELECT r.rider_username, MAX("timestamp") FROM bim.rides r
            WHERE EXTRACT(dow FROM "timestamp") <> 4
            AND NOT EXISTS (
                SELECT 1
                FROM bim.not_thursday_timestamp ntt
                WHERE ntt.rider_username = r.rider_username
            )
            GROUP BY rider_username
        LOOP
            INSERT INTO bim.not_thursday_timestamp
                (rider_username, "timestamp")
            VALUES
                (rider, ts);
        END LOOP;
    END;
$$;

CREATE OR REPLACE FUNCTION bim.achievements_of
( rider character varying(256)
) RETURNS TABLE
( achievement_id bigint
, achieved_on timestamp with time zone
)
LANGUAGE sql
AS $$
    -- NAME: Beastly
    -- DESCR: Ride a vehicle (of any company) with number 666.
    -- ORDER: 1,1 special vehicle numbers
    SELECT 1, MIN("timestamp")
    FROM bim.rides_and_vehicles rav1
    WHERE rav1.rider_username = rider
    AND rav1.vehicle_number = '666'

    UNION ALL

    -- NAME: Nice
    -- DESCR: Ride a vehicle (of any company) with number 69.
    -- ORDER: 1,3 special vehicle numbers
    SELECT 2, MIN("timestamp")
    FROM bim.rides_and_vehicles rav2
    WHERE rav2.rider_username = rider
    AND rav2.vehicle_number = '69'

    UNION ALL

    -- NAME: Home Line
    -- DESCR: Ride a vehicle (of any company) where the vehicle number and the line are the same.
    -- ORDER: 2,1 vehicle numbers in relation to line numbers
    SELECT 3, MIN("timestamp")
    FROM bim.rides_and_vehicles rav3
    WHERE rav3.rider_username = rider
    AND rav3.line = rav3.vehicle_number

    UNION ALL

    -- NAME: Two of a Kind
    -- DESCR: Ride a vehicle (of any company) whose number consists of one digit repeated at least twice.
    -- ORDER: 10,1 repdigits
    SELECT 4, bim.repdigit_unique_vehicle_ride(rider, 2, 1)

    UNION ALL

    -- NAME: Three of a Kind
    -- DESCR: Ride a vehicle (of any company) whose number consists of one digit repeated at least three times.
    -- ORDER: 10,2 repdigits
    SELECT 5, bim.repdigit_unique_vehicle_ride(rider, 3, 1)

    UNION ALL

    -- NAME: Four of a Kind
    -- DESCR: Ride a vehicle (of any company) whose number consists of one digit repeated at least four times.
    -- ORDER: 10,3 repdigits
    SELECT 6, bim.repdigit_unique_vehicle_ride(rider, 4, 1)

    UNION ALL

    -- NAME: Palindrome
    -- DESCR: Ride a vehicle (of any company) whose number is a palindrome while not being all the same digit.
    -- ORDER: 9,1 palindromes
    SELECT 7, min_timestamp
    FROM bim.rider_first_palindrome_vehicle_ride rfpvr7
    WHERE rfpvr7.rider_username = rider
    AND rfpvr7.nth = 1

    UNION ALL

    -- NAME: Mirror Home Line
    -- DESCR: Ride a vehicle (of any company) where the vehicle number is the reverse of the line (but not the same as the line).
    -- ORDER: 2,2 vehicle numbers in relation to line numbers
    SELECT 8, MIN("timestamp")
    FROM bim.rides_and_vehicles rav8
    WHERE rav8.rider_username = rider
    AND REVERSE(rav8.line) = rav8.vehicle_number
    AND rav8.line <> rav8.vehicle_number

    UNION ALL

    -- NAME: Boeing
    -- DESCR: Ride a vehicle (of any company) whose number has the pattern "7x7".
    -- ORDER: 1,9 special vehicle numbers
    SELECT 9, MIN("timestamp")
    FROM bim.rides_and_vehicles rav9
    WHERE rav9.rider_username = rider
    AND LENGTH(rav9.vehicle_number) = 3
    AND SUBSTRING(rav9.vehicle_number FROM 1 FOR 1) = '7'
    AND SUBSTRING(rav9.vehicle_number FROM 3 FOR 1) = '7'

    UNION ALL

    -- NAME: Elsewhere
    -- DESCR: Ride two vehicles with the same vehicle number but different companies.
    -- ORDER: 3,1 vehicle numbers in relation to companies
    SELECT 10, MIN(rav10b."timestamp")
    FROM bim.rides_and_vehicles rav10a
    INNER JOIN bim.rides_and_vehicles rav10b
        ON rav10b.vehicle_number = rav10a.vehicle_number
        AND rav10b.rider_username = rav10a.rider_username
        AND rav10b.company <> rav10a.company
        AND rav10b."timestamp" > rav10a."timestamp"
    WHERE rav10b.rider_username = rider

    UNION ALL

    -- NAME: Monthiversary
    -- DESCR: Ride the same vehicle on the same day of two consecutive months.
    -- ORDER: 4,1 same vehicle
    SELECT 11, MIN(rav11b."timestamp")
    FROM bim.rides_and_vehicles rav11a
    INNER JOIN bim.rides_and_vehicles rav11b
        ON rav11b.rider_username = rav11a.rider_username
        AND rav11b.vehicle_number = rav11a.vehicle_number
        AND rav11b.company = rav11a.company
        AND CAST(rav11b."timestamp" AS date) = CAST((rav11a."timestamp" + CAST('P1M' AS interval)) AS date)
        -- days beyond the end of the month are saturated (2022-01-31 + P1M = 2022-02-28)
        -- counteract this
        AND EXTRACT(DAY FROM rav11b."timestamp") = EXTRACT(DAY FROM rav11a."timestamp")
    WHERE rav11a.rider_username = rider

    UNION ALL

    -- NAME: Anniversary
    -- DESCR: Ride the same vehicle on the same day of two consecutive years.
    -- ORDER: 4,2 same vehicle
    SELECT 12, MIN(rav12b."timestamp")
    FROM bim.rides_and_vehicles rav12a
    INNER JOIN bim.rides_and_vehicles rav12b
        ON rav12b.rider_username = rav12a.rider_username
        AND rav12b.vehicle_number = rav12a.vehicle_number
        AND rav12b.company = rav12a.company
        AND CAST(rav12b."timestamp" AS date) = CAST((rav12a."timestamp" + CAST('P1Y' AS interval)) AS date)
        -- days beyond the end of the month are saturated (2004-02-29 + P1Y = 2005-02-28)
        -- counteract this
        AND EXTRACT(DAY FROM rav12b."timestamp") = EXTRACT(DAY FROM rav12a."timestamp")
        AND EXTRACT(MONTH FROM rav12b."timestamp") = EXTRACT(MONTH FROM rav12a."timestamp")
    WHERE rav12a.rider_username = rider

    UNION ALL

    -- NAME: Same Time Next Week
    -- DESCR: Ride the same vehicle on the same weekday of two consecutive weeks.
    -- ORDER: 4,3 same vehicle
    SELECT 13, MIN(rav13b."timestamp")
    FROM bim.rides_and_vehicles rav13a
    INNER JOIN bim.rides_and_vehicles rav13b
        ON rav13b.rider_username = rav13a.rider_username
        AND rav13b.vehicle_number = rav13a.vehicle_number
        AND rav13b.company = rav13a.company
        AND CAST(rav13b."timestamp" AS date) = CAST((rav13a."timestamp" + CAST('P7D' AS interval)) AS date)
    WHERE rav13a.rider_username = rider

    UNION ALL

    -- NAME: Five Sweep
    -- DESCR: Collect rides with five vehicles of the same company with consecutive numbers.
    -- ORDER: 5,1 consecutive numbers
    SELECT 14, bim.sequence_of_reached(rider, 5)

    UNION ALL

    -- NAME: Ten Sweep
    -- DESCR: Collect rides with ten vehicles of the same company with consecutive numbers.
    -- ORDER: 5,2 consecutive numbers
    SELECT 15, bim.sequence_of_reached(rider, 10)

    UNION ALL

    -- NAME: Twenty Sweep
    -- DESCR: Collect rides with twenty vehicles of the same company with consecutive numbers.
    -- ORDER: 5,3 consecutive numbers
    SELECT 16, bim.sequence_of_reached(rider, 20)

    UNION ALL

    -- NAME: Thirty Sweep
    -- DESCR: Collect rides with thirty vehicles of the same company with consecutive numbers.
    -- ORDER: 5,4 consecutive numbers
    SELECT 17, bim.sequence_of_reached(rider, 30)

    UNION ALL

    -- NAME: Forty Sweep
    -- DESCR: Collect rides with forty vehicles of the same company with consecutive numbers.
    -- ORDER: 5,5 consecutive numbers
    SELECT 18, bim.sequence_of_reached(rider, 40)

    UNION ALL

    -- NAME: Half-Century Sweep
    -- DESCR: Collect rides with fifty vehicles of the same company with consecutive numbers.
    -- ORDER: 5,6 consecutive numbers
    SELECT 19, bim.sequence_of_reached(rider, 50)

    UNION ALL

    -- NAME: Nice Sweep
    -- DESCR: Collect rides with sixty-nine vehicles of the same company with consecutive numbers.
    -- ORDER: 5,7 consecutive numbers
    SELECT 20, bim.sequence_of_reached(rider, 69)

    UNION ALL

    -- NAME: Century Sweep
    -- DESCR: Collect rides with one hundred vehicles of the same company with consecutive numbers.
    -- ORDER: 5,8 consecutive numbers
    SELECT 21, bim.sequence_of_reached(rider, 100)

    UNION ALL

    -- NAME: Kinda Beastly
    -- DESCR: Ride a vehicle (of any company) whose number contains "666" (but isn't 666).
    -- ORDER: 1,2 special vehicle numbers
    SELECT 22, MIN(rav22."timestamp")
    FROM bim.rides_and_vehicles rav22
    WHERE rav22.rider_username = rider
    AND rav22.vehicle_number <> '666'
    AND POSITION('666' IN rav22.vehicle_number) > 0

    UNION ALL

    -- NAME: Rather Nice
    -- DESCR: Ride a vehicle (of any company) whose number contains "69" (but isn't 69).
    -- ORDER: 1,4 special vehicle numbers
    SELECT 23, MIN(rav23."timestamp")
    FROM bim.rides_and_vehicles rav23
    WHERE rav23.rider_username = rider
    AND rav23.vehicle_number <> '69'
    AND POSITION('69' IN rav23.vehicle_number) > 0

    UNION ALL

    -- NAME: Indivisibiliter
    -- DESCR: Ride a vehicle (of any company) whose vehicle number is divisible by (but not equal to) its line number.
    -- ORDER: 2,3 vehicle numbers in relation to line numbers
    SELECT 24, MIN(rav24."timestamp")
    FROM bim.rides_and_numeric_vehicles rav24
    WHERE rav24.rider_username = rider
    AND rav24.vehicle_number > bim.char_to_bigint_or_null(rav24.line)
    AND MOD(rav24.vehicle_number, bim.char_to_bigint_or_null(rav24.line)) = 0

    UNION ALL

    -- NAME: Inseparabiliter
    -- DESCR: Ride a vehicle (of any company) on a line whose number is divisible by (but not equal to) the vehicle's number.
    -- ORDER: 2,4 vehicle numbers in relation to line numbers
    SELECT 25, MIN(rav25."timestamp")
    FROM bim.rides_and_numeric_vehicles rav25
    WHERE rav25.rider_username = rider
    AND bim.char_to_bigint_or_null(rav25.line) > rav25.vehicle_number
    AND MOD(bim.char_to_bigint_or_null(rav25.line), rav25.vehicle_number) = 0

    UNION ALL

    -- NAME: Priming the Pump
    -- DESCR: Ride a vehicle (of any company) whose vehicle number is a four-digit prime.
    -- ORDER: 1,10 special vehicle numbers
    SELECT 26, MIN(rav26."timestamp")
    FROM bim.rides_and_numeric_vehicles rav26
    WHERE rav26.rider_username = rider
    AND rav26.vehicle_number BETWEEN 1000 AND 9999
    AND bim.is_prime(rav26.vehicle_number)

    UNION ALL

    -- NAME: Prim and Proper
    -- DESCR: Ride a vehicle (of any company) whose vehicle number is a three-digit prime.
    -- ORDER: 1,11 special vehicle numbers
    SELECT 27, MIN(rav27."timestamp")
    FROM bim.rides_and_numeric_vehicles rav27
    WHERE rav27.rider_username = rider
    AND rav27.vehicle_number BETWEEN 100 AND 999
    AND bim.is_prime(rav27.vehicle_number)

    UNION ALL

    -- NAME: Primate Representative
    -- DESCR: Ride a vehicle (of any company) whose vehicle number is a two-digit prime.
    -- ORDER: 1,12 special vehicle numbers
    SELECT 28, MIN(rav28."timestamp")
    FROM bim.rides_and_numeric_vehicles rav28
    WHERE rav28.rider_username = rider
    AND rav28.vehicle_number BETWEEN 10 AND 99
    AND bim.is_prime(rav28.vehicle_number)

    UNION ALL

    -- NAME: Primus Inter Pares
    -- DESCR: Ride a vehicle (of any company) whose vehicle number is a single-digit prime.
    -- ORDER: 1,13 special vehicle numbers
    SELECT 29, MIN(rav29."timestamp")
    FROM bim.rides_and_numeric_vehicles rav29
    WHERE rav29.rider_username = rider
    AND rav29.vehicle_number BETWEEN 1 AND 9
    AND bim.is_prime(rav29.vehicle_number)

    UNION ALL

    -- NAME: It Gets Better
    -- DESCR: Ride a vehicle (of any company) whose at least three-digit number's decimal digits are in ascending order.
    -- ORDER: 1,14 special vehicle numbers
    SELECT 30, MIN(rav30."timestamp")
    FROM bim.rides_and_numeric_vehicles rav30
    WHERE rav30.rider_username = rider
    AND rav30.vehicle_number > 99
    AND bim.is_in_sequence(rav30.vehicle_number, TRUE)

    UNION ALL

    -- NAME: Downward Spiral
    -- DESCR: Ride a vehicle (of any company) whose at least three-digit number's decimal digits are in descending order.
    -- ORDER: 1,15 special vehicle numbers
    SELECT 31, MIN(rav31."timestamp")
    FROM bim.rides_and_numeric_vehicles rav31
    WHERE rav31.rider_username = rider
    AND rav31.vehicle_number > 99
    AND bim.is_in_sequence(rav31.vehicle_number, FALSE)

    UNION ALL

    -- NAME: Take Five
    -- DESCR: Ride the same vehicle five times.
    -- ORDER: 4,4 same vehicle
    SELECT 32, MIN(rbrvt32."timestamp")
    FROM bim.ride_by_rider_vehicle_timestamp rbrvt32
    WHERE rbrvt32.rider_username = rider
    AND rbrvt32.nth = 5

    UNION ALL

    -- NAME: Both Hands
    -- DESCR: Ride the same vehicle ten times.
    -- ORDER: 4,5 same vehicle
    SELECT 33, MIN(rbrvt33."timestamp")
    FROM bim.ride_by_rider_vehicle_timestamp rbrvt33
    WHERE rbrvt33.rider_username = rider
    AND rbrvt33.nth = 10

    UNION ALL

    -- NAME: Fingers and Toes
    -- DESCR: Ride the same vehicle twenty times.
    -- ORDER: 4,6 same vehicle
    SELECT 34, MIN(rbrvt34."timestamp")
    FROM bim.ride_by_rider_vehicle_timestamp rbrvt34
    WHERE rbrvt34.rider_username = rider
    AND rbrvt34.nth = 20

    UNION ALL

    -- NAME: Flagrant Favoritism
    -- DESCR: Ride the same vehicle thirty times.
    -- ORDER: 4,7 same vehicle
    SELECT 35, MIN(rbrvt35."timestamp")
    FROM bim.ride_by_rider_vehicle_timestamp rbrvt35
    WHERE rbrvt35.rider_username = rider
    AND rbrvt35.nth = 30

    UNION ALL

    -- NAME: Habitual
    -- DESCR: Ride the same vehicle fifty times.
    -- ORDER: 4,8 same vehicle
    SELECT 36, MIN(rbrvt36."timestamp")
    FROM bim.ride_by_rider_vehicle_timestamp rbrvt36
    WHERE rbrvt36.rider_username = rider
    AND rbrvt36.nth = 50

    UNION ALL

    -- NAME: Familiarity Is Nice
    -- DESCR: Ride the same vehicle sixty-nine times.
    -- ORDER: 4,9 same vehicle
    SELECT 37, MIN(rbrvt37."timestamp")
    FROM bim.ride_by_rider_vehicle_timestamp rbrvt37
    WHERE rbrvt37.rider_username = rider
    AND rbrvt37.nth = 69

    UNION ALL

    -- NAME: Common-Law Marriage
    -- DESCR: Ride the same vehicle one hundred times.
    -- ORDER: 4,10 same vehicle
    SELECT 38, MIN(rbrvt38."timestamp")
    FROM bim.ride_by_rider_vehicle_timestamp rbrvt38
    WHERE rbrvt38.rider_username = rider
    AND rbrvt38.nth = 100

    UNION ALL

    -- NAME: Continual
    -- DESCR: Ride the same vehicle on the same line five times.
    -- ORDER: 2,5 vehicle numbers in relation to line numbers
    SELECT 39, MIN(rbrvlt39."timestamp")
    FROM bim.ride_by_rider_vehicle_line_timestamp rbrvlt39
    WHERE rbrvlt39.rider_username = rider
    AND rbrvlt39.nth = 5

    UNION ALL

    -- NAME: Repeated
    -- DESCR: Ride the same vehicle on the same line ten times.
    -- ORDER: 2,6 vehicle numbers in relation to line numbers
    SELECT 40, MIN(rbrvlt40."timestamp")
    FROM bim.ride_by_rider_vehicle_line_timestamp rbrvlt40
    WHERE rbrvlt40.rider_username = rider
    AND rbrvlt40.nth = 10

    UNION ALL

    -- NAME: Insistent
    -- DESCR: Ride the same vehicle on the same line twenty times.
    -- ORDER: 2,7 vehicle numbers in relation to line numbers
    SELECT 41, MIN(rbrvlt41."timestamp")
    FROM bim.ride_by_rider_vehicle_line_timestamp rbrvlt41
    WHERE rbrvlt41.rider_username = rider
    AND rbrvlt41.nth = 20

    UNION ALL

    -- NAME: Constant
    -- DESCR: Ride the same vehicle on the same line thirty times.
    -- ORDER: 2,8 vehicle numbers in relation to line numbers
    SELECT 42, MIN(rbrvlt42."timestamp")
    FROM bim.ride_by_rider_vehicle_line_timestamp rbrvlt42
    WHERE rbrvlt42.rider_username = rider
    AND rbrvlt42.nth = 30

    UNION ALL

    -- NAME: Dull
    -- DESCR: Ride the same vehicle on the same line fifty times.
    -- ORDER: 2,9 vehicle numbers in relation to line numbers
    SELECT 43, MIN(rbrvlt43."timestamp")
    FROM bim.ride_by_rider_vehicle_line_timestamp rbrvlt43
    WHERE rbrvlt43.rider_username = rider
    AND rbrvlt43.nth = 50

    UNION ALL

    -- NAME: Boring but Nice
    -- DESCR: Ride the same vehicle on the same line sixty-nine times.
    -- ORDER: 2,10 vehicle numbers in relation to line numbers
    SELECT 44, MIN(rbrvlt44."timestamp")
    FROM bim.ride_by_rider_vehicle_line_timestamp rbrvlt44
    WHERE rbrvlt44.rider_username = rider
    AND rbrvlt44.nth = 69

    UNION ALL

    -- NAME: Ceaseless
    -- DESCR: Ride the same vehicle on the same line one hundred times.
    -- ORDER: 2,11 vehicle numbers in relation to line numbers
    SELECT 45, MIN(rbrvlt45."timestamp")
    FROM bim.ride_by_rider_vehicle_line_timestamp rbrvlt45
    WHERE rbrvlt45.rider_username = rider
    AND rbrvlt45.nth = 100

    UNION ALL

    -- NAME: We Meet Again
    -- DESCR: Ride the same vehicle on two different lines.
    -- ORDER: 2,12 vehicle numbers in relation to line numbers
    SELECT 46, MIN(frbrvl46.min_timestamp)
    FROM bim.first_ride_by_rider_vehicle_line frbrvl46
    WHERE frbrvl46.rider_username = rider
    AND frbrvl46.nth = 2

    UNION ALL

    -- NAME: Explorer
    -- DESCR: Ride the same vehicle on three different lines.
    -- ORDER: 2,13 vehicle numbers in relation to line numbers
    SELECT 47, MIN(frbrvl47.min_timestamp)
    FROM bim.first_ride_by_rider_vehicle_line frbrvl47
    WHERE frbrvl47.rider_username = rider
    AND frbrvl47.nth = 3

    UNION ALL

    -- NAME: Seen the World
    -- DESCR: Ride the same vehicle on five different lines.
    -- ORDER: 2,14 vehicle numbers in relation to line numbers
    SELECT 48, MIN(frbrvl48.min_timestamp)
    FROM bim.first_ride_by_rider_vehicle_line frbrvl48
    WHERE frbrvl48.rider_username = rider
    AND frbrvl48.nth = 5

    UNION ALL

    -- NAME: Journeyman
    -- DESCR: Collect five rides in a day.
    -- ORDER: 6,1 over time
    SELECT 49, bim.rider_rides_per_timespan_reached(rider, 'P1D', 5)

    UNION ALL

    -- NAME: Hopper
    -- DESCR: Collect ten rides in a day.
    -- ORDER: 6,2 over time
    SELECT 50, bim.rider_rides_per_timespan_reached(rider, 'P1D', 10)

    UNION ALL

    -- NAME: Serial Tripper
    -- DESCR: Collect twenty rides in a day.
    -- ORDER: 6,3 over time
    SELECT 51, bim.rider_rides_per_timespan_reached(rider, 'P1D', 20)

    UNION ALL

    -- NAME: Single-Stop Vehicle Skipper
    -- DESCR: Collect thirty rides in a day.
    -- ORDER: 6,4 over time
    SELECT 52, bim.rider_rides_per_timespan_reached(rider, 'P1D', 30)

    UNION ALL

    -- NAME: Too Much Spare Time
    -- DESCR: Collect fifty rides in a day.
    -- ORDER: 6,5 over time
    SELECT 53, bim.rider_rides_per_timespan_reached(rider, 'P1D', 50)

    UNION ALL

    -- NAME: Commuter
    -- DESCR: Collect ten rides in a week.
    -- ORDER: 6,6 over time
    SELECT 54, bim.rider_rides_per_timespan_reached(rider, 'P7D', 10)

    UNION ALL

    -- NAME: Passenger
    -- DESCR: Collect twenty rides in a week.
    -- ORDER: 6,7 over time
    SELECT 55, bim.rider_rides_per_timespan_reached(rider, 'P7D', 20)

    UNION ALL

    -- NAME: Enthusiast
    -- DESCR: Collect thirty rides in a week.
    -- ORDER: 6,8 over time
    SELECT 56, bim.rider_rides_per_timespan_reached(rider, 'P7D', 30)

    UNION ALL

    -- NAME: Trainspotter
    -- DESCR: Collect fifty rides in a week.
    -- ORDER: 6,9 over time
    SELECT 57, bim.rider_rides_per_timespan_reached(rider, 'P7D', 50)

    UNION ALL

    -- NAME: Nice Rider
    -- DESCR: Collect sixty-nine rides in a week.
    -- ORDER: 6,10 over time
    SELECT 58, bim.rider_rides_per_timespan_reached(rider, 'P7D', 69)

    UNION ALL

    -- NAME: Trainstopper
    -- DESCR: Collect one hundred rides in a week.
    -- ORDER: 6,11 over time
    SELECT 59, bim.rider_rides_per_timespan_reached(rider, 'P7D', 100)

    UNION ALL

    -- NAME: Two Pow Seven
    -- DESCR: Collect one hundred and twenty-eight rides in a week.
    -- ORDER: 6,12 over time
    SELECT 60, bim.rider_rides_per_timespan_reached(rider, 'P7D', 128)

    UNION ALL

    -- NAME: Pokedex
    -- DESCR: Collect one hundred and fifty-one rides in a week.
    -- ORDER: 6,13 over time
    SELECT 61, bim.rider_rides_per_timespan_reached(rider, 'P7D', 151)

    UNION ALL

    -- NAME: Consistency
    -- DESCR: Collect one hundred rides in thirty days.
    -- ORDER: 6,14 over time
    SELECT 62, bim.rider_rides_per_timespan_reached(rider, 'P30D', 100)

    UNION ALL

    -- NAME: Perseverance
    -- DESCR: Collect two hundred rides in thirty days.
    -- ORDER: 6,15 over time
    SELECT 63, bim.rider_rides_per_timespan_reached(rider, 'P30D', 200)

    UNION ALL

    -- NAME: Frequent Flyer
    -- DESCR: Collect three hundred rides in thirty days.
    -- ORDER: 6,16 over time
    SELECT 64, bim.rider_rides_per_timespan_reached(rider, 'P30D', 300)

    UNION ALL

    -- NAME: No House Required
    -- DESCR: Collect five hundred rides in thirty days.
    -- ORDER: 6,17 over time
    SELECT 65, bim.rider_rides_per_timespan_reached(rider, 'P30D', 500)

    UNION ALL

    -- NAME: Boomer
    -- DESCR: Collect two hundred rides in 365 days.
    -- ORDER: 6,18 over time
    SELECT 66, bim.rider_rides_per_timespan_reached(rider, 'P365D', 200)

    UNION ALL

    -- NAME: GenX
    -- DESCR: Collect five hundred rides in 365 days.
    -- ORDER: 6,19 over time
    SELECT 67, bim.rider_rides_per_timespan_reached(rider, 'P365D', 500)

    UNION ALL

    -- NAME: Millennial
    -- DESCR: Collect one thousand rides in 365 days.
    -- ORDER: 6,20 over time
    SELECT 68, bim.rider_rides_per_timespan_reached(rider, 'P365D', 1000)

    UNION ALL

    -- NAME: GenZ
    -- DESCR: Collect 1500 rides in 365 days.
    -- ORDER: 6,21 over time
    SELECT 69, bim.rider_rides_per_timespan_reached(rider, 'P365D', 1500)

    UNION ALL

    -- NAME: GenAlpha
    -- DESCR: Collect 2000 rides in 365 days.
    -- ORDER: 6,22 over time
    SELECT 70, bim.rider_rides_per_timespan_reached(rider, 'P365D', 2000)

    UNION ALL

    -- NAME: First Post
    -- DESCR: Be the first to ride a vehicle.
    -- ORDER: 7,1 global firsts
    SELECT 71, MIN(fvr71."timestamp")
    FROM
        bim.first_vehicle_rides fvr71
    WHERE
        fvr71.rider_username = rider
        AND fvr71.nth_first = 1

    UNION ALL

    -- NAME: First Time's the Charm
    -- DESCR: Be the first rider in ten different vehicles.
    -- ORDER: 7,2 global firsts
    SELECT 72, MIN(fvr72."timestamp")
    FROM
        bim.first_vehicle_rides fvr72
    WHERE
        fvr72.rider_username = rider
        AND fvr72.nth_first = 10

    UNION ALL

    -- NAME: First Come, First Served
    -- DESCR: Be the first rider in twenty different vehicles.
    -- ORDER: 7,3 global firsts
    SELECT 73, MIN(fvr73."timestamp")
    FROM
        bim.first_vehicle_rides fvr73
    WHERE
        fvr73.rider_username = rider
        AND fvr73.nth_first = 20

    UNION ALL

    -- NAME: First Things First
    -- DESCR: Be the first rider in fifty different vehicles.
    -- ORDER: 7,4 global firsts
    SELECT 74, MIN(fvr74."timestamp")
    FROM
        bim.first_vehicle_rides fvr74
    WHERE
        fvr74.rider_username = rider
        AND fvr74.nth_first = 50

    UNION ALL

    -- NAME: Nice Impressions Are the Most Lasting
    -- DESCR: Be the first rider in sixty-nine different vehicles.
    -- ORDER: 7,5 global firsts
    SELECT 75, MIN(fvr75."timestamp")
    FROM
        bim.first_vehicle_rides fvr75
    WHERE
        fvr75.rider_username = rider
        AND fvr75.nth_first = 69

    UNION ALL

    -- NAME: German Word for a Roof Ridge
    -- DESCR: Be the first rider in one hundred different vehicles.
    -- ORDER: 7,6 global firsts
    SELECT 76, MIN(fvr76."timestamp")
    FROM
        bim.first_vehicle_rides fvr76
    WHERE
        fvr76.rider_username = rider
        AND fvr76.nth_first = 100

    UNION ALL

    -- NAME: "Prince" Is Actually Spelled With an Ü
    -- DESCR: Be the first rider in one hundred and fifty different vehicles.
    -- ORDER: 7,7 global firsts
    SELECT 77, MIN(fvr77."timestamp")
    FROM
        bim.first_vehicle_rides fvr77
    WHERE
        fvr77.rider_username = rider
        AND fvr77.nth_first = 150

    UNION ALL

    -- NAME: "Deadline" Is a Common Misspelling
    -- DESCR: Be the first rider in two hundred different vehicles.
    -- ORDER: 7,8 global firsts
    SELECT 78, MIN(fvr78."timestamp")
    FROM
        bim.first_vehicle_rides fvr78
    WHERE
        fvr78.rider_username = rider
        AND fvr78.nth_first = 200

    UNION ALL

    -- NAME: First Cut Is the Deepest
    -- DESCR: Be the first rider in two hundred and fifty different vehicles.
    -- ORDER: 7,9 global firsts
    SELECT 79, MIN(fvr79."timestamp")
    FROM
        bim.first_vehicle_rides fvr79
    WHERE
        fvr79.rider_username = rider
        AND fvr79.nth_first = 250

    UNION ALL

    -- NAME: Hungry and Firsty
    -- DESCR: Be the first rider in three hundred different vehicles.
    -- ORDER: 7,10 global firsts
    SELECT 80, MIN(fvr80."timestamp")
    FROM
        bim.first_vehicle_rides fvr80
    WHERE
        fvr80.rider_username = rider
        AND fvr80.nth_first = 300

    UNION ALL

    -- NAME: Firsticuffs
    -- DESCR: Be the first rider in four hundred different vehicles.
    -- ORDER: 7,11 global firsts
    SELECT 81, MIN(fvr81."timestamp")
    FROM
        bim.first_vehicle_rides fvr81
    WHERE
        fvr81.rider_username = rider
        AND fvr81.nth_first = 400

    UNION ALL

    -- NAME: Abies-st
    -- DESCR: Be the first rider in five hundred different vehicles.
    -- ORDER: 7,12 global firsts
    SELECT 82, MIN(fvr82."timestamp")
    FROM
        bim.first_vehicle_rides fvr82
    WHERE
        fvr82.rider_username = rider
        AND fvr82.nth_first = 500

    UNION ALL

    -- NAME: Affirstmation
    -- DESCR: Be the first rider in six hundred different vehicles.
    -- ORDER: 7,13 global firsts
    SELECT 83, MIN(fvr83."timestamp")
    FROM
        bim.first_vehicle_rides fvr83
    WHERE
        fvr83.rider_username = rider
        AND fvr83.nth_first = 600

    UNION ALL

    -- NAME: Firstmness
    -- DESCR: Be the first rider in seven hundred different vehicles.
    -- ORDER: 7,14 global firsts
    SELECT 84, MIN(fvr84."timestamp")
    FROM
        bim.first_vehicle_rides fvr84
    WHERE
        fvr84.rider_username = rider
        AND fvr84.nth_first = 700

    UNION ALL

    -- NAME: Yesterday You Said Tomorrow
    -- DESCR: Ride the same vehicle two days in a row.
    -- ORDER: 8,1 same vehicle days in a row
    SELECT 85, bim.same_vehicle_consecutive_days_reached(rider, 2)

    UNION ALL

    -- NAME: Long Weekend Together
    -- DESCR: Ride the same vehicle three days in a row.
    -- ORDER: 8,2 same vehicle days in a row
    SELECT 86, bim.same_vehicle_consecutive_days_reached(rider, 3)

    UNION ALL

    -- NAME: Going Steady
    -- DESCR: Ride the same vehicle four days in a row.
    -- ORDER: 8,3 same vehicle days in a row
    SELECT 87, bim.same_vehicle_consecutive_days_reached(rider, 4)

    UNION ALL

    -- NAME: Dating a Co-Worker
    -- DESCR: Ride the same vehicle five days in a row.
    -- ORDER: 8,5 same vehicle days in a row
    SELECT 88, bim.same_vehicle_consecutive_days_reached(rider, 5)

    UNION ALL

    -- NAME: I Don't Roll on Shabbos
    -- DESCR: Ride the same vehicle six days in a row.
    -- ORDER: 8,6 same vehicle days in a row
    SELECT 89, bim.same_vehicle_consecutive_days_reached(rider, 6)

    UNION ALL

    -- NAME: You Make My Knees Week
    -- DESCR: Ride the same vehicle seven days in a row.
    -- ORDER: 8,7 same vehicle days in a row
    SELECT 90, bim.same_vehicle_consecutive_days_reached(rider, 7)

    UNION ALL

    -- NAME: Palindromic
    -- DESCR: Ride two unique vehicles (of any company) whose numbers are a palindrome while not being all the same digit.
    -- ORDER: 9,2 palindromes
    SELECT 91, min_timestamp
    FROM bim.rider_first_palindrome_vehicle_ride rfpvr91
    WHERE rfpvr91.rider_username = rider
    AND rfpvr91.nth = 2

    UNION ALL

    -- NAME: Palindrome's the Charm
    -- DESCR: Ride three unique vehicles (of any company) whose numbers are a palindrome while not being all the same digit.
    -- ORDER: 9,3 palindromes
    SELECT 92, min_timestamp
    FROM bim.rider_first_palindrome_vehicle_ride rfpvr92
    WHERE rfpvr92.rider_username = rider
    AND rfpvr92.nth = 3

    UNION ALL

    -- NAME: Pentalindrome
    -- DESCR: Ride five unique vehicles (of any company) whose numbers are a palindrome while not being all the same digit.
    -- ORDER: 9,5 palindromes
    SELECT 93, min_timestamp
    FROM bim.rider_first_palindrome_vehicle_ride rfpvr93
    WHERE rfpvr93.rider_username = rider
    AND rfpvr93.nth = 5

    UNION ALL

    -- NAME: Decalindrome
    -- DESCR: Ride ten unique vehicles (of any company) whose numbers are a palindrome while not being all the same digit.
    -- ORDER: 9,6 palindromes
    SELECT 94, min_timestamp
    FROM bim.rider_first_palindrome_vehicle_ride rfpvr94
    WHERE rfpvr94.rider_username = rider
    AND rfpvr94.nth = 10

    UNION ALL

    -- NAME: Two by Two
    -- DESCR: Ride two unique vehicles (of any company) whose numbers consist of one digit repeated at least two times.
    -- ORDER: 10,4 repdigits
    SELECT 94, bim.repdigit_unique_vehicle_ride(rider, 2, 2)

    UNION ALL

    -- NAME: Two by Three
    -- DESCR: Ride two unique vehicles (of any company) whose numbers consist of one digit repeated at least three times.
    -- ORDER: 10,5 repdigits
    SELECT 95, bim.repdigit_unique_vehicle_ride(rider, 3, 2)

    UNION ALL

    -- NAME: Three by Two
    -- DESCR: Ride three unique vehicles (of any company) whose numbers consist of one digit repeated at least two times.
    -- ORDER: 10,6 repdigits
    SELECT 96, bim.repdigit_unique_vehicle_ride(rider, 2, 3)

    UNION ALL

    -- NAME: Every Day Is Exactly the Same
    -- DESCR: Ride the same vehicle on the same line at the same minute two days in a row.
    -- ORDER: 4,11 same vehicle
    SELECT 97, MIN(srtmia97."timestamp")
    FROM bim.same_ride_to_minute_interval_ago(interval '1 day') srtmia97
    WHERE srtmia97.rider_username = rider

    UNION ALL

    -- NAME: Every Week Is Exactly the Same
    -- DESCR: Ride the same vehicle on the same line at the same minute two weeks in a row.
    -- ORDER: 4,12 same vehicle
    SELECT 98, MIN(srtmia98."timestamp")
    FROM bim.same_ride_to_minute_interval_ago(interval '7 days') srtmia98
    WHERE srtmia98.rider_username = rider

    UNION ALL

    -- NAME: Donald Duck's Car
    -- DESCR: Ride a vehicle (of any company) with the number 313.
    -- ORDER: 1,16 special vehicle numbers
    SELECT 99, MIN(rav99."timestamp")
    FROM bim.rides_and_numeric_vehicles rav99
    WHERE rav99.rider_username = rider
    AND rav99.vehicle_number = 313

    UNION ALL

    -- NAME: I Keep Going There
    -- DESCR: Ride on the same line one hundred times.
    -- ORDER: 11,1 same line
    SELECT 100, MIN(rbrlt100."timestamp")
    FROM bim.ride_by_rider_line_timestamp rbrlt100
    WHERE rbrlt100.rider_username = rider
    AND rbrlt100.nth = 100

    UNION ALL

    -- NAME: I Can't Stay Away
    -- DESCR: Ride on the same line two hundred times.
    -- ORDER: 11,2 same line
    SELECT 101, MIN(rbrlt101."timestamp")
    FROM bim.ride_by_rider_line_timestamp rbrlt101
    WHERE rbrlt101.rider_username = rider
    AND rbrlt101.nth = 200

    UNION ALL

    -- NAME: It Gives Me Comfort
    -- DESCR: Ride on the same line five hundred times.
    -- ORDER: 11,3 same line
    SELECT 102, MIN(rbrlt102."timestamp")
    FROM bim.ride_by_rider_line_timestamp rbrlt102
    WHERE rbrlt102.rider_username = rider
    AND rbrlt102.nth = 500

    UNION ALL

    -- NAME: I Must
    -- DESCR: Ride on the same line one thousand times.
    -- ORDER: 11,4 same line
    SELECT 103, MIN(rbrlt103."timestamp")
    FROM bim.ride_by_rider_line_timestamp rbrlt103
    WHERE rbrlt103.rider_username = rider
    AND rbrlt103.nth = 1000

    UNION ALL

    -- NAME: Tyr? Thor? Tripe
    -- DESCR: Ride the same vehicle on the same line on a Tuesday and a Thursday of the same week.
    -- ORDER: 2,15 vehicle numbers in relation to line numbers
    SELECT 104, MIN(rav104."timestamp")
    FROM bim.rides_and_vehicles rav104
    WHERE rav104.rider_username = rider
    AND EXTRACT(DOW FROM rav104."timestamp") = 4
    AND EXISTS (
        SELECT 1
        FROM bim.rides_and_vehicles rav104b
        WHERE rav104b.rider_username = rav104.rider_username
        AND rav104b.company = rav104.company
        AND rav104b.vehicle_number = rav104.vehicle_number
        AND rav104b.line = rav104.line
        AND EXTRACT(YEAR FROM (rav104."timestamp" - INTERVAL 'P2D')) = EXTRACT(YEAR FROM rav104b."timestamp")
        AND EXTRACT(MONTH FROM (rav104."timestamp" - INTERVAL 'P2D')) = EXTRACT(MONTH FROM rav104b."timestamp")
        AND EXTRACT(DAY FROM (rav104."timestamp" - INTERVAL 'P2D')) = EXTRACT(DAY FROM rav104b."timestamp")
    )

    UNION ALL

    -- NAME: It's Not Even Thursday
    -- DESCR: "????" —Steve
    -- ORDER: 99,1 troll achievements
    SELECT 105, MIN(ntt105."timestamp")
    FROM bim.not_thursday_timestamp ntt105
    WHERE ntt105.rider_username = rider

    UNION ALL

    -- NAME: Capture the Flag
    -- DESCR: Be the last rider in at least 100 vehicles at the same time.
    -- ORDER: 12,1 last rider
    SELECT 106, MIN(lr."timestamp")
    FROM bim.last_rides lr
    WHERE lr.rider_username = rider
    AND lr.vehicle_count >= 100

    UNION ALL

    -- NAME: Always the Last One
    -- DESCR: Be the last rider in at least 500 vehicles at the same time.
    -- ORDER: 12,2 last rider
    SELECT 107, MIN(lr."timestamp")
    FROM bim.last_rides lr
    WHERE lr.rider_username = rider
    AND lr.vehicle_count >= 500

    UNION ALL

    -- NAME: World Domination
    -- DESCR: Be the last rider in at least 1000 vehicles at the same time.
    -- ORDER: 12,3 last rider
    SELECT 108, MIN(lr."timestamp")
    FROM bim.last_rides lr
    WHERE lr.rider_username = rider
    AND lr.vehicle_count >= 1000
$$;

CREATE MATERIALIZED VIEW bim.rider_achievements AS
    WITH all_riders(rider_username) AS (
        SELECT DISTINCT r.rider_username
        FROM bim.rides r
    )
    SELECT ar.rider_username, ach.achievement_id, ach.achieved_on
    FROM
        all_riders ar
        CROSS JOIN LATERAL bim.achievements_of(ar.rider_username) ach
    ORDER BY
        ar.rider_username, ach.achievement_id
    WITH DATA
;
