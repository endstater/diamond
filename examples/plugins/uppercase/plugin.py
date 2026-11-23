#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Example Diamond protocol v1 plugin; no packages or network access required."""

import json
import sys

MAX_MESSAGE_BYTES = 1024 * 1024


def dispatch(method, params):
    if method == "initialize":
        if params.get("protocol_version") != 1:
            raise ValueError("Unsupported protocol version")
        return {"protocol_version": 1}
    if method == "commands/list":
        return {"commands": [{"id": "uppercase", "title": "Uppercase selection"}]}
    if method == "commands/invoke" and params.get("command") == "uppercase":
        selection = params.get("selection")
        if not isinstance(selection, str):
            raise ValueError("Select text and grant selection_read first")
        return {
            "replacement": selection.upper(),
            "message": "Selection converted to uppercase",
        }
    raise ValueError("Unknown method or command")


def main():
    while True:
        line = sys.stdin.buffer.readline(MAX_MESSAGE_BYTES + 1)
        if not line:
            return
        if len(line) > MAX_MESSAGE_BYTES or not line.endswith(b"\n"):
            return
        request_id = None
        try:
            request = json.loads(line)
            request_id = request["id"]
            if request.get("jsonrpc") != "2.0":
                raise ValueError("Expected JSON-RPC 2.0")
            response = {
                "jsonrpc": "2.0",
                "id": request_id,
                "result": dispatch(request["method"], request.get("params", {})),
            }
        except (ValueError, KeyError, TypeError) as error:
            response = {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32602, "message": str(error)},
            }
        print(json.dumps(response, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
