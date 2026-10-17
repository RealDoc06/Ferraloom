#!/usr/bin/env python3
"""Generate storage fixtures, or boot a Rust-rewritten world with vanilla 26.3.

Uses only loopback networking and a temporary world. No server jar is committed.
Run: python3 tests/vanilla.py generate --jar /path/to/server.jar --java /path/to/java
     python3 tests/vanilla.py verify /path/to/rewritten/world --jar ... --java ...
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import shutil
import socket
import struct
import subprocess
import tempfile
import threading
import time
import uuid
import zlib

NAME = "StorageFixture"
_digest = bytearray(hashlib.md5(("OfflinePlayer:" + NAME).encode()).digest())
_digest[6] = (_digest[6] & 15) | 48
_digest[8] = (_digest[8] & 63) | 128
PLAYER = uuid.UUID(bytes=bytes(_digest))


# Fixed scenarios, independent of the NBT codec under test. Rust and vanilla both
# assert these coordinates; the default Y-axis log also checks string shorthand.
BLOCK_STATES = json.loads((Path(__file__).parent / "fixtures/block_states.json").read_text())


def block_argument(case):
    properties = ",".join(f"{key}={value}" for key, value in case["properties"].items())
    return case["id"] + "[" + properties + "]"


def varint(n):
    out = bytearray()
    while n >= 128:
        out.append((n & 127) | 128)
        n >>= 7
    return bytes(out + bytes([n]))


def read_varint(read):
    n = 0
    for i in range(5):
        b = read(1)[0]
        n |= (b & 127) << (7 * i)
        if not b & 128:
            return n
    raise ValueError("oversized VarInt")


def string(s):
    data = s.encode()
    return varint(len(data)) + data


class Client:
    def __init__(self, port):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=20)
        self.compression = -1

    def exact(self, count):
        data = bytearray()
        while len(data) < count:
            part = self.sock.recv(count - len(data))
            if not part:
                raise EOFError("vanilla closed the client connection")
            data.extend(part)
        return bytes(data)

    def send(self, packet, data=b""):
        data = varint(packet) + data
        if self.compression >= 0:
            data = varint(len(data)) + zlib.compress(data) if len(data) >= self.compression else b"\0" + data
        self.sock.sendall(varint(len(data)) + data)

    def receive(self):
        import io
        data = self.exact(read_varint(self.exact))
        if self.compression >= 0:
            stream = io.BytesIO(data)
            size = read_varint(stream.read)
            data = zlib.decompress(stream.read()) if size else stream.read()
        stream = io.BytesIO(data)
        return read_varint(stream.read), stream.read()

    def login(self, port):
        import io
        self.send(0, varint(777) + string("localhost") + struct.pack(">H", port) + varint(2))
        self.send(0, string(NAME) + PLAYER.bytes)
        while True:
            packet, data = self.receive()
            if packet == 3:
                self.compression = read_varint(io.BytesIO(data).read)
            elif packet == 2:
                self.send(3)
                break
            elif packet == 0:
                raise AssertionError("login disconnect: " + repr(data))
            else:
                raise AssertionError("unexpected login packet " + str(packet))
        while True:
            packet, data = self.receive()
            if packet == 15:
                self.send(7, b"\0")  # no cached registry packs
            elif packet == 3:
                self.send(3)
                break
            elif packet == 4:
                self.send(4, data)
            elif packet == 5:
                self.send(5, data)
            elif packet == 2:
                raise AssertionError("configuration disconnect: " + repr(data))
        # The first play packet proves that configuration completed.
        self.receive()


def run_server(world, jar, java, generate):
    scratch = Path(tempfile.mkdtemp(prefix="ferraloom-storage-server-"))
    shutil.copytree(world, scratch / "world") if world.exists() else None
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    (scratch / "eula.txt").write_text("eula=true\n")
    (scratch / "server.properties").write_text(
        f"server-ip=127.0.0.1\nserver-port={port}\nonline-mode=false\nwhite-list=false\n"
        "level-name=world\nlevel-seed=12345\nlevel-type=minecraft:flat\n"
        'generator-settings={"layers":[{"block":"minecraft:bedrock","height":1},{"block":"minecraft:dirt","height":2},{"block":"minecraft:grass_block","height":1}],"biome":"minecraft:plains","structure_overrides":[]}\n'
        "view-distance=2\nsimulation-distance=2\nspawn-chunk-radius=0\n"
        "sync-chunk-writes=true\nmax-tick-time=-1\n"
    )
    # Use the shared cache for extracted libraries; world and logs remain in /tmp.
    proc = subprocess.Popen(
        [java, "-Xms128M", "-Xmx1G", "-XX:ActiveProcessorCount=2",
         "--enable-native-access=ALL-UNNAMED", f"-DbundlerRepoDir={jar.parent}",
         "-jar", str(jar), "--nogui"],
        cwd=scratch, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, text=True, bufsize=1,
    )
    lines, events = [], queue.Queue()

    def reader():
        for line in proc.stdout:
            lines.append(line)
            events.put(line)
        events.put(None)

    threading.Thread(target=reader, daemon=True).start()

    def wait_for(needle, timeout=90):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            line = events.get(timeout=max(0.1, deadline - time.monotonic()))
            if line is None:
                raise AssertionError("vanilla exited before " + needle + "\n" + "".join(lines))
            if needle in line:
                return
        raise TimeoutError("vanilla did not report " + needle)

    def commands(text):
        proc.stdin.write(text + "\n")
        proc.stdin.flush()

    try:
        wait_for("Done (")
        if generate:
            commands('setworldspawn 0 -60 0\nforceload add 0 0 31 15\nsetblock 1 -60 1 minecraft:barrel{Items:[{Slot:0b,id:"minecraft:diamond",count:3}]}\nsetblock 2 -60 1 minecraft:bell\nsummon minecraft:armor_stand 3 -59 1 {CustomName:{text:"Storage fixture"}}')
            wait_for("Summoned new")
            # Contain the slab's water and keep wire powered across ordinary ticks/restarts.
            commands("fill 8 -60 2 10 -60 4 minecraft:stone\nsetblock 11 -61 3 minecraft:redstone_block")
            for case in BLOCK_STATES:
                position = " ".join(map(str, case["pos"]))
                commands(f"setblock {position} {block_argument(case)}")
            commands('data merge block 1 -60 6 {Items:[{Slot:0b,id:"minecraft:emerald",count:4}]}\ndata merge block 7 -60 6 {Items:[{Slot:2b,id:"minecraft:iron_ingot",count:2}]}')
        client = Client(port)
        client.login(port)
        wait_for("joined the game")
        if generate:
            commands(f"give {NAME} minecraft:stone 5\nadvancement grant {NAME} only minecraft:story/root")
            wait_for("advancement")
        else:
            # Command predicates prove the rewritten block entity and entity loaded.
            commands('execute if items block 1 -60 1 container.0 minecraft:diamond run say STORAGE_BLOCK_OK\nexecute if entity @e[type=minecraft:armor_stand,name="Storage fixture"] run say STORAGE_ENTITY_OK')
            wait_for("STORAGE_BLOCK_OK")
            wait_for("STORAGE_ENTITY_OK")
        # These predicates run both before fixture generation and on the Rust-resaved copy.
        for index, case in enumerate(BLOCK_STATES):
            position = " ".join(map(str, case["pos"]))
            commands(f"execute if block {position} {block_argument(case)} run say STORAGE_STATE_{index}_OK")
        commands("execute if items block 1 -60 6 container.0 minecraft:emerald run say STORAGE_CHEST_OK\nexecute if items block 7 -60 6 container.2 minecraft:iron_ingot run say STORAGE_FURNACE_OK\nsay STORAGE_STATES_DONE")
        wait_for("STORAGE_STATES_DONE")
        for marker in [*(f"STORAGE_STATE_{i}_OK" for i in range(len(BLOCK_STATES))), "STORAGE_CHEST_OK", "STORAGE_FURNACE_OK"]:
            if not any(marker in line for line in lines):
                raise AssertionError("vanilla state/inventory predicate failed: " + marker)
        client.sock.close()
        commands("save-all flush")
        wait_for("Saved the game")
        commands("stop")
        proc.wait(timeout=60)
        if proc.returncode:
            raise AssertionError("vanilla exit status " + str(proc.returncode))
        text = "".join(lines)
        # Unrelated auth discovery/network warnings are harmless; storage errors are not.
        bad = [line for line in lines if re.search(r"\bERROR\b|Failed to (load|read|save)|corrupt|invalid sector|wrong location", line, re.I)]
        if bad:
            raise AssertionError("vanilla storage errors:\n" + "".join(bad))
        print(f"vanilla 26.3 accepted world, player {PLAYER}, {len(BLOCK_STATES)} block states, block entities and entity; exit 0")
        return scratch / "world"
    except BaseException:
        print("".join(lines))
        raise
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


def compact_region(path, keep):
    data = path.read_bytes()
    output, sector = bytearray(8192), 2
    for index in sorted(keep):
        offset = int.from_bytes(data[4 * index:4 * index + 4], "big")
        start, count = offset >> 8, offset & 255
        if not offset:
            continue
        output[4 * index:4 * index + 4] = ((sector << 8) | count).to_bytes(4, "big")
        output[4096 + 4 * index:4100 + 4 * index] = data[4096 + 4 * index:4100 + 4 * index]
        output.extend(data[start * 4096:(start + count) * 4096])
        sector += count
    path.write_bytes(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["generate", "verify", "compression", "lock", "unlocked"])
    parser.add_argument("world", nargs="?", type=Path)
    parser.add_argument("--jar", type=Path, default=os.environ.get("FERRALOOM_VANILLA_JAR"))
    parser.add_argument("--java", default=str(Path(os.environ["JAVA_HOME"]) / "bin/java") if "JAVA_HOME" in os.environ else "java")
    args = parser.parse_args()
    if args.jar is None:
        parser.error("--jar or FERRALOOM_VANILLA_JAR is required")
    if args.mode in ("compression", "lock", "unlocked"):
        if args.world is None:
            parser.error("compression requires an output directory")
        cache = args.jar.resolve().parent
        jars = list((cache / "libraries").rglob("*.jar")) + list((cache / "versions").rglob("*.jar"))
        if not jars:
            parser.error("vanilla jar must have been extracted in its shared cache")
        source = "CompressionVectors.java" if args.mode == "compression" else "DirectoryLockProbe.java"
        arguments = ["verify", str(args.world)] if args.mode == "compression" else [str(args.world), str(args.mode == "lock").lower()]
        with tempfile.TemporaryDirectory(prefix="ferraloom-storage-java-") as cwd:
            subprocess.run([args.java, "--enable-native-access=ALL-UNNAMED", "-cp", os.pathsep.join(map(str, jars)), str((Path(__file__).parent / source).resolve()), *arguments], cwd=cwd, check=True)
        return
    if args.mode == "generate":
        world = run_server(Path("/tmp/ferraloom-storage-nonexistent-world"), args.jar.resolve(), args.java, True)
        destination = Path(__file__).parent / "fixtures/world"
        if destination.exists():
            shutil.rmtree(destination)
        destination.mkdir(parents=True)
        for source in world.rglob("*"):
            if source.is_file() and source.name != "session.lock":
                relative = source.relative_to(world)
                target = destination / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
        region = destination / "dimensions/minecraft/overworld/region"
        for path in region.glob("*.mca"):
            keep = {"r.0.0.mca": {0, 1}, "r.-1.0.mca": {31}}.get(path.name)
            if keep is None:
                path.unlink()
            else:
                compact_region(path, keep)
        print("Generated fixtures:", destination)
    else:
        if args.world is None:
            parser.error("verify requires the rewritten world path")
        run_server(args.world, args.jar.resolve(), args.java, False)


if __name__ == "__main__":
    main()
