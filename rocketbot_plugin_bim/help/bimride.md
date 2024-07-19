*Usage:* `{cpfx}bimride [OPTIONS] VEHICLE[+VEHICLE[!]...][/LINE]` or `{cpfx}bimride [OPTIONS] LINE:VEHICLE[+VEHICLE[!]...]`

Registers a ride with the given vehicle or multiple vehicles on the given line. Specifying a line is optional. Suffixing a vehicle number with `!` marks that vehicle as the one actually ridden, which is especially useful in the case of multiple vehicles.

The following options are supported:

* `{sopfx}c COMPANY` or `{lopfx}company COMPANY` specifies the company operating that vehicle. A default company chosen by the bot operator is used if none is supplied explicitly.
* `{sopfx}r RIDER` or `{lopfx}rider RIDER` specifies the rider riding the vehicle. This option can only be used by `bim` administrators. By default, the user issuing the command is considered the rider.
* `{sopfx}t TIMESTAMP` or `{lopfx}timestamp TIMESTAMP` specifies a timestamp for this ride. This option can only be used by `bim` administrators. By default, the current timestamp is taken, optionally adjusted by the value of `{sopfx}b`/`{lopfx}backdate` (which cannot be used simultaneously with `{sopfx}t`/`{lopfx}timestamp`).
* `{sopfx}u` or `{lopfx}utc` interprets timestamps as UTC. The default assumes local time. This option is useful if the local timestamp is ambiguous (e.g. during daylight saving time adjustments).
* `{sopfx}b MINUTES` or `{lopfx}backdate MINUTES` backdates the ride by the given amount of minutes. Cannot be used simultaneously with `{sopfx}t`/`{lopfx}timestamp`. Users who are not `bim` administrators might be limited in how far they may backdate their rides.
* `{sopfx}s` or `{lopfx}sandbox` skips storing the ride in the database, but produces the expected output.

The following options concern long-term ticket amortization calculation (see also `{cpfx}bimcost`):

* `{sopfx}P PRICE` or `{lopfx}price PRICE` specifies how much a regular ticket for the given trip would cost. Cannot be used simultaneously with `{sopfx}D`/`{lopfx}default-price`.
* `{sopfx}D` or `{lopfx}default-price` specifies that the default price for the given company should be assigned to the trip. Cannot be used simultaneously with `{sopfx}P`/`{lopfx}price`.
* `{sopfx}A PRICE` or `{lopfx}actual-price PRICE` specifies the actual price paid for the ticket, after applying all discounts provided through the long-term ticket. By default, this is assumed to be zero (i.e. the long-term ticket fully covers the cost).

Examples: `{cpfx}bimride 49/37`, `{cpfx}bimride 4008!+1408/38`, `{cpfx}bimride 3923+2523+2923+2924+2524+3924/U3`, `{cpfx}bimride {lopfx}company wlb 101/WLB`, `{cpfx}bimride 38:4008+1408!`, `{cpfx}bimride {sopfx}c wlb WLB:404!+101`
