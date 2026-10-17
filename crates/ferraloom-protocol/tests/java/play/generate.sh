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
"$jdk/javac" -cp "$classpath" -d "$classes" "$root/crates/ferraloom-protocol/tests/java/play/PlayVectors.java"
cd "$classes"
"$jdk/java" -cp "$classes:$classpath" PlayVectors "$root/crates/ferraloom-protocol/src/play/fixtures" "$root/crates/ferraloom-protocol/tests/java/play/samples.tsv"
