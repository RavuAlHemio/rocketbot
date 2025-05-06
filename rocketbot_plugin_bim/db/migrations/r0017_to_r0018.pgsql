DROP FUNCTION bim.current_monopolies
(
);

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
company_to_vehicle_to_set = {}
company_to_vehicle_to_last_rider = {}
for row in plpy.cursor('SELECT id, "timestamp", company, rider_username, vehicles FROM bim.rides_vehicle_arrays_ridden_fixed ORDER BY "timestamp", id'):
    company = row["company"]
    rider_username = row["rider_username"]
    vehicles = json.loads(row["vehicles"])

    try:
        vehicle_to_set = company_to_vehicle_to_set[company]
    except KeyError:
        vehicle_to_set = {}
        company_to_vehicle_to_set[company] = vehicle_to_set

    try:
        vehicle_to_last_rider = company_to_vehicle_to_last_rider[company]
    except KeyError:
        vehicle_to_last_rider = {}
        company_to_vehicle_to_last_rider[company] = vehicle_to_last_rider

    vehicle_set = set()
    for vehicle_dict in vehicles:
        vehicle_set.add(vehicle_dict["n"])
    for vehicle in vehicle_set:
        vehicle_to_set[vehicle] = vehicle_set

    for vehicle_dict in vehicles:
        if vehicle_dict["m"] == "R":
            vehicle_to_last_rider[vehicle_dict["n"]] = rider_username

for (company, vehicle_to_set) in company_to_vehicle_to_set.items():
    vehicle_to_last_rider = company_to_vehicle_to_last_rider[company]

    known_vehicles = set()
    for (vehicle, vehicle_set) in vehicle_to_set.items():
        if not vehicle_set:
            continue
        if vehicle in known_vehicles:
            continue
        known_vehicles.update(vehicle_set)

        set_vehicles = list(vehicle_set)
        first_rider = vehicle_to_last_rider.get(set_vehicles[0])
        if first_rider is None:
            continue
        is_monopoly = True
        for next_vehicle in set_vehicles[1:]:
            next_rider = vehicle_to_last_rider.get(next_vehicle)
            if next_rider != first_rider:
                is_monopoly = False
                break

        if is_monopoly:
            yield (company, first_rider, sorted(set_vehicles))
$$;

UPDATE bim.schema_revision SET sch_rev=18;
