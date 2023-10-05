CREATE INDEX IF NOT EXISTS idx_ride_vehicles_ridden ON bim.ride_vehicles (ride_id, vehicle_number) WHERE coupling_mode = 'R';

UPDATE bim.schema_revision SET sch_rev=8;
