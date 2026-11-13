#!/usr/bin/env bash
# wisp-fixture.sh: one request per route of fixtures/wisp (`gleam build` there first),
# each compared with erl by tools/difftest-project.sh. Builds the wasm once.
cd "$(dirname "$0")/.."
set -u
proj=fixtures/wisp mod=wisp_app
fail=0
check() { # [HEADERS=...] METHOD URL [BODY]
  tools/difftest-project.sh $proj $mod "$@" || fail=1
  export SKIP_BUILD=1
}
check GET /
check GET /users/42
check GET /users/abc
check GET "/search?q=a&q=b%20c&x=1"
check GET /nowhere
check GET /old
check DELETE /json
HEADERS="content-type: application/json" check POST /json '{"name":"ada","age":36}'
HEADERS="content-type: application/json" check POST /json '{"name":"ada"}'
HEADERS="content-type: application/x-www-form-urlencoded" check POST /form 'name=ada+l&x=1'
HEADERS="content-type: application/x-www-form-urlencoded" check POST /form 'x=1'
check GET /visits
# wisp base64-encodes plain cookie values: NA is 4, so this response says "visit 5".
HEADERS="cookie: visits=NA" check GET /visits
[[ $fail == 0 ]]
