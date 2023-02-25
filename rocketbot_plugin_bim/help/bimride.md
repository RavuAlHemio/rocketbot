*Usage:* `{cpfx}bimride [OPTIONS] VEHICLE[+VEHICLE[!]...][/LINE]` or `{cpfx}bimride [OPTIONS] LINE:VEHICLE[+VEHICLE[!]...]`

Registers a ride with the given vehicle or multiple vehicles on the given line. Specifying a line is optional. Suffixing a vehicle number with `!` marks that vehicle as the one actually ridden, which is especially useful in the case of multiple vehicles.

The following options are supported:

* `-c` or `--company` specifies the company operating that vehicle. A default company chosen by the bot operator is used if none is supplied explicitly.
* `-r` or `--rider` specifies the rider riding the vehicle. This option can only be used by `bim` administrators. By default, the user issuing the command is considered the rider.
* `-t` or `--timestamp` specifies a timestamp for this ride. This option can only be used by `bim` administrators. By default, the current timestamp is taken.
* `-u` or `--utc` interprets timestamps as UTC. The default assumes local time. This option is useful if the local timestamp is ambiguous (e.g. during daylight saving time adjustments).
* `-s` or `--sandbox` skips storing the ride in the database, but produces the expected output.

Examples: `{cpfx}bimride 49/37`, `{cpfx}bimride 4008!+1408/38`, `{cpfx}bimride 3923+2523+2923+2924+2524+3924/U3`, `{cpfx}bimride --company wlb 101/WLB`, `{cpfx}bimride 38:4008+1408!`, `{cpfx}bimride -c wlb WLB:404!+101`
