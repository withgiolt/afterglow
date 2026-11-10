#!/usr/bin/env bash
# otp-libs.sh: the ebin dirs of the newest stdlib, kernel and erts in the OTP that `erl`
# runs, one per line. (An in-place OTP upgrade can leave older versions next to them.)
set -euo pipefail
otp=$(erl -noshell -eval 'io:put_chars(code:root_dir()), halt().')
for app in stdlib kernel erts; do
  ls -d "$otp"/lib/$app-*/ebin | sort -V | tail -1
done
