#!/usr/bin/env bash
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cache=/home/doc/code/Ferraloom/.cache
jdk="$cache/toolchains/jdk-25.0.4.1+1/bin"
vanilla="$cache/vanilla/26.3"
classes=$(mktemp -d)
trap 'rm -rf "$classes"' EXIT
classpath="$vanilla/versions/26.3/server-26.3.jar"
while IFS= read -r -d '' library; do classpath="$classpath:$library"; done < <(find "$vanilla/libraries" "$vanilla/tree-harness" -name '*.jar' -print0)
"$jdk/javac" -cp "$classpath" -d "$classes" "$root/crates/ferraloom-game/tests/java/level/lightning/LightningTargetVectors.java"
cd "$classes"
"$jdk/java" -Xmx512M -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$classes:$classpath" LightningTargetVectors "$root/crates/ferraloom-game/tests/java/level/lightning-targets.tsv"
