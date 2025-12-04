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

CREATE OR REPLACE FUNCTION bim.last_rider_count_reached
( unit bigint
) RETURNS TABLE
( ride_count bigint
, ride_id bigint
, rider_username character varying(256)
, "timestamp" timestamp with time zone
)
LANGUAGE plpython3u
STABLE STRICT
AS $$
from collections import defaultdict
global unit

if unit is None:
    return
if unit == 0:
    unit = 1

rider_to_count = defaultdict(lambda: 0)
hit_counts = set()
for row in plpy.cursor('SELECT id, "timestamp", old_rider, new_rider FROM bim.ridden_vehicles_between_riders(false)'):
    if row["old_rider"] is not None:
        rider_to_count[row["old_rider"]] -= 1
    rider_to_count[row["new_rider"]] += 1

    for (rider, count) in rider_to_count.items():
        if count == 0:
            continue
        if count % unit == 0:
            if count in hit_counts:
                continue
            hit_counts.add(count)

            yield (count, row["id"], row["new_rider"], row["timestamp"])
$$;

CREATE OR REPLACE FUNCTION bim.days_without_rides
( since_date date
) RETURNS TABLE
( from_date date
, to_date date
)
LANGUAGE plpython3u
STABLE STRICT
AS $$
import datetime
import re

global since_date

if since_date is None:
    return

DATE_RE = re.compile("^(?P<year>[0-9]+)-(?P<month>[0-9]+)-(?P<day>[0-9]+)$")
def parse_date(date_str):
    m = DATE_RE.match(date_str)
    return datetime.date(
        int(m.group("year")),
        int(m.group("month")),
        int(m.group("day")),
    )
def stringify_date(date):
    return f"{date.year:04}-{date.month:02}-{date.day:02}"

date_walker = parse_date(since_date)
today = datetime.date.today()
for row in plpy.execute('SELECT DISTINCT CAST("timestamp" AS date) tsdate FROM bim.rides ORDER BY tsdate'):
    ride_date = parse_date(row["tsdate"])
    if ride_date < date_walker:
        # fast forward
        continue

    if date_walker < ride_date:
        # there is a drought between date_walker and (ride_date - 1)
        day_before_ride = datetime.date.fromordinal(ride_date.toordinal() - 1)
        yield (stringify_date(date_walker), stringify_date(day_before_ride))
        date_walker = ride_date
    date_walker = datetime.date.fromordinal(date_walker.toordinal() + 1)
if date_walker < today:
    # drought at the end
    yield (stringify_date(date_walker), stringify_date(today))
$$;

CREATE OR REPLACE FUNCTION bim.max_last_rider_counts
() RETURNS TABLE
( rider_username character varying(256)
, last_rider_count bigint
, "timestamp" timestamp with time zone
)
LANGUAGE plpython3u
STABLE STRICT
AS $$
from collections import defaultdict

rider_to_count = defaultdict(lambda: 0)
rider_to_max = {}
for row in plpy.cursor('SELECT "timestamp", old_rider, new_rider FROM bim.ridden_vehicles_between_riders(false)'):
    if row["old_rider"] is not None:
        rider_to_count[row["old_rider"]] -= 1
    rider_to_count[row["new_rider"]] += 1

    for (rider, count) in rider_to_count.items():
        cur_max = rider_to_max.get(rider, None)
        if cur_max is None:
            rider_to_max[rider] = (count, row["timestamp"])
        else:
            cur_max_count = cur_max[0]
            if cur_max_count < count:
                rider_to_max[rider] = (count, row["timestamp"])

for (rider, (max_count, timestamp)) in sorted(rider_to_max.items()):
    yield (rider, max_count, timestamp)
$$;

CREATE OR REPLACE FUNCTION bim.last_rider_ranking_changes
() RETURNS TABLE
( id bigint
, "timestamp" timestamp with time zone
, rider_to_rank jsonb
)
LANGUAGE plpython3u
STABLE STRICT
AS $$
from collections import defaultdict
import json

rider_to_count = defaultdict(lambda: 0)
last_rider_to_rank = {}
for row in plpy.cursor('SELECT id, "timestamp", old_rider, new_rider FROM bim.ridden_vehicles_between_riders(false)'):
    if row["old_rider"] is not None:
        rider_to_count[row["old_rider"]] -= 1
    rider_to_count[row["new_rider"]] += 1

    count_to_riders = defaultdict(set)
    for (rider, count) in rider_to_count.items():
        count_to_riders[count].add(rider)

    counts = sorted(count_to_riders.keys())
    counts.reverse()

    rider_to_rank = {}
    current_rank = 1
    for count in counts:
        this_count_riders = count_to_riders[count]
        for rider in this_count_riders:
            rider_to_rank[rider] = current_rank

        current_rank += len(this_count_riders)

    if last_rider_to_rank != rider_to_rank:
        last_rider_to_rank = rider_to_rank
        yield (row["id"], row["timestamp"], json.dumps(rider_to_rank))
$$;

CREATE OR REPLACE FUNCTION bim.last_rider_ranking_change_diffs
() RETURNS TABLE
( id bigint
, "timestamp" timestamp with time zone
, rider_to_difference jsonb
)
LANGUAGE plpython3u
STABLE STRICT
AS $$
import json

last_rider_to_rank = {}
for row in plpy.cursor('SELECT id, "timestamp", rider_to_rank FROM bim.last_rider_ranking_changes()'):
    rider_to_rank = json.loads(row["rider_to_rank"])
    if last_rider_to_rank != rider_to_rank:
        differences = {}
        for rider, new_rank in rider_to_rank.items():
            old_rank = last_rider_to_rank.get(rider, None)
            if old_rank == new_rank:
                continue
            differences[rider] = (old_rank, new_rank)

        last_rider_to_rank = rider_to_rank
        yield (row["id"], row["timestamp"], json.dumps(differences))
$$;
