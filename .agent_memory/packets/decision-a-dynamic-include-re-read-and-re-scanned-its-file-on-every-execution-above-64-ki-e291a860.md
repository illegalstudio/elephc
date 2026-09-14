---
type: "Decision"
title: "A dynamic include re-read and re-scanned its file on every execution; above 64 KiB it also re-parsed"
description: "Before the runtime script cache, every runtime include through the eval bridge paid, EVERY TIME: std::fs::read of the whole file, a windows 5 / windows 2 byte scan to split <?php / ? blocks, and a HashMap<Vec<u8 lookup t"
resource: "crates/elephc-magician/src/interpreter/include_exec.rs"
tags: ["session-learning", "opcache", "script-cache", "eval-bridge", "include", "performance"]
timestamp: "2026-09-11T15:09:22.955Z"
x-kage-id: "repo:sparkling-jingling-flute:decision:a-dynamic-include-re-read-and-re-scanned-its-file-on-every-execution-above-64-ki"
x-kage-type: "decision"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/include_exec.rs", "crates/elephc-magician/src/parse_cache.rs", "crates/elephc-magician/src/script_cache/store.rs", "src/resolver/include_path.rs"]
x-kage-stack: ["rust", "php"]
---

# A dynamic include re-read and re-scanned its file on every execution; above 64 KiB it also re-parsed

> Before the runtime script cache, every runtime include through the eval bridge paid, EVERY TIME: std::fs::read of the…

Before the runtime script cache, every runtime `include` through the eval bridge paid, EVERY TIME: `std::fs::read` of the whole file, a `windows(5)`/`windows(2)` byte scan to split `<?php`/`?>` blocks, and a `HashMap<Vec<u8>>` lookup that SipHashed the block bytes. Only the parse was memoized, and only below `parse_cache::MAX_CACHEABLE_FRAGMENT_BYTES` (64 KiB) — above that ceiling the file was re-lexed and re-parsed on every single include.

MEASURED on macOS arm64, same interpreted work in every row, only the included file's size varying (padding inside a PHP comment): 52 B = 263 us, 4 KB = 345 us, 32 KB = 1232 us, 64 KB = 1580 us, 128 KB = 10979 us, 512 KB = 45161 us. Below the cap the slope is ~20.6 ns per source byte on top of a ~47 us floor; crossing the cap costs 7x for a doubling of size.

With a path-keyed, mtime-validated script cache the warm cost becomes FLAT in file size: 64 KB = 42.6 us (37x), 128 KB = 66.5 us (165x, and that number is still dominated by amortizing the single cold miss).

Two consequences worth keeping: (1) the 64 KiB constant was a silent performance cliff that `opcache.max_file_size` now replaces with the directive reference PHP uses for the same decision; (2) a runtime-dynamic include is a hard COMPILE ERROR at AOT top level (`src/resolver/include_path.rs:50`), so this whole tier is reachable ONLY from inside `eval()` — which is where Symfony templates land.
Evidence: Benchmark harness in the session scratchpad: an eval-hosted loop including one file N times, compiled twice (default CLI = cache off, `--ini opcache.enable_cli=1` = cache on). Numbers above are from those runs.
Verified by: Compiled probes run on macOS arm64; differential output check against `php -n` 8.5.6 came back IDENTICAL for a fixture exercising mixed literal/code, tagless, unclosed-block, returning and non-returning includes.

## Verification

Benchmark harness in the session scratchpad: an eval-hosted loop including one file N times, compiled twice (default CLI = cache off, `--ini opcache.enable_cli=1` = cache on). Numbers above are from those runs.

# Citations

