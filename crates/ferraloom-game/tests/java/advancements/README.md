Run `bash crates/ferraloom-game/tests/java/advancements/run.sh` on devbox with the
shared vanilla 26.3 cache and JDK 25. The harness loads real vanilla registries,
uses the real advancement codecs, matchers, progress and player advancement
classes, and writes `src/advancements/vanilla.json.gz`. Mockito supplies explicit
live facts; any unspecified fact throws. No server or daily-driver world runs.

The fixture contains codec observations rather than copied datapack files:
normalized built-in advancement values, observed child order and positions,
synthetic trigger definitions/results, progress and player sequences, packet
bytes, actual saved JSON, and inline loot RNG observations. Object identity set
iteration varies between JVM runs, so the Rust layout test replays the captured
child order before comparing every position.

Phase 2 uses `PlayerAdvancements::matching_listeners` to capture matches, then
calls `award` and applies its effects **before the next award**. Recipe rewards
likewise use `recipe_holders` and `ServerRecipeBook::unlock_recipe` one at a time,
calling recipe_unlocked before advancing. The batch helpers are for state-only
callers. The save adapter supplies the system timezone offset at each obtained
time, including DST, to match vanilla's timestamp format.

`AnnouncementVectors` adds four actual `PlayerAdvancements#award` observations:
all before/after chat-rule combinations, with the XP reward changing both the
rule and display name. Its separate compressed fixture avoids recapturing
identity-dependent tree order when observing this synchronous callback boundary.

`RecipeCraftedVectors` observes the real recipe matcher with six ordered input
lists, including a repeated reference to the same stack. The retained live event
and pure event use these cases to check greedy ingredient matching and receiver
multiplicity. Rust tests also check retention cleanup after capture or predicate
errors.
