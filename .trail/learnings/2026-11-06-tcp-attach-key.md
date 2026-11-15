---
title: Loopback attach uses a per-run 0600 key and a mutual HMAC challenge
author: OpenCode
date: 2026-11-06
cost: { minutes: 120 }
branch: detached/db62f09226
touches:
  - crates/elephc-debug/src/dap.rs
  - crates/elephc-debug/src/lib.rs
  - src/debug_cli.rs
  - tests/codegen/cli.rs
  - docs/compiling/cli-reference.md
  - docs/compiling/dap.md
  - docs/beyond-php/debugging.md
  - .plans/native-debugger.md
---

## Decided

- A debug binary can expose one **loopback TCP** attach endpoint with:
  - `ELEPHC_DEBUG_ATTACH_ADDR=127.0.0.1:PORT` (PORT may be 0);
  - `ELEPHC_DEBUG_ATTACH_KEY=FILE`.
- The runtime generates a fresh 32-byte key, writes `FILE` (0600), the actual
  `FILE.addr` (0600) and `FILE.pid` (0600), then accepts loopback peers only.
  A non-loopback requested bind is refused before listening.
- Handshake (mutual, key never on the wire):
  - client -> server: `nonce_c` (32 random bytes);
  - server -> client: `nonce_s || HMAC(key, "S" || nonce_c || nonce_s)`;
  - client -> server: `HMAC(key, "C" || nonce_s || nonce_c)`;
  - server -> client: one status byte (`1` accepted, `0` rejected).
- A wrong key is **not fatal**: the rejected client gets `0` and the listener
  keeps accepting. A client that fails the server proof sends a one-byte abort
  so the listener does not sit on its 10s read timeout; the server then loops.
- `elephc debug --attach-tcp HOST:PORT --key FILE` connects, performs the same
  handshake, and then proxies DAP. It reads `FILE.pid` before connecting for
  pause. The Unix `--attach <socket>` path is untouched.
- The DAP transport was generalized from `BufReader<UnixStream>`/`UnixStream`
  to `Box<dyn BufRead + Send>`/`Box<dyn Write + Send>`, so launch, Unix attach
  and TCP attach share one proxy without duplicating the framing.
- HMAC-SHA256 is implemented with the existing `sha2` dependency in both the
  runtime and the compiler CLI. The two implementations are deliberately small
  and match the RFC 4231 vector; the debug crate does not depend on the probe
  crate.
- After a successful handshake the key/address/pid files are removed, so a
  later process cannot reuse the credential.

## Tried and ruled out

- Reusing the probe build key: `--debug` does not generate or embed a probe key,
  and the debug crate must not depend on `elephc-probe`. A per-run key written
  next to the listener is stronger for a local attach (it rotates every run).
- Sending the key itself over loopback: possession is the credential, but a
  local sniffer would then own it. The challenge proves possession without
  disclosing it.
- Making a wrong key kill the listener: an operator typo should not force a
  debuggee restart. The listener loops; the test proves the correct key still
  works afterwards.
- Letting the wrong-key client wait for the server's 10s read timeout: the test
  would need 10s between the wrong and correct attempts. The abort byte makes
  rejection immediate.
- Binding `0.0.0.0`: the listener rejects anything that is not loopback, and
  the CLI also rejects a non-loopback address.

## Watch out

- The key file's directory must exist; the runtime does not create it. Use a
  0700 directory so the 0600 key/address/pid files cannot be listed by others.
- `FILE.addr` is required when `PORT=0`; the test reads it. With an explicit
  non-zero port the file still exists for symmetry.
- The address file is written after bind; a client that reads it can connect
  immediately.
- The server's handshake has a 10s read/write timeout so an unauthenticated
  client cannot pin the debuggee forever; after success the timeouts are
  cleared for normal DAP blocking.
- The runtime removes the key/address/pid after the **successful** handshake,
  not after a rejection.
- The wrong-key diagnostic is `attach key rejected: server proof did not match`;
  a bad client proof yields `attach key rejected by the debuggee`.
- The e2e test starts the debuggee with `ELEPHC_DEBUG_DAP=1`, waits for
  `attach.key.addr`, checks the key mode is 0600, runs the wrong-key CLI, then
  the correct-key DAP session, asserts program output and file cleanup.
