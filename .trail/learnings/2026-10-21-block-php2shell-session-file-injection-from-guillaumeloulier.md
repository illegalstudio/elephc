---
title: Block php2shell session-file injection from upload progress
author: Guillaumeloulier
date: 2026-10-21
branch: audit/php2shell
cost: { minutes: 45 }
touches:
  - crates/elephc-web/src/session/upload_progress.rs
  - docs/php/sessions.md
  - docs/specs/session-support.md
  - tests/web_session_tests.rs
  - src/resolver/include_path.rs
  - src/codegen_support/runtime/system/unserialize/validator_aarch64.rs
---

Audited elephc against the php2shell/CVE-2026-109618+109619 chains (session.upload_progress). Reproduced a real pre-auth RCE primitive that is stronger than the writeup filename path: the client-controlled PHP_SESSION_UPLOAD_PROGRESS value is assembled into a $_SESSION key and written by set_entry_php as `key|value`, so a key containing `|O:4:"Evil":...` appends attacker serialized data; the next session_start() unserializes it and runs __wakeup/__destruct (verified with a compiled web app and marker files, default php handler, no LFI, no handler mismatch).

Ruled out as non-applicable: runtime-dynamic include is a compile error (src/resolver/include_path.rs), so the LFI leg is blocked; unserialize() validates the full grammar before allocating (src/codegen_support/runtime/system/unserialize/validator_aarch64.rs), so malformed input returns false without constructing objects or running destructors (var_destroy/__destruct-on-failure chains do not apply); upload_progress metadata is owned Rust data, no cached zval* buckets; SERIALIZE_HANDLER is reset per request (verified `ini_get` returns the env default after an option switch); body-supplied PHPSESSID is gated on use_only_cookies=0 in both __elephc_session_start_core and upload_progress::begin; sid is charset-validated before any path join; uploaded files use tempnam, not the client filename.

Fix in crates/elephc-web/src/session/upload_progress.rs: ready_to_write() rejects keys containing `|` or NUL (and php_binary keys >127), and session_wire_safe() percent-encodes `|`/NUL in progress filename/field_name so a php_serialize file read by a php handler cannot smuggle an entry. Do not "fix" extract_quoted to accept escaped quotes without keeping this sanitization: PHP accepts `filename="...\\"..."`, and once parsing is PHP-compatible that becomes a second injection vector.
