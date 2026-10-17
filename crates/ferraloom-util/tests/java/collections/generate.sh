#!/bin/sh
# Light Java-only oracle generation on devbox. Rust builds run through ferraloom-build.
set -eu
jdk=/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
vanilla=/home/doc/code/Ferraloom/.cache/vanilla/26.3
classpath=$(python3 - "$vanilla" <<'PYTHON'
import pathlib,sys
root=pathlib.Path(sys.argv[1])
print(':'.join(map(str,[root/'minecraft.jar',*sorted((root/'libraries').rglob('*.jar'))])))
PYTHON
)
"$jdk/bin/javac" -cp "$classpath" -d "$tmp" "$root/tests/java/collections/CollectionsOracle.java" "$root/tests/java/collections/HashOracle.java"
(
    cd "$tmp"
    "$jdk/bin/java" --add-opens java.base/java.util=ALL-UNNAMED -cp "$tmp:$classpath" CollectionsOracle "$tmp/oracle.txt"
    "$jdk/bin/java" --add-opens java.base/java.util=ALL-UNNAMED -cp "$tmp:$classpath" HashOracle "$root/src/collections/hashes.txt"
)
python3 "$root/tests/java/collections/pack.py" "$tmp/oracle.txt" "$root/src/collections/oracle.bin"
