#!/bin/sh
# Light Java-only oracle generation on devbox. Rust builds run through ferraloom-build.
set -eu
jdk=/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1
fastutil=/home/doc/code/Ferraloom/.cache/vanilla/26.3/libraries/it/unimi/dsi/fastutil/8.5.18/fastutil-8.5.18.jar
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"$jdk/bin/javac" -cp "$fastutil" -d "$tmp" "$root/tests/java/collections/CollectionsOracle.java"
"$jdk/bin/java" --add-opens java.base/java.util=ALL-UNNAMED -cp "$tmp:$fastutil" CollectionsOracle "$tmp/oracle.txt"
python3 "$root/tests/java/collections/pack.py" "$tmp/oracle.txt" "$root/src/collections/oracle.bin"
