CREATE EXTENSION IF NOT EXISTS plpython3u;

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

UPDATE bim.schema_revision SET sch_rev=16;
