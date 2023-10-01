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

UPDATE bim.schema_revision SET sch_rev=7;
