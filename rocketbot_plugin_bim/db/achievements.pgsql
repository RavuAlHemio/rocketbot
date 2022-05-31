CREATE OR REPLACE FUNCTION bim.reverse_bigint
( val bigint
) RETURNS bigint
LANGUAGE plpgsql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    DECLARE
        result bigint NOT NULL := 0;
        last_digit bigint NOT NULL := 0;
        negate boolean NOT NULL := FALSE;
    BEGIN
        IF val IS NULL THEN
            RETURN NULL;
        END IF;
        IF val < 0 THEN
            val := -val;
            negate := TRUE;
        END IF;
        WHILE val > 0 LOOP
            -- pick off last digit
            last_digit := MOD(val, 10);
            val := DIV(val, 10);
            result := result * 10;
            result := result + last_digit;
        END LOOP;
        IF negate THEN
            result := -result;
        END IF;
        RETURN result;
    END;
$$;

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

CREATE OR REPLACE FUNCTION bim.same_digits
( val bigint
, min_length bigint
) RETURNS boolean
LANGUAGE plpgsql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    DECLARE
        val_str character varying;
        digit character;
    BEGIN
        IF val IS NULL OR min_length IS NULL THEN
            RETURN NULL;
        END IF;
        val_str := CAST(val AS character varying);
        IF LENGTH(val_str) < min_length THEN
            RETURN FALSE;
        END IF;
        IF LENGTH(val_str) < 2 THEN
            RETURN TRUE;
        END IF;

        digit := SUBSTRING(val_str FROM 1 FOR 1);
        FOR i IN 2..LENGTH(val_str) LOOP
            IF SUBSTRING(val_str FROM i FOR 1) <> digit THEN
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
    SELECT bim.smallest_factor(val) = val
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
                FROM bim.rides_and_vehicles
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
                FROM bim.rides_and_vehicles
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

