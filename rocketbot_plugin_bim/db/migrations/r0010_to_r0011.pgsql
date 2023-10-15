CREATE OR REPLACE FUNCTION bim.ridden_vehicles_taken_over
() RETURNS TABLE
( id bigint
, company character varying(256)
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
    company_to_vehicle_to_last_rider jsonb;
    vehicle_to_last_rider jsonb;
BEGIN
    company_to_vehicle_to_last_rider := JSONB_BUILD_OBJECT();
    FOR rv IN SELECT rav.id, rav.company, rav.vehicle_number, rav."timestamp", rav.rider_username FROM bim.rides_and_vehicles rav WHERE rav.coupling_mode = 'R' ORDER BY rav."timestamp", rav.id
    LOOP
        id := rv.id;
        company := rv.company;
        vehicle_number := rv.vehicle_number;
        timestamp := rv."timestamp";

        vehicle_to_last_rider := company_to_vehicle_to_last_rider -> company;
        IF vehicle_to_last_rider IS NULL
        THEN
            vehicle_to_last_rider := JSONB_BUILD_OBJECT();
        END IF;
        old_rider := vehicle_to_last_rider ->> vehicle_number;
        new_rider := rv.rider_username;

        IF old_rider = new_rider
        THEN
            CONTINUE;
        END IF;

        vehicle_to_last_rider := vehicle_to_last_rider || JSONB_BUILD_OBJECT(vehicle_number, new_rider);
        company_to_vehicle_to_last_rider := company_to_vehicle_to_last_rider || JSONB_BUILD_OBJECT(company, vehicle_to_last_rider);
        RETURN NEXT;
    END LOOP;
END;
$$;

UPDATE bim.schema_revision SET sch_rev=11;
