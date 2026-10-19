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
"$jdk/javac" -cp "$classpath" -d "$classes" "$root/crates/ferraloom-game/tests/java/level/NeighborVectors.java"
cd "$classes"
"$jdk/java" -Xmx512M -cp "$classes:$classpath" NeighborVectors generate "$classes/NeighborFixture.java"
"$jdk/javac" -cp "$classes:$classpath" -d "$classes" "$classes/NeighborFixture.java" "$root/crates/ferraloom-game/tests/java/level/PathNavigationRegionVectors.java"
"$jdk/java" -Xmx512M -cp "$classes:$classpath" PathNavigationRegionCapture | awk -F '\t' '$1 ~ /^(capture|mutation|collision|piston)$/ {print}' > "$classes/vectors.tsv"
mv "$classes/vectors.tsv" "$root/crates/ferraloom-game/tests/java/level/path-navigation-region.tsv"
