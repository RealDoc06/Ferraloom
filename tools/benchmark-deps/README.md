# Java comparison dependencies

These locks support `tools/server_bench.py` (Fabric + Lithium). `tools/runtime.py` downloads and verifies
the pinned Fabric libraries. Game files still come from the user's own Minecraft install.

`mods.lock.json` pins Lithium and Fabric API with
publisher URLs and hashes. `fabric.lock.json` pins the loader and its libraries.
These are developer benchmark inputs.
