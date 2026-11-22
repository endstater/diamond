# Uppercase selection

This is a complete Diamond protocol v1 plugin using Python 3's standard library. It illustrates that the native application does not require JavaScript plugins or embed a plugin language runtime.

Explicitly load `manifest.json` using the application's plugin launcher, inspect its program and capabilities, and launch it. The program receives only a selection explicitly supplied by the host. `uppercase` returns the transformed text as a proposed replacement; the host checks the target document before applying it.

Run a wire-level smoke test directly:

```sh
python3 -u plugin.py <<'JSON'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol_version":1}}
{"jsonrpc":"2.0","id":2,"method":"commands/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"commands/invoke","params":{"command":"uppercase","selection":"Привет, Diamond"}}
JSON
```

The subprocess runs with your usual OS permissions. Protocol capabilities govern host communication; they are not a sandbox.
