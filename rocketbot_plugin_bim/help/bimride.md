*Usage:* `{cpfx}bimride [--company COMPANY] VEHICLE[+VEHICLE...][/LINE]` or `{cpfx}bimride [--company COMPANY] LINE:VEHICLE[+VEHICLE...]`

Registers a ride with the given vehicle or multiple vehicles on the given line. Specifying a line is optional.

`-c` or `--company` specifies the company operating that vehicle. A default company chosen by the bot operator is used if none is supplied explicitly.

Examples: `{cpfx}bimride 49/37`, `{cpfx}bimride 4008+1408/38`, `{cpfx}bimride 3923+2523+2923+2924+2524+3924/U3`, `{cpfx}bimride --company wlb 101/WLB`, `{cpfx}bimride 38:4008+1408`, `{cpfx}bimride -c wlb WLB:404+101`
