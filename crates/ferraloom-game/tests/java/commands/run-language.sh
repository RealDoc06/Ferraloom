#!/usr/bin/env bash
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
VANILLA=/home/doc/code/Ferraloom/.cache/vanilla/26.3
JDK=/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1
CLASSES=/home/doc/code/Ferraloom/.cache/tools/command-vectors
CP="$VANILLA/versions/26.3/server-26.3.jar"
while IFS= read -r jar; do CP="$CP:$jar"; done < <(find "$VANILLA/libraries" "$VANILLA/tree-harness" -name '*.jar' -type f | LC_ALL=C sort)
python3 - "$ROOT" "$VANILLA/versions/26.3/server-26.3.jar" <<'PYDATA'
import pathlib, sys, zipfile
root = pathlib.Path(sys.argv[1]) / "data/generated"
with zipfile.ZipFile(sys.argv[2]) as jar:
    for source, target in [("en_us.json", "en_us.json"), ("deprecated.json", "deprecated_translations.json")]:
        (root / target).write_bytes(jar.read("assets/minecraft/lang/" + source))
PYDATA
mkdir -p "$CLASSES"
"$JDK/bin/javac" --release 25 -cp "$CP" -d "$CLASSES" "$ROOT/crates/ferraloom-game/tests/java/commands/LanguageVector.java"
cd "$VANILLA"
"$JDK/bin/java" -Xmx512m -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$CLASSES:$CP" LanguageVector "$ROOT/crates/ferraloom-game/src/commands/vanilla/language-vectors.json"
