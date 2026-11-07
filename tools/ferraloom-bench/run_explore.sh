#!/bin/sh
# Phase D: players exploring in different directions, Horizon vs vanilla vs Paper on vm-minecraft.
# One server at a time on port 25570 (Velocity keeps 25565); bots run on the laptop through the relay.
# After each run: count chunks by status in its region files, keep the log, delete the world (13 GB free disk).
cd "$HOME/horizon-bench" || exit 1
export PATH="$HOME/horizon-bench/jdk/bin:$PATH" PYTHONPATH="$HOME/horizon-bench/pylib"
export HORIZON_BENCH_BIN="${BIN:-$HOME/horizon-bench/horizon/horizon-linux-x86_64}"
export HORIZON_BENCH_JAVA="$HOME/horizon-bench/jdk/bin/java"
R=${R:-$HOME/horizon-bench/results}; mkdir -p $R
for config in ${CONFIGS:-25:11 50:11 100:11 100:22}; do
  set -- $(echo $config | tr : " ")
  for server in ${SERVERS:-horizon vanilla paper}; do
    tag="$server-$1-$2"
    [ -f "$R/$tag.json" ] && continue
    echo "$(date +%T) start $tag" >> $R/progress.log
    python3 repo/tools/server_bench.py --servers "$server" --bots "$1" --behaviour explore --explore-speed "$2" \
      --warmup 60 --seconds 180 --heap 8g --port 25570 --memory-gate-gb 10 \
      --bots-via "python3 $HOME/horizon-bench/relay_vm.py" --bots-bin horizon-bots --connect 10.10.10.101 \
      --out "$R/$tag.json" > "$R/$tag.console.txt" 2>&1
    run="$R/$tag-artifacts/$server-explore-$1"
    python3 count_chunks.py "$run" > "$R/$tag.chunks.json" 2>> "$R/$tag.console.txt"
    du -sh "$run" >> "$R/$tag.console.txt" 2>/dev/null
    rm -rf "$run"
    echo "$(date +%T) done $tag $(cat $R/$tag.chunks.json)" >> $R/progress.log
  done
done
echo "$(date +%T) ALL DONE" >> $R/progress.log
