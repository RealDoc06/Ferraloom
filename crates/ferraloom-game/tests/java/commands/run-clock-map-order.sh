#!/usr/bin/env bash
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
VANILLA=/home/doc/code/Ferraloom/.cache/vanilla/26.3
JDK=/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1
CLASSES=/home/doc/code/Ferraloom/.cache/tools/command-vectors
CP="$VANILLA/versions/26.3/server-26.3.jar"
while IFS= read -r jar; do CP="$CP:$jar"; done < <(find "$VANILLA/libraries" "$VANILLA/tree-harness" -name '*.jar' -type f | LC_ALL=C sort)
mkdir -p "$CLASSES"
"$JDK/bin/javac" --release 25 -cp "$CP" -d "$CLASSES" "$ROOT/crates/ferraloom-game/tests/java/commands/ClockMapOrderVector.java"
cd "$VANILLA"
for run in 1 2; do
  "$JDK/bin/java" -Xmx512m -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$CLASSES:$CP" ClockMapOrderVector "/home/doc/code/Ferraloom/.cache/logs/command-clock-map-order-$run.json" "$((run-1))"
done
python3 - "$ROOT" <<'PY_CAPTURE'
import json,sys
from pathlib import Path
captures=[json.loads(Path(f"/home/doc/code/Ferraloom/.cache/logs/command-clock-map-order-{run}.json").read_text()) for run in [1,2]]
Path(sys.argv[1],"crates/ferraloom-game/src/commands/vanilla/full-clock-map-orders.json").write_text(json.dumps(captures,indent=2)+"\n")
PY_CAPTURE
