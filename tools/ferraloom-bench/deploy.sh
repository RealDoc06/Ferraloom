#!/bin/sh
# deploy.sh <sha>: copies the builder's horizon-server and horizon-client to vm-minecraft as builds/<sha>, with the
# builder's loader and libc beside them (the VM's Debian 12 glibc is too old). The loader sits next to the
# programs so /proc/self/exe's folder still holds horizon-client and the resources lookup still works.
set -e
sha=$1; tmp=/tmp/horizon-eval/builds/$sha
rm -rf "$tmp" && mkdir -p "$tmp/lib"
scp -q ferraloom-builder:/home/doc/code/Ferraloom/target/release/horizon-client "$tmp/horizon-client.bin"
scp -q ferraloom-builder:/home/doc/code/Ferraloom/target/release/horizon-server "$tmp/horizon-server.bin"
scp -q ferraloom-builder:/lib/x86_64-linux-gnu/libc.so.6 ferraloom-builder:/lib/x86_64-linux-gnu/libm.so.6 \
  ferraloom-builder:/lib/x86_64-linux-gnu/libgcc_s.so.1 "$tmp/lib/"
scp -q ferraloom-builder:/lib64/ld-linux-x86-64.so.2 "$tmp/"
for p in horizon-server horizon-client; do
  printf '#!/bin/sh\nd=$(dirname "$(readlink -f "$0")")\nexec "$d/ld-linux-x86-64.so.2" --library-path "$d/lib" "$d/%s.bin" "$@"\n' "$p" > "$tmp/$p"
done
chmod +x "$tmp"/horizon-* "$tmp/ld-linux-x86-64.so.2"
ssh doc@10.10.10.101 "rm -rf ~/horizon-bench/builds/$sha && mkdir -p ~/horizon-bench/builds"
scp -q -r "$tmp" doc@10.10.10.101:horizon-bench/builds/$sha
ssh doc@10.10.10.101 "~/horizon-bench/builds/$sha/horizon-server --help 2>&1 | head -2"