[1] explicit_capture (2026-09-11T15:09:22.955Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:decision:a-dynamic-include-re-read-and-re-scanned-its-file-on-every-execution-above-64-ki","title":"A dynamic include re-read and re-scanned its file on every execution; above 64 KiB it also re-parsed","summary":"Before the runtime script cache, every runtime include through the eval bridge paid, EVERY TIME: std::fs::read of the whole file, a windows 5 / windows 2 byte scan to split <?php / ? blocks, and a HashMap<Vec<u8 lookup t","body":"Before the runtime script cache, every runtime `include` through the eval bridge paid, EVERY TIME: `std::fs::read` of the whole file, a `windows(5)`/`windows(2)` byte scan to split `<?php`/`?>` blocks, and a `HashMap<Vec<u8>>` lookup that SipHashed the block bytes. Only the parse was memoized, and only below `parse_cache::MAX_CACHEABLE_FRAGMENT_BYTES` (64 KiB) — above that ceiling the file was re-lexed and re-parsed on every single include.\n\nMEASURED on macOS arm64, same interpreted work in every row, only the included file's size varying (padding inside a PHP comment): 52 B = 263 us, 4 KB = 345 us, 32 KB = 1232 us, 64 KB = 1580 us, 128 KB = 10979 us, 512 KB = 45161 us. Below the cap the slope is ~20.6 ns per source byte on top of a ~47 us floor; crossing the cap costs 7x for a doubling of size.\n\nWith a path-keyed, mtime-validated script cache the warm cost becomes FLAT in file size: 64 KB = 42.6 us (37x), 128 KB = 66.5 us (165x, and that number is still dominated by amortizing the single cold miss).\n\nTwo consequences worth keeping: (1) the 64 KiB constant was a silent performance cliff that `opcache.max_file_size` now replaces with the directive reference PHP uses for the same decision; (2) a runtime-dynamic include is a hard COMPILE ERROR at AOT top level (`src/resolver/include_path.rs:50`), so this whole tier is reachable ONLY from inside `eval()` — which is where Symfony templates land.\nEvidence: Benchmark harness in the session scratchpad: an eval-hosted loop including one file N times, compiled twice (default CLI = cache off, `--ini opcache.enable_cli=1` = cache on). Numbers above are from those runs.\nVerified by: Compiled probes run on macOS arm64; differential output check against `php -n` 8.5.6 came back IDENTICAL for a fixture exercising mixed literal/code, tagless, unclosed-block, returning and non-returning includes.","type":"decision","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","opcache","script-cache","eval-bridge","include","performance"],"paths":["crates/elephc-magician/src/interpreter/include_exec.rs","crates/elephc-magician/src/parse_cache.rs","crates/elephc-magician/src/script_cache/store.rs","src/resolver/include_path.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T15:09:22.955Z"}],"context":{"fact":"Before the runtime script cache, every runtime `include` through the eval bridge paid, EVERY TIME: `std::fs::read` of the whole file, a `windows(5)`/`windows(2)` byte scan to split `<?php`/`?>` blocks, and a `HashMap<Vec<u8>>` lookup that SipHashed the block bytes. Only the parse was memoized, and only below `parse_cache::MAX_CACHEABLE_FRAGMENT_BYTES` (64 KiB) — above that ceiling the file was re-lexed and re-parsed on every single include.","verification":"Benchmark harness in the session scratchpad: an eval-hosted loop including one file N times, compiled twice (default CLI = cache off, `--ini opcache.enable_cli=1` = cache on). Numbers above are from those runs."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T15:09:22.955Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/include_exec.rs","sha256":"a32d5c16bc3e9f65f3f2b7375a83cde50e5c0f49c0f829e8b35e352367b8389e","size":7129},{"path":"crates/elephc-magician/src/parse_cache.rs","sha256":"a746020d8ac0f847332f7e799f1f7ac80ae93ccb9c024be68a874e2174982e6e","size":5806},{"path":"crates/elephc-magician/src/script_cache/store.rs","sha256":"55b1b6d750069775aae459ba6ae7266db1c46250abf9cca11abbdf6cee0807d4","size":14452},{"path":"src/resolver/include_path.rs","sha256":"9d420b303af2342d20276e7b25caf0b0fed8e9e217cd081a47ea06acb9c4ad06","size":5875}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":4000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":465,"stale":true,"stale_reasons":["linked path changed since memory was verified: crates/elephc-magician/src/script_cache/store.rs"],"suggested_action":"update"},"created_at":"2026-09-11T15:09:22.955Z","updated_at":"2026-09-12T18:12:29.318Z","author_branch":"feat/opcache-runtime-cache"}
```

