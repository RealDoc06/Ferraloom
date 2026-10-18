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

`CommandVectors` adds 118 real dispatcher cases. Each case compares exact
feedback components, return values/errors and the ordered bytes emitted by real
ServerScoreboard hooks through all five native scoreboard packet codecs.

Separate fixtures invoke real PlayerList join synchronization (one ordered
packet sequence), ReadOnlyScoreInfo formatting (four cases), and actual
ServerPlayer.awardStat/resetStat methods (two cases).
