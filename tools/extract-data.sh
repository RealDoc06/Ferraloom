#!/usr/bin/env bash
# Java 25 is needed only to regenerate committed facts, never for a Rust build.
set -euo pipefail
source "$(dirname "$0")/common.sh"
"$ROOT/tools/fetch-vanilla.sh"
CP="$VANILLA/versions/26.3/server-26.3.jar"
while IFS= read -r jar; do CP="$CP:$jar"; done < <(find "$VANILLA/libraries" -type f -name '*.jar' | LC_ALL=C sort)
CLASSES="$CACHE/tools/extractor-classes"
mkdir -p "$CLASSES" "$ROOT/data/generated"
"$JAVAC" --release 25 -cp "$CP" -d "$CLASSES" "$ROOT/tools/extractor/ExtractData.java" "$ROOT/tools/extractor/ExtractTagKeys.java" "$ROOT/tools/extractor/ExtractBlockMethods.java" "$ROOT/tools/extractor/ExtractBlockEntityTypes.java" "$ROOT/tools/extractor/ExtractRedstoneProperties.java" "$ROOT/tools/extractor/ExtractDispenserProperties.java" "$ROOT/tools/extractor/ExtractNoteProperties.java"
"$JAVAC" --release 25 -cp "$CP" -d "$CLASSES" "$ROOT/tools/extractor/ExtractFeatureRequirements.java" "$ROOT/tools/extractor/ExtractCopperChestProperties.java" "$ROOT/tools/extractor/ExtractCommandBlockProperties.java"
"$JAVAC" --release 25 -cp "$CP" -d "$CLASSES" "$ROOT/tools/extractor/ExtractBuildingProperties.java"
# Keep bootstrap logs and incidental files in the shared cache.
cd "$VANILLA"
"$JAVA" -Xmx2G -cp "$CLASSES:$CP" ExtractData "$ROOT/data/generated"
"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractTagKeys "$VANILLA/versions/26.3/server-26.3.jar" "$ROOT/data/generated/tag_keys.json"
"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractBlockMethods "$ROOT/data/generated/block_methods.json"
"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractBlockEntityTypes "$ROOT/data/generated/block_entity_types.json"
"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractRedstoneProperties "$ROOT/data/generated/redstone_properties.json"
for report in blocks registries packets commands; do
    cp "$VANILLA/generated/reports/$report.json" "$ROOT/data/generated/$report.json"
done

"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractDispenserProperties "$ROOT/data/generated/dispenser_properties.json"
"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractNoteProperties "$ROOT/data/generated/note_properties.json"
"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractFeatureRequirements "$ROOT/data/generated/feature_requirements.json"

"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractCopperChestProperties "$ROOT/data/generated/copper_chest_properties.json"

"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractCommandBlockProperties "$ROOT/data/generated/command_block_properties.json"
"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractBuildingProperties "$ROOT/data/generated/building_properties.json"

"$JAVAC" --release 25 -cp "$CP" -d "$CLASSES" "$ROOT/tools/extractor/ExtractWeatheringCopper.java"
"$JAVA" -Xmx512M -cp "$CLASSES:$CP" ExtractWeatheringCopper "$ROOT/data/generated/weathering_copper.json"
