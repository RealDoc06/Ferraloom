Run `bash crates/ferraloom-game/tests/java/stats/run.sh` on devbox with the shared
vanilla cache and JDK 25. It observes the real Stats registries, StatsCounter and
ServerStatsCounter, their JSON codec and dirty packets, and a file written by
ServerStatsCounter#save. Output is `src/stats/vanilla.json.gz`.

The mock data fixer accepts only the identity update from 5023 to 5023. Older
save migration remains an explicit Unsupported result until Ferraloom has data
fixing. REQUEST_STATS drains the dirty set; it does not mark all values dirty.
Live player statistic and scoreboard hooks belong to phase 2.

`HashMapVectors.java` observes JDK 25 `putAll` for 80 combinations of source
size, existing destination entries, constructor capacity and load factor.
Regenerate its collection fixture with JDK 25:

```sh
java --add-opens java.base/java.util=ALL-UNNAMED crates/ferraloom-game/tests/java/stats/HashMapVectors.java crates/ferraloom-util/src/collections/put-all.tsv
```
