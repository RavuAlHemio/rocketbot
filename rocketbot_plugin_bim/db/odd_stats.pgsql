CREATE OR REPLACE FUNCTION bim.ride_chains
( within_time interval
) RETURNS TABLE
( rider_username character varying(256)
, company character varying(256)
, vehicle_number character varying(256)
, earliest_timestamp timestamp with time zone
, rides bigint[]
)
LANGUAGE plpgsql
STABLE STRICT
AS $$
    DECLARE
        visited_rides jsonb;
        r_uname character varying;
        r_company character varying;
        r_vehnum character varying;
        r_timestamp timestamp with time zone;
        r_next_timestamp timestamp with time zone;
        r_id bigint;
        r_id_char character varying;
        run_again boolean;
    BEGIN
        visited_rides := JSONB_BUILD_OBJECT();

        -- for each ride
        FOR r_uname, r_company, r_vehnum, r_id, r_timestamp IN
            SELECT rarv.rider_username, rarv.company, rarv.vehicle_number, rarv.id, rarv."timestamp"
            FROM bim.rides_and_ridden_vehicles rarv
            WHERE EXISTS (
                SELECT 1
                FROM bim.rides_and_ridden_vehicles rarv2
                WHERE rarv2.rider_username = rarv.rider_username
                AND rarv2.company = rarv.company
                AND rarv2.vehicle_number = rarv.vehicle_number
                AND rarv2."timestamp" > rarv."timestamp"
                AND rarv2."timestamp" < rarv."timestamp" + within_time
            )
            ORDER BY rarv."timestamp"
        LOOP
            r_id_char := CAST(r_id AS character varying);
            CONTINUE WHEN visited_rides ? r_id_char;

            -- assemble the chain
            rides := ARRAY[r_id];
            rider_username := r_uname;
            company := r_company;
            vehicle_number := r_vehnum;
            earliest_timestamp := r_timestamp;

            LOOP
                run_again := FALSE;
                FOR r_id, r_next_timestamp IN
                    SELECT rarv3.id, rarv3."timestamp"
                    FROM bim.rides_and_ridden_vehicles rarv3
                    WHERE rarv3.rider_username = r_uname
                    AND rarv3.company = r_company
                    AND rarv3.vehicle_number = r_vehnum
                    AND rarv3."timestamp" > r_timestamp
                    AND rarv3."timestamp" < r_timestamp + within_time
                    ORDER BY rarv3."timestamp"
                    LIMIT 1
                LOOP
                    r_id_char := CAST(r_id AS character varying);
                    r_timestamp := r_next_timestamp;
                    rides := rides || r_id;
                    visited_rides := visited_rides || JSONB_BUILD_OBJECT(r_id_char, TRUE);
                    run_again := TRUE;
                END LOOP;
                EXIT WHEN NOT run_again;
            END LOOP;

            -- output the longest chain we found
            RETURN NEXT;
        END LOOP;
    END;
$$;
