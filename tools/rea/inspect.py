#!/usr/bin/env python3
"""Inspect the pinned vanilla artifact with REA, keeping vendor bytes in .cache/."""

import argparse
import collections
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parents[2]
VERSION = "26.3"
SERVER_SHA1 = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c"
SERVER_URL = f"https://piston-data.mojang.com/v1/objects/{SERVER_SHA1}/server.jar"
REA_VERSION = "6.5.0"
STATUS_CLASSES = [
    "net.minecraft.server.network.ServerStatusPacketListenerImpl",
    "net.minecraft.network.protocol.handshake.ClientIntentionPacket",
    "net.minecraft.network.protocol.ping.ServerboundPingRequestPacket",
]


def digest(path, algorithm="sha256"):
    with path.open("rb") as source:
        return hashlib.file_digest(source, algorithm).hexdigest()


def rea(output, *arguments):
    """Use the version-pinned CLI without writing client or global configuration."""
    with output.open("w") as destination:
        subprocess.run(
            ["npm", "exec", "--yes", f"--package=rea-agents@{REA_VERSION}", "--",
             "rea", *map(str, arguments), "--format", "json"],
            cwd=ROOT, stdout=destination, check=True,
        )
    return json.loads(output.read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, default=ROOT / f".cache/vanilla/{VERSION}/server.jar")
    parser.add_argument("--java", type=Path, help="Optional existing Java 25+ executable for a REA --help capture")
    parser.add_argument("--javap", type=Path, help="Optional existing javap executable for focused bytecode inspection")
    arguments = parser.parse_args()
    cache = ROOT / ".cache/rea"
    cache.mkdir(parents=True, exist_ok=True)
    server = arguments.server.resolve()
    if not server.exists():
        server.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(SERVER_URL, timeout=60) as response, server.open("wb") as target:
            shutil.copyfileobj(response, target)
    if digest(server, "sha1") != SERVER_SHA1:
        raise SystemExit("Server SHA-1 differs from the pinned official Mojang artifact")

    outer = rea(cache / "server-artifact.json", "inspect-artifact", server)
    with zipfile.ZipFile(server) as archive:
        checksum, release, member = archive.read("META-INF/versions.list").decode().strip().split("\t")
        if release != VERSION:
            raise SystemExit(f"Unexpected bundled version: {release}")
        inner = cache / f"server-{VERSION}.jar"
        inner.write_bytes(archive.read(f"META-INF/versions/{member}"))
        if digest(inner) != checksum:
            raise SystemExit("Inner server differs from the bundler's declared SHA-256")
        main_class = archive.read("META-INF/main-class").decode().strip()
    inside = rea(cache / "inner-artifact.json", "inspect-artifact", inner)
    with zipfile.ZipFile(inner) as archive:
        files = [entry.filename for entry in archive.infolist() if not entry.is_dir()]
        version = json.loads(archive.read("version.json"))
        resources = collections.Counter(
            name.split("/")[2] for name in files if name.startswith("data/minecraft/")
        )
    report = {
        "release": VERSION,
        "rea_version": REA_VERSION,
        "server_url": SERVER_URL,
        "server_sha1": SERVER_SHA1,
        "server_sha256": digest(server),
        "inner_sha256": digest(inner),
        "inner_main_class": main_class,
        "version": version,
        "class_count": sum(name.endswith(".class") for name in files),
        "resource_file_counts": dict(sorted(resources.items())),
        "outer_evidence_id": outer["evidence_id"],
        "inner_evidence_id": inside["evidence_id"],
        "limitations": inside["limitations"],
    }

    if arguments.javap:
        with (cache / "status-bytecode.txt").open("w") as output:
            subprocess.run([str(arguments.javap.resolve()), "-classpath", str(inner), "-p", "-c", *STATUS_CLASSES],
                           stdout=output, check=True)
    if arguments.java:
        working = cache / "help-run"
        working.mkdir(exist_ok=True)
        scenario = {
            "executable": str(arguments.java.resolve()),
            "arguments": [f"-DbundlerRepoDir={working}", "-jar", str(server), "--help"],
            "working_directory": str(working),
            "filesystem_observation_paths": [str(working)],
            "timeout_ms": 30000,
            "limits": {"output_bytes": 262144, "file_bytes": 65536},
        }
        scenario_path = cache / "help-scenario.json"
        scenario_path.write_text(json.dumps(scenario, indent=2) + "\n")
        capture = rea(cache / "help-capture.json", "capture-process", scenario_path)
        report["help_capture_evidence_id"] = capture["evidence_id"]
    (cache / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
