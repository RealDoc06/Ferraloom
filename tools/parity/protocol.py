"""Bounded Java status protocol probes shared by the live compatibility runner."""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
import json
import socket
import struct
import time

MAX_PACKET = 2 * 1024 * 1024


def require(condition: bool, detail: object) -> None:
    if not condition:
        raise AssertionError(detail)


def varint(value: int) -> bytes:
    value &= 0xFFFFFFFF
    result = bytearray()
    while value >= 128:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return bytes(result)


def read_exact(sock: socket.socket, length: int) -> bytes:
    if not 0 <= length <= MAX_PACKET:
        raise ValueError(f"Invalid packet length {length}")
    result = bytearray()
    original_timeout = sock.gettimeout()
    deadline = time.monotonic() + 5
    try:
        while len(result) < length:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("Frame read deadline exceeded")
            sock.settimeout(min(remaining, original_timeout or 5))
            chunk = sock.recv(length - len(result))
            if not chunk:
                raise EOFError("Connection closed")
            result.extend(chunk)
    finally:
        sock.settimeout(original_timeout)
    return bytes(result)


def read_varint(sock: socket.socket) -> int:
    value = 0
    for i in range(5):
        byte = read_exact(sock, 1)[0]
        value |= (byte & 127) << (i * 7)
        if not byte & 128:
            if i == 4 and byte > 15:
                raise ValueError("VarInt overflow")
            return value
    raise ValueError("VarInt exceeds five bytes")


def packet(data: bytes) -> bytes:
    return varint(len(data)) + data


def handshake(port: int, protocol: int = 777, intent: int = 1) -> bytes:
    host = b"localhost"
    return packet(b"\x00" + varint(protocol) + varint(len(host)) + host + struct.pack(">H", port) + varint(intent))


def connect(port: int) -> socket.socket:
    sock = socket.create_connection(("127.0.0.1", port), timeout=5)
    sock.settimeout(5)
    return sock


def status(port: int, *, fragmented: bool = False, client_protocol: int = 777) -> dict:
    with connect(port) as sock:
        data = handshake(port, client_protocol) + packet(b"\x00")
        if fragmented:
            for byte in data:
                sock.sendall(bytes([byte]))
        else:
            sock.sendall(data)
        length = read_varint(sock)
        response = read_exact(sock, length)
        # Parse from a bounded memory view rather than accidentally consuming a second frame.
        if response[0] != 0:
            raise ValueError("Expected status response")
        size, offset = 0, 1
        for i in range(5):
            byte = response[offset]
            offset += 1
            size |= (byte & 127) << (i * 7)
            if not byte & 128:
                break
        else:
            raise ValueError("Invalid JSON length")
        if size != len(response) - offset:
            raise ValueError("Status string length mismatch")
        return json.loads(response[offset:])


def status_fields(port: int) -> dict:
    result = status(port)
    require(result["version"]["protocol"] == 777, result.get("version"))
    require(type(result["players"]["online"]) is int, "Invalid online player count")
    require(type(result["players"]["max"]) is int, "Invalid maximum player count")
    require("description" in result, "Missing server description")
    return {"protocol": result["version"]["protocol"], "version": result["version"]["name"]}


def status_other_version(port: int) -> dict:
    result = status(port, client_protocol=776)
    require(result["version"]["protocol"] == 777, f"Status echoed client's unsupported protocol: {result['version']}")
    return {"client_protocol": 776, "advertised_protocol": result["version"]["protocol"]}


def incompatible_login(port: int) -> dict:
    with connect(port) as sock:
        name = b"FerraloomWrong"
        sock.sendall(handshake(port, 776, 2) + packet(b"\x00" + varint(len(name)) + name + bytes(16)))
        response = read_exact(sock, read_varint(sock))
        require(response and response[0] == 0, "Unsupported login did not receive login disconnect")
        reason = response[1:].decode("utf-8", errors="replace")
        require(any(key in reason.lower() for key in ("outdated", "incompatible")), f"Unexpected login rejection: {reason}")
    return {"client_protocol": 776, "version_rejection": True}


def ping(port: int) -> dict:
    payload = struct.pack(">q", -918273645546372819)
    with connect(port) as sock:
        sock.sendall(handshake(port) + packet(b"\x01" + payload))
        response = read_exact(sock, read_varint(sock))
        require(response == b"\x01" + payload, response.hex())
        closed = not sock.recv(1)
        require(closed, "Vanilla closes status connection after ping")
    return {"signed_payload_echoed": True, "closed_after_ping": True}


def duplicate_status(port: int) -> dict:
    with connect(port) as sock:
        sock.sendall(handshake(port) + packet(b"\x00"))
        read_exact(sock, read_varint(sock))
        sock.sendall(packet(b"\x00"))
        require(not sock.recv(1), "Second status request should disconnect")
    return {"duplicate_disconnected": True}


def fragmented_status(port: int) -> dict:
    require(status(port, fragmented=True)["version"]["protocol"] == 777, "Fragmented status failed")
    return {"split_frame_accepted": True}


def concurrent_status(port: int) -> dict:
    with ThreadPoolExecutor(max_workers=12) as executor:
        results = list(executor.map(lambda _: status(port), range(24)))
    require(all(result["version"]["protocol"] == 777 for result in results), "Concurrent status failed")
    return {"requests": len(results), "concurrency": 12}


def malformed_length(port: int) -> dict:
    with connect(port) as sock:
        sock.sendall(b"\x80" * 6)
        try:
            require(not sock.recv(1), "Overlong length VarInt should disconnect")
        except ConnectionResetError:
            pass
    status_fields(port)
    return {"malformed_disconnected": True, "server_alive": True}
