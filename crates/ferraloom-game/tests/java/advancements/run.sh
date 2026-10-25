#!/usr/bin/env bash
set -euo pipefail
ADV_ROOT=$(git rev-parse --show-toplevel)
ADV_VANILLA=/home/doc/code/Ferraloom/.cache/vanilla/26.3
ADV_JDK=/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1
ADV_CLASSES=/home/doc/code/Ferraloom/.cache/tools/advancement-vectors
ADV_CP="$ADV_VANILLA/versions/26.3/server-26.3.jar"
while IFS= read -r jar; do ADV_CP="$ADV_CP:$jar"; done < <(find "$ADV_VANILLA/libraries" "$ADV_VANILLA/tree-harness" -name '*.jar' -type f | LC_ALL=C sort)
mkdir -p "$ADV_CLASSES"
"$ADV_JDK/bin/javac" --release 25 -cp "$ADV_CP" -d "$ADV_CLASSES" "$ADV_ROOT/crates/ferraloom-game/tests/java/advancements/AdvancementVectors.java" "$ADV_ROOT/crates/ferraloom-game/tests/java/advancements/AnnouncementVectors.java" "$ADV_ROOT/crates/ferraloom-game/tests/java/advancements/RecipeCraftedVectors.java" "$ADV_ROOT/crates/ferraloom-game/tests/java/advancements/ItemTriggerVectors.java" "$ADV_ROOT/crates/ferraloom-game/tests/java/advancements/SculkTriggerVectors.java" "$ADV_ROOT/crates/ferraloom-game/tests/java/advancements/FallHeightVectors.java" "$ADV_ROOT/crates/ferraloom-game/tests/java/advancements/HurtTriggerVectors.java"
cd "$ADV_VANILLA"
"$ADV_JDK/bin/java" -Xmx2g -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$ADV_CLASSES:$ADV_CP" AdvancementVectors "$ADV_ROOT/crates/ferraloom-game/src/advancements/vanilla.json.gz"

"$ADV_JDK/bin/java" -Xmx2g -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$ADV_CLASSES:$ADV_CP" AnnouncementVectors "$ADV_ROOT/crates/ferraloom-game/src/advancements/announcement.json.gz"

"$ADV_JDK/bin/java" -Xmx2g -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$ADV_CLASSES:$ADV_CP" RecipeCraftedVectors "$ADV_ROOT/crates/ferraloom-game/src/advancements/recipe-crafted.json.gz"

"$ADV_JDK/bin/java" -Xmx2g -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$ADV_CLASSES:$ADV_CP" ItemTriggerVectors "$ADV_ROOT/crates/ferraloom-game/src/advancements/item-triggers.json.gz"

"$ADV_JDK/bin/java" -Xmx2g -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$ADV_CLASSES:$ADV_CP" SculkTriggerVectors "$ADV_ROOT/crates/ferraloom-game/src/advancements/sculk-triggers.json.gz"

"$ADV_JDK/bin/java" -Xmx2g -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$ADV_CLASSES:$ADV_CP" FallHeightVectors "$ADV_ROOT/crates/ferraloom-game/src/advancements/fall-height.json.gz"

"$ADV_JDK/bin/java" -Xmx2g -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$ADV_CLASSES:$ADV_CP" HurtTriggerVectors "$ADV_ROOT/crates/ferraloom-game/src/advancements/hurt-triggers.json.gz"
