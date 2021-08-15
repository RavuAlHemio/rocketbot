#!/bin/sh
ATTEMPTS=5

fetch_ret="-1"
for retry in `seq 0 $ATTEMPTS`
do
    cargo fetch
    fetch_ret="$?"

    if [ "$fetch_ret" -eq "0" ]
    then
        break
    fi
done

exit "$fetch_ret"
