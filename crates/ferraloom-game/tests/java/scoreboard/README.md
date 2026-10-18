Run `python3 generate.py` on devbox with the shared vanilla 26.3 jar, JDK 25,
libraries and the existing tree-harness Mockito jars. The harness invokes the
real Scoreboard, PlayerTeam, EntitySelector.pushableBy and ScoreboardSaveData
codec. Its saved fixture is read back from a gzip file written by vanilla
NbtIo, then recorded as SNBT. No Mojang binary is committed.

The 84 Phase A cases cover seven membership transitions, four identity/alliance
checks, 64 collision combinations, one real saved-file round trip, three codec
default cases, four iterator/stream traversal checks and the writable statistic-criterion flag.

Additional codec/criterion cases cover fractional numeric decoding and three
statistic aliases (`mined:stone` becomes `minecraft.mined:minecraft.stone`).

`CommandVectors` adds 120 real dispatcher cases. Each case compares exact
feedback components, return values/errors and the ordered bytes emitted by real
ServerScoreboard hooks through all five native scoreboard packet codecs.
Capture includes both PlayerList broadcasts and direct player-connection sends
for objective tracking start/stop packets.
Only `/scoreboard players list <holder>` entry lines compare without ordering:
vanilla keys those entries by Objective identity hash, which varies with JVM
allocation. Their full components and header remain checked; packet order is exact.

Separate fixtures invoke real PlayerList join synchronization (one ordered
packet sequence), ReadOnlyScoreInfo formatting (four cases), and actual
ServerPlayer.awardStat/resetStat methods (two cases).

A real Commands.performCommand case captures a swap against a read-only source:
the target is already written and broadcast when the source write throws.

`TeamFeedbackVectors` adds nine real TeamCommand join/leave cases, including
same-team rejoining, duplicate holders, transfers, and mixed fake/entity holders.
Entity feedback-name access throws in the harness, proving that count-only
feedback does not read it. Leave feedback counts successful removals rather than
inputs, including a single fake-holder removal among unchanged entities.
