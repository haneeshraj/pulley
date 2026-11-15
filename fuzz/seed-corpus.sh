#!/usr/bin/env bash
# Copy the repository's real inputs into fuzz/corpus/<target>/ as libFuzzer's starting points.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p corpus/scene corpus/protocol corpus/svg_path corpus/illustrator corpus/figma
cp ../schema/examples/*.json corpus/scene/
cp ../core/pulley-core/tests/fuzz-seeds/figma-frame.json corpus/figma/
caps=../tests/fixtures/illustrator/captures
for c in "$caps"/*.capture.json; do
  name=$(basename "$c" .capture.json)
  { cat "$c"; printf '\0'; if [ -f "$caps/$name.svg" ]; then cat "$caps/$name.svg"; fi; } > "corpus/illustrator/$name"
done
printf 'M0 0L10 10Z' > corpus/svg_path/lines
printf 'M10 10 A 5 5 0 1 0 20 20 a3 6 45 0 1 10 0' > corpus/svg_path/arcs
printf 'M 0 0 C 10 0 20 10 20 20 S 30 40 40 40 Q 5 10 10 0 T 20 0 z' > corpus/svg_path/curves
printf '%s' '{"protocol":1,"id":"a","type":"ping"}' > corpus/protocol/ping
printf '%s' '{"protocol":1,"id":"b","type":"pair_request","code":"ABCD-2345","name":"Figma"}' > corpus/protocol/pair
printf '%s' '{"protocol":1,"id":"c","type":"import_result","transfer":"t","result":{"ok":true,"layers":3,"warnings":["w"],"timings":{"apply":1.5}}}' > corpus/protocol/result
{ printf '%s' '{"protocol":1,"id":"d","type":"push_scene","transfer":"t","scene":'; cat ../schema/examples/basic-shapes.json; printf '}'; } > corpus/protocol/push