CREATE OR REPLACE FUNCTION bim.achievements_of
( rider character varying(256)
) RETURNS TABLE
( achievement_id bigint
, achieved_on timestamp with time zone
)
LANGUAGE sql
AS $$
    -- Beastly
    -- Ride a vehicle (of any company) with number 666.
    SELECT 1, MIN("timestamp")
    FROM bim.rides_and_vehicles rav1
    WHERE rav1.rider_username = rider
    AND rav1.vehicle_number = 666

    UNION ALL

    -- Nice
    -- Ride a vehicle (of any company) with number 69.
    SELECT 2, MIN("timestamp")
    FROM bim.rides_and_vehicles rav2
    WHERE rav2.rider_username = rider
    AND rav2.vehicle_number = 69

    UNION ALL

    -- Home Line
    -- Ride a vehicle (of any company) where the vehicle number and the line are the same.
    SELECT 3, MIN("timestamp")
    FROM bim.rides_and_vehicles rav3
    WHERE rav3.rider_username = rider
    AND rav3.line = CAST(rav3.vehicle_number AS character varying)

    UNION ALL

    -- Two of a Kind
    -- Ride a vehicle (of any company) whose number consists of one digit repeated at least twice.
    SELECT 4, MIN("timestamp")
    FROM bim.rides_and_vehicles rav4
    WHERE rav4.rider_username = rider
    AND bim.same_digits(rav4.vehicle_number, 2)

    UNION ALL

    -- Three of a Kind
    -- Ride a vehicle (of any company) whose number consists of one digit repeated at least three times.
    SELECT 5, MIN("timestamp")
    FROM bim.rides_and_vehicles rav5
    WHERE rav5.rider_username = rider
    AND bim.same_digits(rav5.vehicle_number, 3)

    UNION ALL

    -- Four of a Kind
    -- Ride a vehicle (of any company) whose number consists of one digit repeated at least four times.
    SELECT 6, MIN("timestamp")
    FROM bim.rides_and_vehicles rav6
    WHERE rav6.rider_username = rider
    AND bim.same_digits(rav6.vehicle_number, 4)

    UNION ALL

    -- Palindrome
    -- Ride a vehicle (of any company) whose number is a palindrome while not being all the same digit.
    SELECT 7, MIN("timestamp")
    FROM bim.rides_and_vehicles rav7
    WHERE rav7.rider_username = rider
    AND rav7.vehicle_number > 99
    AND rav7.vehicle_number = bim.reverse_bigint(rav7.vehicle_number)
    AND NOT bim.same_digits(rav7.vehicle_number, 0)

    UNION ALL

    -- Mirror Home Line
    -- Ride a vehicle (of any company) where the vehicle number is the reverse of the line.
    SELECT 8, MIN("timestamp")
    FROM bim.rides_and_vehicles rav8
    WHERE rav8.rider_username = rider
    AND REVERSE(rav8.line) = CAST(rav8.vehicle_number AS character varying)

    UNION ALL

    -- Boeing
    -- Ride a vehicle (of any company) whose number has the pattern "7x7".
    SELECT 9, MIN("timestamp")
    FROM bim.rides_and_vehicles rav9
    WHERE rav9.rider_username = rider
    AND rav9.vehicle_number BETWEEN 707 AND 797
    AND MOD(rav9.vehicle_number, 10) = 7

    UNION ALL

    -- Elsewhere
    -- Ride two vehicles with the same vehicle number but different companies.
    SELECT 10, MIN(rav10b."timestamp")
    FROM bim.rides_and_vehicles rav10a
    INNER JOIN bim.rides_and_vehicles rav10b
        ON rav10b.vehicle_number = rav10a.vehicle_number
        AND rav10b.rider_username = rav10a.rider_username
        AND rav10b.company <> rav10a.company
        AND rav10b."timestamp" > rav10a."timestamp"
    WHERE rav10b.rider_username = rider

    UNION ALL

    -- Monthiversary
    -- Ride the same vehicle on the same day of two consecutive months.
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

    -- Anniversary
    -- Ride the same vehicle on the same day of two consecutive years.
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

    -- Same Time Next Week
    -- Ride the same vehicle on the same weekday of two consecutive weeks.
    SELECT 13, MIN(rav13b."timestamp")
    FROM bim.rides_and_vehicles rav13a
    INNER JOIN bim.rides_and_vehicles rav13b
        ON rav13b.rider_username = rav13a.rider_username
        AND rav13b.vehicle_number = rav13a.vehicle_number
        AND rav13b.company = rav13a.company
        AND CAST(rav13b."timestamp" AS date) = CAST((rav13a."timestamp" + CAST('P7D' AS interval)) AS date)
    WHERE rav13a.rider_username = rider

    UNION ALL

    -- Five Sweep
    -- Collect rides with five vehicles of the same company with consecutive numbers.
    SELECT 14, bim.sequence_of_reached(rider, 5)

    UNION ALL

    -- Ten Sweep
    -- Collect rides with ten vehicles of the same company with consecutive numbers.
    SELECT 15, bim.sequence_of_reached(rider, 10)

    UNION ALL

    -- Twenty Sweep
    -- Collect rides with twenty vehicles of the same company with consecutive numbers.
    SELECT 16, bim.sequence_of_reached(rider, 20)

    UNION ALL

    -- Thirty Sweep
    -- Collect rides with thirty vehicles of the same company with consecutive numbers.
    SELECT 17, bim.sequence_of_reached(rider, 30)

    UNION ALL

    -- Forty Sweep
    -- Collect rides with forty vehicles of the same company with consecutive numbers.
    SELECT 18, bim.sequence_of_reached(rider, 40)

    UNION ALL

    -- Half-Century Sweep
    -- Collect rides with fifty vehicles of the same company with consecutive numbers.
    SELECT 19, bim.sequence_of_reached(rider, 50)

    UNION ALL

    -- Nice Sweep
    -- Collect rides with sixty-nine vehicles of the same company with consecutive numbers.
    SELECT 20, bim.sequence_of_reached(rider, 69)

    UNION ALL

    -- Century Sweep
    -- Collect rides with one hundred vehicles of the same company with consecutive numbers.
    SELECT 21, bim.sequence_of_reached(rider, 100)

    UNION ALL

    -- Kinda Beastly
    -- Ride a vehicle (of any company) whose number contains "666" (but isn't 666).
    SELECT 22, MIN(rav22."timestamp")
    FROM bim.rides_and_vehicles rav22
    WHERE rav22.rider_username = rider
    AND rav22.vehicle_number <> 666
    AND POSITION('666' IN CAST(rav22.vehicle_number AS character varying)) > 0

    UNION ALL

    -- Rather Nice
    -- Ride a vehicle (of any company) whose number contains "69" (but isn't 69).
    SELECT 23, MIN(rav23."timestamp")
    FROM bim.rides_and_vehicles rav23
    WHERE rav23.rider_username = rider
    AND rav23.vehicle_number <> 69
    AND POSITION('69' IN CAST(rav23.vehicle_number AS character varying)) > 0

    UNION ALL

    -- Indivisibiliter
    -- Ride a vehicle (of any company) whose vehicle number is divisible by (but not equal to) its line number.
    SELECT 24, MIN(rav24."timestamp")
    FROM bim.rides_and_vehicles rav24
    WHERE rav24.rider_username = rider
    AND rav24.vehicle_number > bim.char_to_bigint_or_null(rav24.line)
    AND MOD(rav24.vehicle_number, bim.char_to_bigint_or_null(rav24.line)) = 0

    UNION ALL

    -- Inseparabiliter
    -- Ride a vehicle (of any company) on a line whose number is divisible by (but not equal to) the vehicle's number.
    SELECT 25, MIN(rav25."timestamp")
    FROM bim.rides_and_vehicles rav25
    WHERE rav25.rider_username = rider
    AND bim.char_to_bigint_or_null(rav25.line) > rav25.vehicle_number
    AND MOD(bim.char_to_bigint_or_null(rav25.line), rav25.vehicle_number) = 0

    UNION ALL

    -- Priming the Pump
    -- Ride a vehicle (of any company) whose vehicle number is a four-digit prime.
    SELECT 26, MIN(rav26."timestamp")
    FROM bim.rides_and_vehicles rav26
    WHERE rav26.rider_username = rider
    AND rav26.vehicle_number BETWEEN 1000 AND 9999
    AND bim.is_prime(rav26.vehicle_number)

    UNION ALL

    -- Prim and Proper
    -- Ride a vehicle (of any company) whose vehicle number is a three-digit prime.
    SELECT 27, MIN(rav27."timestamp")
    FROM bim.rides_and_vehicles rav27
    WHERE rav27.rider_username = rider
    AND rav27.vehicle_number BETWEEN 100 AND 999
    AND bim.is_prime(rav27.vehicle_number)

    UNION ALL

    -- Primate Representative
    -- Ride a vehicle (of any company) whose vehicle number is a two-digit prime.
    SELECT 28, MIN(rav28."timestamp")
    FROM bim.rides_and_vehicles rav28
    WHERE rav28.rider_username = rider
    AND rav28.vehicle_number BETWEEN 10 AND 99
    AND bim.is_prime(rav28.vehicle_number)

    UNION ALL

    -- Primus Inter Pares
    -- Ride a vehicle (of any company) whose vehicle number is a single-digit prime.
    SELECT 29, MIN(rav29."timestamp")
    FROM bim.rides_and_vehicles rav29
    WHERE rav29.rider_username = rider
    AND rav29.vehicle_number BETWEEN 1 AND 9
    AND bim.is_prime(rav29.vehicle_number)
$$;
