#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Smoke-test the bundled plugin using only Python's standard library.

Run from any directory: python3 /path/to/diamond/scripts/check_plugin_example.py
Every response has a three-second deadline. No Rust toolchain is required.
"""

import json
from pathlib import Path
import queue
import subprocess
import sys
import threading


MAX_MESSAGE_BYTES = 1024 * 1024
RESPONSE_TIMEOUT = 3.0


class ProtocolFailure(RuntimeError):
    """An invalid response, timeout, or failed example assertion."""


def read_responses(stream, replies, overflowed):
    """A daemon reader makes pipe reads portable and keeps the deadline bounded."""
    while True:
        try:
            line = stream.readline(MAX_MESSAGE_BYTES + 1)
            if not line:
                raise ProtocolFailure("plugin closed stdout before a response")
            if len(line) > MAX_MESSAGE_BYTES or not line.endswith(b"\n"):
                raise ProtocolFailure("plugin emitted an oversized or incomplete line")
            reply = json.loads(line.decode("utf-8"))
        except (OSError, UnicodeError, ValueError, ProtocolFailure) as error:
            try:
                replies.put_nowait(error)
            except queue.Full:
                overflowed.set()
            return
        try:
            replies.put_nowait(reply)
        except queue.Full:
            overflowed.set()
            return


def require(condition, message):
    if not condition:
        raise ProtocolFailure(message)


def exchange(process, replies, overflowed, request_id, method, params):
    if overflowed.is_set():
        raise ProtocolFailure("plugin emitted excessive output")
    request = {
        "jsonrpc": "2.0",
        "id": request_id,
        "method": method,
        "params": params,
    }
    # All four fixed test requests together fit comfortably within the smallest
    # supported pipe buffer. Only pipe reads need a worker for this smoke test.
    process.stdin.write(json.dumps(request, ensure_ascii=False).encode("utf-8") + b"\n")
    process.stdin.flush()
    try:
        response = replies.get(timeout=RESPONSE_TIMEOUT)
    except queue.Empty as error:
        raise ProtocolFailure(f"{method}: no response within three seconds") from error
    if isinstance(response, BaseException):
        raise ProtocolFailure(f"{method}: {response}") from response
    require(not overflowed.is_set(), "plugin emitted excessive output")
    require(isinstance(response, dict), f"{method}: response must be an object")
    require(response.get("jsonrpc") == "2.0", f"{method}: wrong JSON-RPC version")
    require(
        type(response.get("id")) is int and response["id"] == request_id,
        f"{method}: wrong response id",
    )
    require(
        ("result" in response) != ("error" in response),
        f"{method}: expected exactly one result or error",
    )
    return response


def check_example():
    project = Path(__file__).resolve().parents[1]
    example = project / "examples" / "plugins" / "uppercase" / "plugin.py"
    replies = queue.Queue(maxsize=4)
    overflowed = threading.Event()
    process = subprocess.Popen(
        [sys.executable, "-u", str(example)],
        cwd=example.parent,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    reader = threading.Thread(
        target=read_responses,
        args=(process.stdout, replies, overflowed),
        daemon=True,
    )
    reader.start()
    try:
        initialized = exchange(
            process,
            replies,
            overflowed,
            1,
            "initialize",
            {
                "protocol_version": 1,
                "capabilities": ["selection_read", "selection_replace"],
            },
        )
        require(
            initialized.get("result") == {"protocol_version": 1},
            "initialize: wrong protocol version result",
        )

        commands = exchange(process, replies, overflowed, 2, "commands/list", {})
        require(
            commands.get("result")
            == {"commands": [{"id": "uppercase", "title": "Uppercase selection"}]},
            "commands/list: expected the uppercase command",
        )

        selected_text = "Привет, Diamond 🦀 Straße"
        transformed = exchange(
            process,
            replies,
            overflowed,
            3,
            "commands/invoke",
            {"command": "uppercase", "selection": selected_text},
        )
        result = transformed.get("result")
        require(isinstance(result, dict), "commands/invoke: expected an object result")
        require(
            result.get("replacement") == selected_text.upper(),
            "commands/invoke: Unicode transformation did not match",
        )
        require(
            isinstance(result.get("message"), str),
            "commands/invoke: expected a status message",
        )

        rejected = exchange(process, replies, overflowed, 4, "unknown/method", {})
        error = rejected.get("error")
        require(isinstance(error, dict), "unknown method: expected an error object")
        require(
            type(error.get("code")) is int and isinstance(error.get("message"), str),
            "unknown method: expected an integer code and a message",
        )

        process.stdin.close()
        try:
            returncode = process.wait(timeout=RESPONSE_TIMEOUT)
        except subprocess.TimeoutExpired as error:
            raise ProtocolFailure("plugin did not exit after stdin closed") from error
        require(returncode == 0, f"plugin exited with status {returncode}")
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=RESPONSE_TIMEOUT)
        if not process.stdin.closed:
            process.stdin.close()
        reader.join(timeout=RESPONSE_TIMEOUT)
        process.stdout.close()


def main():
    try:
        check_example()
    except (OSError, ProtocolFailure, subprocess.TimeoutExpired) as error:
        print(f"FAIL: plugin example: {error}", file=sys.stderr)
        return 1
    print("PASS: plugin example (initialize, commands/list, Unicode invoke, unknown method)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
