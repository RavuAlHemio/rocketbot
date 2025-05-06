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

UPDATE bim.schema_revision SET sch_rev=19;
