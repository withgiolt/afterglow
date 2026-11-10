#!/usr/bin/env bash
# build-do.sh BEAM_DIR OUTDIR NAME [KEEP_MOD...]: a Durable Object Worker (glowc node)
# built from BEAM_DIR (glowc_node.erl is compiled into it), keeping KEEP_MOD reachable.
set -euo pipefail
cd "$(dirname "$0")/../.."
src=$1 out=$2 name=$3; shift 3
keep=(); for m in "${@:-counter}"; do keep+=(--keep "$m"); done
mkdir -p "$out"
erlc -o "$src" tests/erl/p2/glowc_node.erl
tools/glowc-build.sh "$out/app.wasm" --beams "$src" "${keep[@]}" --keep gen --keep gen_server --keep glowc_node --entry glowc_node:turn/0 2>/dev/null
build=$(shasum -a 256 "$out/app.wasm" | cut -c1-16)
sed "s/__BUILD__/$build/" tools/do/node.js > "$out/index.js"
cat > "$out/wrangler.jsonc" <<J
{
  "name": "$name",
  "main": "index.js",
  "compatibility_date": "2025-08-06",
  "durable_objects": { "bindings": [{ "name": "NODE", "class_name": "GlowNode" }] },
  "migrations": [{ "tag": "v1", "new_sqlite_classes": ["GlowNode"] }],
  "observability": { "enabled": true, "head_sampling_rate": 1 }
}
J
# Cron and its target, when CRON is set: CRON="* * * * *" CRON_NODE=t CRON_PROC=ticker
if [[ -n "${CRON:-}" ]]; then
  python3 - "$out/wrangler.jsonc" "$CRON" "${CRON_NODE:-default}" "${CRON_PROC:-ticker}" <<'P'
import sys
p, cron, node, proc = sys.argv[1:]
s = open(p).read()
s = s.replace('"observability"', '"triggers": { "crons": ["%s"] },\n  "vars": { "GLOWC_CRON_NODE": "%s", "GLOWC_CRON_PROC": "%s" },\n  "observability"' % (cron, node, proc))
open(p, "w").write(s)
P
fi
echo "$out build $build"
