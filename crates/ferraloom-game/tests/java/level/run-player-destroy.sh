#!/usr/bin/env bash
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cache=/home/doc/code/Ferraloom/.cache
jdk="$cache/toolchains/jdk-25.0.4.1+1/bin"
vanilla="$cache/vanilla/26.3"
classes=$(mktemp -d)
trap 'python3 -c "import shutil,sys; shutil.rmtree(sys.argv[1])" "$classes"' EXIT
classpath="$vanilla/versions/26.3/server-26.3.jar"
while IFS= read -r -d '' library; do classpath="$classpath:$library"; done < <(find "$vanilla/libraries" "$vanilla/tree-harness" -name '*.jar' -print0)
"$jdk/javac" -cp "$classpath" -d "$classes" "$root/crates/ferraloom-game/tests/java/level/DestroyBlockVectors.java" "$root/crates/ferraloom-game/tests/java/level/MiningVectors.java" "$root/crates/ferraloom-game/tests/java/level/PlayerDestroyVectors.java"
cd "$classes"
"$jdk/java" -Xmx768M -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$classes:$classpath" PlayerDestroyCapture | sed -n 's/^PLAYER_DESTROY\t//p' > "$root/crates/ferraloom-game/tests/java/level/player-destroy.tsv"
