#!/usr/bin/env bash
# build-do.sh VERSION_DIR OUTDIR NAME: a Durable Object Worker for the P2 counter.
set -euo pipefail
cd "$(dirname "$0")/../.."
src=$1 out=$2 name=$3
mkdir -p "$out"
erlc -o "$src" tests/erl/p2/glowc_node.erl
tools/glowc-build.sh "$out/app.wasm" --beams "$src" --keep counter --keep gen --keep gen_server --keep glowc_node --entry glowc_node:turn/0 2>/dev/null
build=$(shasum -a 256 "$out/app.wasm" | cut -c1-16)
sed "s/__BUILD__/$build/" tools/do/node.js > "$out/index.js"
cat > "$out/wrangler.jsonc" <<J
{
  "name": "$name",
  "main": "index.js",
  "compatibility_date": "2025-08-06",
  "account_id": "00000000000000000000000000000000",
  "durable_objects": { "bindings": [{ "name": "NODE", "class_name": "GlowNode" }] },
  "migrations": [{ "tag": "v1", "new_sqlite_classes": ["GlowNode"] }],
  "observability": { "enabled": true, "head_sampling_rate": 1 }
}
J
echo "$out build $build"
