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
"$jdk/javac" -cp "$classpath" -d "$classes" "$root/crates/ferraloom-game/tests/java/level/NeighborVectors.java" "$root/crates/ferraloom-game/tests/java/level/DestroyBlockVectors.java" "$root/crates/ferraloom-game/tests/java/level/ClipVectors.java"
cd "$classes"
"$jdk/java" -Xmx768M -cp "$classes:$classpath" ClipVectors | awk -F '\t' '$1 ~ /^(clip|traverse|memo|interaction|player-shape|with-position)$/ {print}' > "$root/crates/ferraloom-game/tests/java/level/clip.tsv"
