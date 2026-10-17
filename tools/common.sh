#!/usr/bin/env bash
# Shared downloads live in the primary checkout, including when run in a worktree.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMMON=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir)
CACHE=${FERRALOOM_CACHE_DIR:-"$(dirname "$COMMON")/.cache"}
VANILLA="$CACHE/vanilla/26.3"
if [[ -n ${JAVA_HOME:-} ]]; then
    JAVA="$JAVA_HOME/bin/java"
    JAVAC="$JAVA_HOME/bin/javac"
elif [[ -x "$CACHE/toolchains/jdk-25.0.4.1+1/bin/java" ]]; then
    JAVA="$CACHE/toolchains/jdk-25.0.4.1+1/bin/java"
    JAVAC="$CACHE/toolchains/jdk-25.0.4.1+1/bin/javac"
else
    JAVA=java
    JAVAC=javac
fi
fetch() {
    local url=$1 path=$2 sha=$3
    if [[ -f "$path" ]] && printf '%s  %s\n' "$sha" "$path" | sha1sum --check --status; then
        return
    fi
    mkdir -p "$(dirname "$path")"
    curl --fail --location --retry 3 "$url" --output "$path.part"
    printf '%s  %s\n' "$sha" "$path.part" | sha1sum --check --status
    mv "$path.part" "$path"
}
