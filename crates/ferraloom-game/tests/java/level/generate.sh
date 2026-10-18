#!/usr/bin/env bash
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cache=/home/doc/code/Ferraloom/.cache
jdk="$cache/toolchains/jdk-25.0.4.1+1/bin"
vanilla="$cache/vanilla/26.3"
classes=$(mktemp -d)
trap 'rm -rf "$classes"' EXIT
classpath="$vanilla/versions/26.3/server-26.3.jar"
while IFS= read -r -d '' library; do classpath="$classpath:$library"; done < <(find "$vanilla/libraries" -name '*.jar' -print0)
"$jdk/javac" -cp "$classpath" -d "$classes" "$root/crates/ferraloom-game/tests/java/level/TickVectors.java" "$root/crates/ferraloom-game/tests/java/level/NeighborVectors.java" "$root/crates/ferraloom-game/tests/java/level/SimulationVectors.java" "$root/crates/ferraloom-game/tests/java/level/TickRateVectors.java"
cd "$classes"
"$jdk/java" -Xmx512M -cp "$classes:$classpath" TickRateVectors > "$root/crates/ferraloom-game/tests/java/level/tick-rate.tsv"
"$jdk/java" -Xmx512M -cp "$classes:$classpath" SimulationVectors > "$root/crates/ferraloom-game/tests/java/level/simulation.tsv"
"$jdk/java" -Xmx512M -cp "$classes:$classpath" TickVectors > "$root/crates/ferraloom-game/tests/java/level/ticks.tsv"

"$jdk/java" -Xmx512M -cp "$classes:$classpath" NeighborVectors generate "$classes/NeighborFixture.java"
"$jdk/javac" -cp "$classes:$classpath" -d "$classes" "$classes/NeighborFixture.java"
"$jdk/java" -Xmx512M -cp "$classes:$classpath" NeighborVectors > "$classes/neighbors.tsv"
rg '^(direction-order|skip-west|nested-layer|chain-limit|zero-limit|random-positions|valid-chunk-bound|setblock-[0-9]+|simple-waterlogged|analog-neighbors)\t' "$classes/neighbors.tsv" > "$root/crates/ferraloom-game/tests/java/level/neighbors.tsv"

rg '^tag:' "$classes/neighbors.tsv" > "$root/crates/ferraloom-game/tests/java/level/tags.tsv"

rg '^fluid-legacy:' "$classes/neighbors.tsv" > "$root/crates/ferraloom-game/tests/java/level/fluids.tsv"

rg '^fluid-tag:' "$classes/neighbors.tsv" > "$root/crates/ferraloom-game/tests/java/level/fluid-tags.tsv"

rg '^water-slope:' "$classes/neighbors.tsv" > "$root/crates/ferraloom-game/tests/java/level/slopes.tsv"

rg '^precipitation-slots:' "$classes/neighbors.tsv" > "$root/crates/ferraloom-game/tests/java/level/precipitation.tsv"

rg '^thunder-slots:' "$classes/neighbors.tsv" > "$root/crates/ferraloom-game/tests/java/level/thunder.tsv"

rg '^fluid-wall:' "$classes/neighbors.tsv" > "$root/crates/ferraloom-game/tests/java/level/walls.tsv"
