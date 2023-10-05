CREATE OR REPLACE FUNCTION bim.ridden_vehicles_taken_over
() RETURNS TABLE
( company character varying(256)
, vehicle_number character varying(256)
, "timestamp" timestamp with time zone
, old_rider character varying(256)
, new_rider character varying(256)
)
LANGUAGE plpgsql
STABLE STRICT
AS $$
DECLARE
    rv record;
    company_and_vehicle_to_last_rider jsonb;
    company_and_vehicle character varying;
BEGIN
    company_and_vehicle_to_last_rider := jsonb_build_object();
    FOR rv IN SELECT * FROM bim.rides_and_vehicles WHERE coupling_mode = 'R' ORDER BY "timestamp", id
    LOOP
        company := rv.company;
        vehicle_number := rv.vehicle_number;
        timestamp := rv."timestamp";
        company_and_vehicle := rv.company || '/' || rv.vehicle_number;
        old_rider := company_and_vehicle_to_last_rider ->> company_and_vehicle;
        new_rider := rv.rider_username;
        IF old_rider = new_rider
        THEN
            CONTINUE;
        END IF;
        company_and_vehicle_to_last_rider := company_and_vehicle_to_last_rider || jsonb_build_object(company_and_vehicle, new_rider);
        RETURN NEXT;
    END LOOP;
END;
$$;

UPDATE bim.schema_revision SET sch_rev=9;
