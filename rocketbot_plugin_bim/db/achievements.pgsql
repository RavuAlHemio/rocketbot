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
    AND rav3.line = CAST(rav3.vehicle_number AS varchar)

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
    AND REVERSE(rav8.line) = CAST(rav8.vehicle_number AS varchar)

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
        AND rav10b.id > rav10a.id
    WHERE rav10b.rider_username = rider
$$;
