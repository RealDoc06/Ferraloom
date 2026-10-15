#!/usr/bin/env python3
"""Five paired observable chat/command throttle cases on fresh offline connections.

Threshold one second = 20: the first unsigned trigger/chat effect precedes typed
disconnect.spam. Threshold zero: real effects and a subsequent positive client
command fence without disconnect. Operator command bypass, then deop enforcement.
No exact 200-tick boundary, separate-budget, frozen-decay, signed-chat/command,
signature/ack-validation, or retained-debt amount claim is made.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import uuid

from check_container_metadata import (Fixture, finish_rows, interaction_deadline, paired_main,
                                      typed_predicate, wait_predicate)
from parity import suite
from parity.bot import Client, delivered_chat
from parity.protocol import require

CASES = {f"chat_throttle.{case}": "network.chat_delivery" for case in (
    "first_command", "first_chat", "zero_command", "zero_chat", "operator_deop_command")}
DEFERRED = [{"id": "chat_throttle." + name, "result": "unverified", "reason": reason} for name, reason in (
    ("separate_budgets", "Without read-only production counter/tick observations, wall-time decay cannot establish independent counters. First-action threshold tests establish only per-route enforcement."),
    ("default_200_boundary", "Ten-message bursts and sleeps do not prove an exact boundary with listener-tick decay."),
    ("frozen_listener_decay", "No production read-only listener tick/counter instrumentation is available; game freeze does not freeze connection ticks."),
    ("signed_command", "Valid signatures and acknowledgments require an authentic session; no fabricated signature/ack evidence."),
    ("operator_debt_amount", "At a threshold of 20 the first post-deop action reaches the threshold by itself; this cannot establish how much operator debt was retained."),
    ("level_zero_operator", "Membership bypass at operator level zero remains unverified by this runner."),
    ("overflow_and_async_ordering", "Java integer overflow and existing unordered async handler/signature validation differences remain outside this bounded acceptance."))]


def offline_uuid(name: str) -> uuid.UUID:
    digest = bytearray(hashlib.md5(("OfflinePlayer:" + name).encode(), usedforsecurity=False).digest())
    digest[6] = (digest[6] & 15) | 48
    digest[8] = (digest[8] & 63) | 128
    return uuid.UUID(bytes=bytes(digest))


def membership(fixture: Fixture, client: Client, *, operator: bool = False) -> dict:
    """Read actual ops plus the entity UUID; no player command spends setup debt."""
    identity = offline_uuid(client.name)
    words = [int.from_bytes(identity.bytes[index:index + 4], "big", signed=True) for index in range(0, 16, 4)]
    observed = typed_predicate(fixture.server, f'if data entity {client.name} {{UUID:[I;{",".join(map(str, words))}]}}')
    path = fixture.server.directory / "ops.json"
    entries = json.loads(path.read_text())
    require(isinstance(entries, list), "ops.json is not a list")
    matches = [entry for entry in entries if entry.get("uuid") == str(identity)]
    require(bool(matches) == operator, {"uuid": str(identity), "operator_expected": operator, "ops": entries})
    return {"name": client.name, "uuid": str(identity), "entity_uuid": observed,
            "operator": operator, "ops": entries, "ops_sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def arm_trigger(fixture: Fixture, client: Client, value: int = 0) -> dict:
    server = fixture.server
    set_output = server.command(f"scoreboard players set {client.name} spamprobe {value}")
    enable_output = server.command(f"scoreboard players enable {client.name} spamprobe")
    # Semantic saved score readback proves a trigger objective and an unlocked
    # player score before the first action. Saving/console commands spend no debt.
    saved = suite.saved_scoreboard(server)
    objective = next((entry for entry in saved.get("Objectives", []) if entry.get("Name") == "spamprobe"), {})
    score = next((entry for entry in saved.get("PlayerScores", [])
                  if entry.get("Name") == client.name and entry.get("Objective") == "spamprobe"), {})
    require(objective.get("CriteriaName") == "trigger" and score.get("Score") == value and not score.get("Locked", False),
            {"objective": objective, "score": score})
    return {"console_set": set_output, "console_enable": enable_output, "saved_objective": objective, "saved_score": score}


def command_effect(fixture: Fixture, client: Client, value: int) -> dict:
    predicate = f"if score {client.name} spamprobe matches {value}"
    effect = wait_predicate(fixture.server, predicate)
    saved = suite.saved_scoreboard(fixture.server)
    score = next((entry for entry in saved.get("PlayerScores", [])
                  if entry.get("Name") == client.name and entry.get("Objective") == "spamprobe"), {})
    require(score.get("Score") == value and score.get("Locked") in (True, 1), score)
    return {"console_observed_effect": effect, "persisted_locked_score": score}


def positive_fence(fixture: Fixture, client: Client, value: int = 91) -> dict:
    armed = arm_trigger(fixture, client)
    sent = client.request("trigger", objective="spamprobe", value=value)
    effect = command_effect(fixture, client, value)
    token = "NO_SPAM_" + uuid.uuid4().hex
    fixture.server.command("say " + token)
    state = client.wait_state(lambda state: delivered_chat(state, token) is not None)
    require(state.get("disconnect") is None, state.get("disconnect"))
    spam_overlays = [entry for entry in state["chat_messages"] if entry.get("overlay")
                     and isinstance(entry.get("component_nbt"), dict)
                     and entry["component_nbt"].get("translate") == "disconnect.spam"]
    require(not spam_overlays, spam_overlays)
    return {"armed": armed, "sent": sent, "effect": effect, "received_marker": delivered_chat(state, token),
            "disconnect": state.get("disconnect"), "spam_overlays": spam_overlays}


def first_command(fixture: Fixture) -> dict:
    sender = fixture.connect("first-command", name="FirstCommand", metadata=False)
    identity = membership(fixture, sender)
    prepared = arm_trigger(fixture, sender)
    sender.expect_spam_disconnect()
    # This is the sender's FIRST command. No player command fence precedes it.
    sent = sender.request("trigger", objective="spamprobe", value=7)
    disconnected = sender.wait_spam_disconnect()
    effect = command_effect(fixture, sender, 7)
    fixture.close(sender)
    return {"identity": identity, "prepared": prepared, "first_action": sent,
            "effect_persisted_after_disconnect": effect, "intentional_disconnect": disconnected}


def first_chat(fixture: Fixture, observer: Client) -> dict:
    sender = fixture.connect("first-chat", name="FirstChat", metadata=False)
    identity = membership(fixture, sender)
    token = "FIRST_CHAT_" + uuid.uuid4().hex
    sender.expect_spam_disconnect()
    sent = sender.request("chat", text=token)
    observed = observer.wait_state(lambda state: delivered_chat(state, token) is not None)
    disconnected = sender.wait_spam_disconnect()
    # The sender's own received chat precedes its Disconnect on one TCP stream.
    # Cross-connection arrival times alone cannot establish server dispatch order.
    echoed = delivered_chat(disconnected["final_state"], token)
    require(echoed is not None and echoed["sequence"] < disconnected["disconnect"]["sequence"], disconnected)
    fixture.close(sender)
    return {"identity": identity, "first_action": sent, "second_connection_delivery": delivered_chat(observed, token),
            "sender_echo_before_disconnect": echoed, "intentional_disconnect": disconnected,
            "scope": "Fresh offline unsigned body broadcast and first-action kick; signatures are absent and unverified."}


def zero_command(fixture: Fixture) -> dict:
    sender = fixture.connect("zero-command", name="ZeroCommand", metadata=False)
    identity = membership(fixture, sender)
    prepared = arm_trigger(fixture, sender)
    sent = sender.request("trigger", objective="spamprobe", value=13)
    effect = command_effect(fixture, sender, 13)
    fence = positive_fence(fixture, sender)
    fixture.close(sender)
    return {"identity": identity, "prepared": prepared, "first_action": sent, "positive_effect": effect, "subsequent_positive_fence": fence}


def zero_chat(fixture: Fixture, observer: Client) -> dict:
    sender = fixture.connect("zero-chat", name="ZeroChat", metadata=False)
    identity = membership(fixture, sender)
    token = "ZERO_CHAT_" + uuid.uuid4().hex
    sent = sender.request("chat", text=token)
    observed = observer.wait_state(lambda state: delivered_chat(state, token) is not None)
    fence = positive_fence(fixture, sender)
    fixture.close(sender)
    return {"identity": identity, "first_action": sent, "second_connection_delivery": delivered_chat(observed, token),
            "subsequent_positive_fence": fence}


def operator_deop(fixture: Fixture) -> dict:
    sender = fixture.connect("operator-deop", name="BypassOperator", metadata=False)
    nonoperator = membership(fixture, sender)
    fixture.server.command(f"op {sender.name}")
    operator = membership(fixture, sender, operator=True)
    prepared = arm_trigger(fixture, sender)
    sent_operator = sender.request("trigger", objective="spamprobe", value=21)
    operator_effect = command_effect(fixture, sender, 21)
    # A server marker positively proves the connection is alive after the effect.
    token = "OP_ALIVE_" + uuid.uuid4().hex
    fixture.server.command("say " + token)
    state = sender.wait_state(lambda state: delivered_chat(state, token) is not None)
    require(state.get("disconnect") is None, state)
    fixture.server.command(f"deop {sender.name}")
    deopped = membership(fixture, sender)
    rearmed = arm_trigger(fixture, sender)
    sender.expect_spam_disconnect()
    sent_deopped = sender.request("trigger", objective="spamprobe", value=22)
    disconnected = sender.wait_spam_disconnect()
    deopped_effect = command_effect(fixture, sender, 22)
    fixture.close(sender)
    return {"initial_nonoperator": nonoperator, "operator": operator, "prepared": prepared,
            "operator_action": sent_operator, "operator_effect": operator_effect, "operator_alive_marker": delivered_chat(state, token),
            "deopped": deopped, "rearmed": rearmed, "first_post_deop_action": sent_deopped,
            "post_deop_effect_persisted": deopped_effect, "intentional_disconnect": disconnected,
            "scope": "Membership bypass then enforcement after deop. Threshold=20 does not measure retained operator debt."}


def configure(fixture: Fixture, seconds: int) -> None:
    server = fixture.server
    server.configure()
    (server.directory / "ops.json").write_text("[]\n")
    if server.kind == "vanilla":
        with (server.directory / "server.properties").open("a") as target:
            target.write(f"command-spam-threshold-seconds={seconds}\nchat-spam-threshold-seconds={seconds}\n")
    else:
        # These exact fields belong to pinned AntiSpamConfig and are accepted by
        # the wrapper's strict unknown-key check. No permissive advanced file.
        with (server.directory / "pumpkin.toml").open("a") as target:
            target.write('[chat.anti_spam]\nenabled = true\nspam_threshold = 200\n'
                         f'chat_spam_threshold_seconds = {seconds}\ncommand_spam_threshold_seconds = {seconds}\n'
                         'message_cost = 20\ndecay_per_tick = 1\nops_bypass = true\n')


def run_side(kind: str, directory: Path, args: argparse.Namespace, report: dict) -> list[dict]:
    results: dict[str, dict] = {}
    errors = []
    for seconds, profile in ((1, "first-threshold"), (0, "disabled-threshold")):
        # Each profile gets a fresh world and listening server, sequentially.
        fixture = Fixture(kind + "/" + profile, directory / profile, args, report)
        fixture.server.kind = kind
        server = fixture.server
        try:
            configure(fixture, seconds)
            fixture.start(profile)
            report.setdefault("effective_settings", {})[kind + "/" + profile] = {
                "chat_threshold_seconds": seconds, "command_threshold_seconds": seconds,
                "threshold_ticks": seconds * 20, "message_cost": 20, "decay_per_listener_tick": 1,
                "configuration_source": "official DedicatedServerProperties bytecode and pinned AntiSpamConfig; production defaults otherwise"}
            server.command("scoreboard objectives add spamprobe trigger")
            observer = fixture.connect("observer", name="SpamObserver", metadata=False)
            membership(fixture, observer)
            actions = (("first_command", lambda: first_command(fixture)),
                       ("first_chat", lambda: first_chat(fixture, observer)),
                       ("operator_deop_command", lambda: operator_deop(fixture))) if seconds else (
                       ("zero_command", lambda: zero_command(fixture)), ("zero_chat", lambda: zero_chat(fixture, observer)))
            for suffix, action in actions:
                identifier = "chat_throttle." + suffix
                def guarded(action=action, identifier=identifier) -> dict:
                    with interaction_deadline(server, identifier):
                        return action()
                results[identifier] = suite.recorded(identifier, (CASES[identifier],), guarded)
                require(not server.broken, server.broken)
                require(not any(client.failure for client in fixture.clients), "Unexpected client failure")
            fixture.close(observer)
            server.save_flush()
            fixture.stop()
            require(suite.data_version(server.directory / "world") == 5023, "World DataVersion differs from 5023")
        except Exception as error:
            errors.append(f"{profile}: {type(error).__name__}: {error}")
        finally:
            errors.extend(fixture.cleanup())
    report.setdefault("runtime_errors", {})[kind] = errors
    return finish_rows(CASES, results, errors)


if __name__ == "__main__":
    raise SystemExit(paired_main("chat-throttle", CASES, run_side, __doc__, DEFERRED))
