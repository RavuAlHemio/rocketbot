*Usage:* `{cpfx}fixbimride OPTIONS`

Changes properties of a recent ride.

Options are classified into two groups: those that identify the ride to modify and those that specify what to modify.

A ride is identified using:

* `-i ID` or `--id ID`: The ID of the ride to change. This can uniquely identify a ride.
* `-r USERNAME` or `--rider USERNAME`: The username of the rider. Their most recent ride is targeted.

If neither is given, the last ride of the rider who issued the command is targeted.

A ride is modified using:

* `-d` or `--delete`: The ride is deleted.
* `-c COMPANY` or `--company COMPANY`: The company is changed to the given value. Note that mis-identified fixed couplings etc. are not corrected.
* `-l LINE` or `--line LINE`: The line is changed to the given value.
* `-R USERNAME` or `--set-rider USERNAME`: The ride is assigned to a different user. This option can only be used by `bim` administrators.
* `-t TIMESTAMP` or `--set-timestamp TIMESTAMP`: The ride's timestamp is changed. This option can only be used by `bim` administrators.

The following options can modify the command behavior further:

* `-u` or `--utc`: Interprets timestamps as UTC. The default assumes local time. This option is useful if the local timestamp is ambiguous (e.g. during daylight saving time adjustments).

`bim` administrators can modify all rides. Riders can only modify their own rides which have been registered recently enough.
