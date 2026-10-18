Run `python3 generate.py` on devbox with the shared vanilla 26.3 jar, JDK 25,
libraries and the existing tree-harness Mockito jars. The harness invokes the
real Scoreboard, PlayerTeam, EntitySelector.pushableBy and ScoreboardSaveData
codec. Its saved fixture is read back from a gzip file written by vanilla
NbtIo, then recorded as SNBT. No Mojang binary is committed.

The 80 Phase A cases cover seven membership transitions, four identity/alliance
checks, 64 collision combinations, one real saved-file round trip, three codec
default cases and the writable statistic-criterion flag.
