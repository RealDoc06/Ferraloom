#!/usr/bin/env bash
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
JDK=/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1
JAR=/home/doc/code/Ferraloom/.cache/vanilla/26.3/libraries/com/mojang/brigadier/1.3.11/brigadier-1.3.11.jar
CLASSES=/home/doc/code/Ferraloom/.cache/tools/command-vectors
mkdir -p "$CLASSES"
"$JDK/bin/javac" --release 25 -cp "$JAR" -d "$CLASSES" "$ROOT/crates/ferraloom-brigadier/tests/java/SuggestionOrder.java"
"$JDK/bin/java" -cp "$CLASSES:$JAR" SuggestionOrder "$ROOT/crates/ferraloom-brigadier/src/suggestion-order.tsv"
