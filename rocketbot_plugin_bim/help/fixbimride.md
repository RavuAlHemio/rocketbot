*Usage:* `{cpfx}fixbimride OPTIONS`

Changes properties of a recent ride.

Options are classified into two groups: those that identify the ride to modify and those that specify what to modify.

A ride is identified using:

* `{sopfx}i ID` or `{lopfx}id ID`: The ID of the ride to change. This can uniquely identify a ride.
* `{sopfx}r USERNAME` or `{lopfx}rider USERNAME`: The username of the rider. Their most recent ride is targeted.

If neither is given, the last ride of the rider who issued the command is targeted.

A ride is modified using:

* `{sopfx}d` or `{lopfx}delete`: The ride is deleted.
* `{sopfx}c COMPANY` or `{lopfx}company COMPANY`: The company is changed to the given value. Note that mis-identified fixed couplings etc. are not corrected unless the vehicles are re-specified using `{sopfx}v`/`{lopfx}vehicles`.
* `{sopfx}l LINE` or `{lopfx}line LINE`: The line is changed to the given value.
* `{sopfx}R USERNAME` or `{lopfx}set-rider USERNAME`: The ride is assigned to a different user. This option can only be used by `bim` administrators.
* `{sopfx}t TIMESTAMP` or `{lopfx}set-timestamp TIMESTAMP`: The ride's timestamp is changed. This option can only be used by `bim` administrators.
* `{sopfx}v VEHICLES` or `{lopfx}vehicles VEHICLES`: The vehicles of the ride are replaced by those in the given vehicle specification string (as well as any vehicles participating in a fixed coupling with the given vehicles).

The following options can modify the command behavior further:

* `{sopfx}u` or `{lopfx}utc`: Interprets timestamps as UTC. The default assumes local time. This option is useful if the local timestamp is ambiguous (e.g. during daylight saving time adjustments).

`bim` administrators can modify all rides. Riders can only modify their own rides which have been registered recently enough.
