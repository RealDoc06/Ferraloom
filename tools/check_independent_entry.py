#!/usr/bin/env python3
"""Exercise the independent executable through the external headless client.

This checks socket/world entry and keepalives, not graphical-client acceptance
or vanilla gameplay. It starts only its own loopback listener and child client.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import queue
import re
import signal
import socket
import subprocess
import threading
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/ferraloom"))
    parser.add_argument("--expected-host", required=True)
    args = parser.parse_args()
    if socket.gethostname() != args.expected_host:
        raise SystemExit("Unexpected execution host")
    root = Path(__file__).resolve().parents[1]
    binary = args.binary.resolve(strict=True)
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True):
        raise SystemExit("A clean committed source is required")
    run = root / ".cache/independent-entry" / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    run.mkdir(parents=True)
    events = queue.Queue()
    children = {}
    readers = []
    report = {"host": socket.gethostname(), "source_commit": source,
              "binary": str(binary), "binary_sha256": digest(binary),
              "scope": "headless socket/world entry only", "passed": False}

    def launch(name, command):
        child = subprocess.Popen(command, cwd=root, stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                 text=True, bufsize=1)
        children[name] = child

        def collect():
            count = 0
            with (run / f"{name}.log").open("w") as output:
                for line in child.stdout:
                    count += len(line.encode())
                    if count > 8 * 1024 * 1024:
                        events.put(("error", f"{name} output exceeded budget"))
                        return
                    output.write(line)
                    output.flush()
                    events.put((name, line))
            events.put((name, None))

        reader = threading.Thread(target=collect, daemon=True)
        readers.append(reader)
        reader.start()
        return child

    deadline = time.monotonic() + 20

    def receive(until=deadline):
        remaining = until - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("entry check exceeded its deadline")
        name, line = events.get(timeout=remaining)
        if name == "error":
            raise RuntimeError(line)
        if line is None:
            raise RuntimeError(f"{name} exited before requested cleanup")
        return name, line

    def send(client, action):
        client.stdin.write(json.dumps(action) + "\n")
        client.stdin.flush()

    try:
        server = launch("server", [str(binary), "--offline", "--workers", "2", "--bind", "127.0.0.1:0"])
        while True:
            name, line = receive()
            match = re.search(r"\| (127\.0\.0\.1):(\d+)\s*$", line)
            if name == "server" and match:
                port = match.group(2)
                break
        client = launch("client", ["node", "tools/parity/client.ts", "--host", "127.0.0.1",
                                   "--port", port, "--name", "FerraloomProbe",
                                   "--known-core-pack", "--block-data"])
        while True:
            name, line = receive()
            if name != "client":
                continue
            event = json.loads(line)
            if event.get("event") == "error":
                raise RuntimeError(event)
            if event.get("event") == "ready":
                report["ready"] = event
                break
        send(client, {"id": "move", "action": "move", "x": 17.5, "y": 64, "z": 8.5})
        # Wait for the real ten-second server keepalive interval. Continue
        # draining events during that time; no timer or counters are injected.
        inspect_at = time.monotonic() + 11
        while time.monotonic() < inspect_at:
            try:
                name, line = receive(min(inspect_at, deadline))
            except queue.Empty:
                break
            if name == "client" and json.loads(line).get("event") == "error":
                raise RuntimeError(line)
        send(client, {"id": "state", "action": "state"})
        while True:
            name, line = receive()
            if name != "client":
                continue
            event = json.loads(line)
            if event.get("event") == "error":
                raise RuntimeError(event)
            if event.get("id") == "state":
                state = event["result"]
                if not (event.get("ok") and state["phase"] == "play" and state["login"]
                        and state["registry_packets"] == 32 and state["chunks"] == 30
                        and len(state["chunk_packets"]) == 30
                        and state["teleport_confirmations"] == 1 and state["keepalives"] >= 1):
                    raise AssertionError(state)
                report["state"] = state
                break
        send(client, {"id": "quit", "action": "quit"})
        if client.wait(timeout=3) != 0:
            raise RuntimeError("client did not exit cleanly")
        server.send_signal(signal.SIGTERM)
        if server.wait(timeout=3) != 0:
            raise RuntimeError("server did not exit cleanly")
        report["passed"] = True
    except Exception as error:
        report["error"] = str(error)
    finally:
        for child in children.values():
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=3)
        for reader in readers:
            reader.join(timeout=1)
        report["exit_codes"] = {name: child.returncode for name, child in children.items()}
        report["logs"] = {path.name: digest(path) for path in sorted(run.glob("*.log"))}
        (run / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "result": str(run / "result.json"),
                      "error": report.get("error")}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
