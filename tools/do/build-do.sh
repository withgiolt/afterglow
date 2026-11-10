#!/usr/bin/env bash
# build-do.sh BEAM_DIR OUTDIR NAME [KEEP_MOD...]: a Durable Object Worker (afterglow node)
# built from BEAM_DIR (afterglow_node.erl is compiled into it), keeping KEEP_MOD reachable.
set -euo pipefail
cd "$(dirname "$0")/../.."
src=$1 out=$2 name=$3; shift 3
keep=(); for m in "${@:-counter}"; do keep+=(--keep "$m"); done
mkdir -p "$out"
erlc -o "$src" tests/erl/p2/afterglow_node.erl
tools/afterglow-build.sh "$out/app.wasm" --beams "$src" "${keep[@]}" --keep gen --keep gen_server --keep afterglow_node --entry afterglow_node:turn/0 2>/dev/null
build=$(shasum -a 256 "$out/app.wasm" | cut -c1-16)
sed "s/__BUILD__/$build/" tools/do/node.js > "$out/index.js"
cat > "$out/wrangler.jsonc" <<J
{
  "name": "$name",
  "main": "index.js",
  "compatibility_date": "2025-08-06",
  "durable_objects": { "bindings": [{ "name": "NODE", "class_name": "AfterglowNode" }] },
  "migrations": [{ "tag": "v1", "new_sqlite_classes": ["AfterglowNode"] }],
  "observability": { "enabled": true, "head_sampling_rate": 1 }
}
J
# Cron and its target, when CRON is set: CRON="* * * * *" CRON_NODE=t CRON_PROC=ticker
if [[ -n "${CRON:-}" ]]; then
  node - "$out/wrangler.jsonc" "$CRON" "${CRON_NODE:-default}" "${CRON_PROC:-ticker}" <<'N'
const fs = require("fs");
const [p, cron, node, proc] = process.argv.slice(2);
const s = fs.readFileSync(p, "utf8").replace('"observability"',
  `"triggers": { "crons": ["${cron}"] },\n  "vars": { "AFTERGLOW_CRON_NODE": "${node}", "AFTERGLOW_CRON_PROC": "${proc}" },\n  "observability"`);
fs.writeFileSync(p, s);
N
fi
echo "$out build $build"
