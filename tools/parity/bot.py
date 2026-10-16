"""Drive the bounded protocol-777 probe and verify its effects on a real server."""
from __future__ import annotations

import json
import os
from pathlib import Path
import queue
import re
import signal
import subprocess
import threading
import time
from typing import Callable, Protocol
import uuid

from parity import protocol


ROOT = Path(__file__).resolve().parents[2]
NAME = "FerraloomTest"


def delivered_chat(state: dict, token: str) -> dict | None:
    """Find the complete unique marker without requiring a specific chat decoration.

    Retain transport metadata and received text; delivery is separate from packet
    format, rendering, sender identity, and signature conformance.
    """
    marker = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(token) + r"(?![A-Za-z0-9_])")
    for observation in state.get("chat_messages", []):
        if isinstance(observation, dict) and isinstance(observation.get("text"), str) and marker.search(observation["text"]):
            return observation
    return None


class TestServer(Protocol):
    port: int
    directory: Path

    def command(self, command: str, timeout: float = 15) -> list[str]: ...


class Client:
    """One owned Node process; all stdout is JSON and stderr stays in the fixture."""

    def __init__(self, server: TestServer, *, block_data: bool = False, log_name: str = "wire-client",
                 name: str = NAME, container_metadata: bool = False, raw_packets: bool = False,
                 strict_offline_chat: bool = False, horns: bool = False):
        if not re.fullmatch(r"[A-Za-z0-9_-]+", log_name):
            raise ValueError("Invalid client log name")
        if not re.fullmatch(r"[A-Za-z0-9_]{1,16}", name, flags=re.ASCII):
            raise ValueError("Client name must be 1..16 ASCII username characters")
        self.name = name
        self.expected_disconnect: dict | None = None
        self.forced_close = False
        self.sequence = 0
        self.messages: queue.Queue[dict] = queue.Queue()
        self.pending: dict[int, dict] = {}
        self.events: list[dict] = []
        self.statistics_values: dict[tuple[int, int], dict | None] = {}
        self.failure: str | None = None
        self.stderr = (server.directory / f"{log_name}.stderr.log").open("w")
        try:
            self.process = subprocess.Popen(
                ["node", str(ROOT / "tools/parity/client.ts"), "--host", "127.0.0.1",
                 "--port", str(server.port), "--name", name,
                 *(["--block-data"] if block_data or container_metadata or horns else []),
                 *(["--container-metadata"] if container_metadata else []),
                 *(["--horns"] if horns else []),
                 *(["--strict-offline-chat"] if strict_offline_chat else []),
                 *(["--raw-packets"] if raw_packets else [])],
                cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr,
                text=True, bufsize=1, start_new_session=True,
            )
        except Exception:
            self.stderr.close()
            raise

        def read() -> None:
            if self.process.stdout is None:
                self.messages.put({"event": "error", "message": "Client stdout unavailable"})
                return
            with (server.directory / f"{log_name}.jsonl").open("w") as log:
                for line in self.process.stdout:
                    log.write(line)
                    log.flush()
                    try:
                        message = json.loads(line)
                        if not isinstance(message, dict):
                            raise ValueError("Expected JSON object")
                    except (ValueError, TypeError) as error:
                        message = {"event": "error", "message": f"Invalid client JSON: {error}"}
                    self.messages.put(message)
            self.messages.put({"event": "eof"})

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()

    def receive(self, timeout: float) -> dict:
        if self.failure:
            raise RuntimeError(self.failure)
        try:
            message = self.messages.get(timeout=max(0.001, timeout))
        except queue.Empty:
            raise TimeoutError("Timed out waiting for protocol client") from None
        event = message.get("event")
        if event == "error" or event == "eof" or (event == "end" and not message.get("expected")):
            self.failure = str(message.get("message", f"Client ended: {message}"))
            raise RuntimeError(self.failure)
        if event == "statistics_packet":
            for entry in message["entries"]:
                key = (entry["category_id"], entry["statistic_id"])
                if key in self.statistics_values:
                    self.statistics_values[key] = {"entry": entry, "packet": message}
        if "id" in message:
            self.pending[message["id"]] = message
        else:
            self.events.append(message)
            self.events = self.events[-100:]
        return message

    def watch_statistic(self, category_id: int, statistic_id: int) -> None:
        """Retain absolute updates and their raw packet for selected counters.

        AwardStats omits unchanged values. A later packet's omission preserves
        the last received value; it does not reset the counter or add a delta.
        """
        if any(type(value) is not int or value < 0 for value in (category_id, statistic_id)):
            raise ValueError("Statistic IDs must be nonnegative integers")
        key = (category_id, statistic_id)
        if key not in self.statistics_values and len(self.statistics_values) >= 64:
            raise ValueError("Watched statistics exceed probe budget")
        self.statistics_values.setdefault(key, None)

    def ready(self, timeout: float = 60) -> dict:
        for event in self.events:
            if event.get("event") == "ready":
                return event
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            message = self.receive(deadline - time.monotonic())
            if message.get("event") == "ready":
                return message
        raise TimeoutError("Client did not reach PLAY with position and chunks")

    def request(self, action: str, timeout: float = 10, **fields) -> dict | None:
        if self.failure:
            raise RuntimeError(self.failure)
        self.sequence += 1
        identifier = self.sequence
        protocol.require(self.process.stdin is not None, "Client stdin unavailable")
        self.process.stdin.write(json.dumps({"id": identifier, "action": action, **fields}) + "\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + timeout
        while identifier not in self.pending:
            if time.monotonic() >= deadline:
                raise TimeoutError(f"Client action timed out: {action}")
            self.receive(deadline - time.monotonic())
        reply = self.pending.pop(identifier)
        protocol.require(reply.get("ok") is True, reply)
        return reply.get("result")

    def state(self) -> dict:
        result = self.request("state")
        protocol.require(isinstance(result, dict), f"Invalid client state: {result}")
        return result

    def wait_state(self, predicate: Callable[[dict], bool], timeout: float = 15) -> dict:
        deadline = time.monotonic() + timeout
        state: dict = {}
        while time.monotonic() < deadline:
            state = self.state()
            if predicate(state):
                return state
            time.sleep(0.1)
        raise AssertionError(f"Client state predicate failed: {state}")

    def expect_spam_disconnect(self) -> None:
        """Arm only the named negative fixture; every other protocol error stays fatal."""
        self.request("expect_disconnect", translation="disconnect.spam")

    def wait_spam_disconnect(self, timeout: float = 10) -> dict:
        deadline = time.monotonic() + timeout
        observed = next((event for event in self.events if event.get("event") == "disconnect"), None)
        ended = next((event for event in self.events if event.get("event") == "end"), None)
        while observed is None or ended is None:
            message = self.receive(deadline - time.monotonic())
            if message.get("event") == "disconnect":
                observed = message
            if message.get("event") == "end":
                ended = message
            if time.monotonic() >= deadline:
                raise TimeoutError("Expected typed spam disconnect and owned client exit")
        protocol.require(observed.get("expected") is True and observed.get("translation") == "disconnect.spam", observed)
        protocol.require(ended.get("expected") is True, ended)
        self.process.wait(timeout=max(0.001, deadline - time.monotonic()))
        protocol.require(self.process.returncode == 0, f"Expected-disconnect client exited {self.process.returncode}")
        self.expected_disconnect = observed
        return {"disconnect": observed, "final_state": ended.get("state"), "client_exit_code": self.process.returncode}

    def close(self) -> None:
        try:
            if self.process.poll() is None:
                try:
                    self.request("quit", timeout=3)
                    self.process.wait(timeout=5)
                except (OSError, RuntimeError, TimeoutError, AssertionError, subprocess.TimeoutExpired):
                    self.forced_close = True
                    try:
                        os.killpg(self.process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    try:
                        self.process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        try:
                            os.killpg(self.process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        self.process.wait(timeout=5)
        finally:
            self.reader.join(timeout=2)
            self.stderr.close()
            if self.process.stdin is not None:
                self.process.stdin.close()
            if self.process.stdout is not None:
                self.process.stdout.close()


def run_client_scenarios(server: TestServer) -> list[dict]:
    """Record independent wire checks; a failed client produces failed rows, never skips."""
    from parity.suite import recorded, check_predicate

    client: Client | None = None
    startup_error: Exception | None = None
    try:
        client = Client(server)
    except Exception as error:
        startup_error = error

    def connected() -> Client:
        if startup_error is not None:
            raise RuntimeError(f"Could not start protocol client: {startup_error}") from startup_error
        protocol.require(client is not None, "Protocol client unavailable")
        client.ready()
        return client

    def login() -> dict:
        state = connected().state()
        protocol.require(state.get("login") is True and state.get("phase") == "play", state)
        protocol.require(state.get("protocol") == 777 and state.get("registry_packets", 0) > 0, state)
        return {"auth": "offline", "protocol": 777, "registry_packets": state["registry_packets"]}

    def chunks() -> dict:
        state = connected().wait_state(lambda state: state.get("chunks", 0) >= 4)
        return {"unique_chunk_coordinates": state["chunks"]}

    def inventory() -> dict:
        state = connected().wait_state(lambda state: state.get("inventory_packets", 0) > 0 and state.get("health") is not None)
        protocol.require(state.get("health") == 20 and state.get("food") == 20, f"Fresh player default health/food mismatch: {state.get('health')}/{state.get('food')}")
        return {key: state[key] for key in ("inventory_packets", "slot_packets", "health", "food")}

    def teleport() -> dict:
        active = connected()
        before = active.state()["teleport_confirmations"]
        server.command(f"tp {NAME} 4.5 201 4.5 90 0")

        def arrived(state: dict) -> bool:
            position = state.get("server_position")
            return isinstance(position, dict) and state.get("teleport_confirmations", 0) > before and all(
                abs(position.get(axis, float("inf")) - expected) < 0.01
                for axis, expected in (("x", 4.5), ("y", 201), ("z", 4.5), ("yaw", 90), ("pitch", 0)))

        state = active.wait_state(arrived)
        predicate = f"if entity @a[name={NAME},x=4.5,y=201,z=4.5,distance=..0.01]"
        check_predicate(server, predicate)
        return {"server_position": state["server_position"], "teleport_confirmations": state["teleport_confirmations"], "server_predicate": predicate}

    def movement() -> dict:
        active = connected()
        server.command(f"gamemode spectator {NAME}")
        position = active.state()["position"]
        protocol.require(isinstance(position, dict), "Missing client position")
        target = {axis: position[axis] for axis in ("x", "y", "z")}
        target["x"] += 0.25
        active.request("move", **target)
        predicate = f"if entity @a[name={NAME},x={target['x']},y={target['y']},z={target['z']},distance=..0.05]"
        wait_server_predicate(predicate)
        return {"sent_position": target, "server_predicate": predicate}

    def wait_server_predicate(predicate: str) -> None:
        deadline = time.monotonic() + 10
        while True:
            try:
                check_predicate(server, predicate)
                return
            except AssertionError:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(0.1)

    def command() -> dict:
        active = connected()
        server.command(f"op {NAME}")
        server.command("scoreboard objectives add wireprobe dummy")
        server.command("scoreboard players set Wire wireprobe 0")
        active.request("command", text="scoreboard players set Wire wireprobe 777")
        predicate = "if score Wire wireprobe matches 777"
        wait_server_predicate(predicate)
        return {"command": "scoreboard players set Wire wireprobe 777", "server_predicate": predicate}

    def command_tree() -> dict:
        active = connected()
        observed = active.wait_state(lambda state: isinstance(state.get("command_tree"), dict)
                                     and "scoreboard" in state["command_tree"].get("roots", []))
        protocol.require(observed.get("command_tree_error") is None, observed.get("command_tree_error"))
        tree = observed["command_tree"]
        protocol.require(tree["decoded_bytes"] == tree["payload_bytes"], "Command tree was not consumed completely")
        return {**tree, "commands_packets": observed["commands_packets"],
                "scope": "advertised syntax after operator setup; execution and permission behavior excluded"}

    def chat() -> dict:
        active = connected()
        token = "FERRALOOM_CHAT_" + uuid.uuid4().hex
        server.command("say " + token)
        server_message = active.wait_state(lambda state: delivered_chat(state, token) is not None)
        echo = "FERRALOOM_ECHO_" + uuid.uuid4().hex
        active.request("chat", text=echo)
        player_message = active.wait_state(lambda state: delivered_chat(state, echo) is not None)
        return {"scope": "offline unsigned message delivery; packet/rendering/signature parity unverified",
                "server_to_client": delivered_chat(server_message, token),
                "unsigned_offline_player_chat_echo": delivered_chat(player_message, echo)}

    scenarios = [
        ("network.play_login", ("network.login", "network.configuration", "network.registries"), login),
        ("network.chunk_stream", ("network.chunk_stream",), chunks),
        ("network.inventory_stream", ("network.inventory_stream",), inventory),
        ("network.teleport", ("players.teleport",), teleport),
        ("network.movement", ("network.movement",), movement),
        ("network.client_command", ("network.login",), command),
        ("network.command_tree", ("players.commands",), command_tree),
        ("network.chat_delivery", ("network.chat_delivery",), chat),
    ]
    try:
        return [recorded(identifier, features, action) for identifier, features, action in scenarios]
    finally:
        if client is not None:
            client.close()
