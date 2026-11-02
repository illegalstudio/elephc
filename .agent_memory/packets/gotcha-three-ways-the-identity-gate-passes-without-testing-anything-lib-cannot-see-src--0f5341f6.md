---
type: "Gotcha"
title: "Three ways the identity gate passes without testing anything: --lib cannot see src/linker or src/pipeline, --emit-asm writes a file rather than stdout, and nm on a stripped binary sees 222 symbols"
description: "Each of these produced a confident green that meant nothing, on 2026 09 05. 1. cargo test p elephc lib DOES NOT COMPILE src/linker/ or src/pipeline/ . src/lib.rs and src/main.rs declare overlapping but different module t"
resource: "src/lib.rs"
tags: ["session-learning", "testing", "identity-gate", "false-green", "cargo", "emit-asm"]
timestamp: "2026-09-05T12:04:43.060Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:three-ways-the-identity-gate-passes-without-testing-anything-lib-cannot-see-src-"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/lib.rs", "src/main.rs", "src/linker/mod.rs", "src/pipeline/backend.rs"]
x-kage-stack: ["rust", "cargo"]
---

# Three ways the identity gate passes without testing anything: --lib cannot see src/linker or src/pipeline, --emit-asm writes a file rather than stdout, and nm on a stripped binary sees 222 symbols

> Each of these produced a confident green that meant nothing, on 2026 09 05. 1. cargo test p elephc lib DOES NOT COMPI…

Each of these produced a confident green that meant nothing, on 2026-09-05.

1. `cargo test -p elephc --lib` DOES NOT COMPILE `src/linker/**` or `src/pipeline/**`. src/lib.rs and src/main.rs declare overlapping but different module trees, and `mod linker`, `mod pipeline`, `mod cli`, `mod link_plan` exist only in main.rs. Running `-p elephc --lib linker::` against a patch that adds a whole new file under src/linker prints "Finished test profile" and "0 passed; 1655 filtered out" — the lib target never saw a line of it, and `--list` on the test binary confirms none of its tests are present. Use `cargo test -j 2 -p elephc --bins <filter>` for bin-only modules; `--lib` genuinely does test ir, ir_lower, ir_passes, optimize, codegen_support, types and runtime_cache. A corollary worth knowing: a failure appearing in BOTH --lib and --bins for a patch under src/linker cannot have been caused by that patch, because --lib does not contain it — that is how 19 red tests were attributed to the branch rather than to the patch.

2. `elephc --emit-asm foo.php` WRITES THE ASSEMBLY TO `foo.s` and prints a one-line confirmation on stdout. The runbook recipe `elephc --emit-asm probe.php > base.s` therefore captures 96 bytes of "Emitted assembly ... -> ...", not 34,949,666 bytes of assembly, and every `cmp` built on it is trivially green. Copy the emitted .s after each run instead.

3. `nm -n` on a default-built binary is nearly empty because the binary is stripped: freshprobe2 has 222 symbols by default and 68,372 with `--keep-symbols`. An nm-shape gate must run on a --keep-symbols build or it proves nothing.

Also: `size -m` prints `Section __text:` / `Section __data:`, not the `__TEXT, __text` form some recipes grep for, so the section-size check silently matches nothing.

With the recipe corrected, the gate is genuinely sound on a release compiler: two consecutive `--emit-asm` runs of examples/symfony-app/freshprobe2.php produce byte-identical 34,949,666-byte assembly with identical stderr.
Evidence: `cargo test -p elephc --bins linker::` ran 49 tests including 11 asm_split ones; the same filter under --lib ran 0 of 1655. `--list` on target/debug/deps/elephc-*.rs showed no asm_split test. `wc -c` on the redirected stdout gave 96 vs 34,949,666 for the emitted file. `wc -l` on nm output: 222 stripped vs 68,372 with --keep-symbols. grep 'Section __text' matched where '__TEXT, __text' did not. cmp of two consecutive emissions: exit 0.
Verified by: cargo test -p elephc --lib/--bins with and without filters, --list on the test binary, wc -c/-l, cmp, size -m

## Verification

`cargo test -p elephc --bins linker::` ran 49 tests including 11 asm_split ones; the same filter under --lib ran 0 of 1655. `--list` on target/debug/deps/elephc-*.rs showed no asm_split test. `wc -c` on the redirected stdout gave 96 vs 34,949,666 for the emitted file. `wc -l` on nm output: 222 stripped vs 68,372 with --keep-symbols. grep 'Section __text' matched where '__TEXT, __text' did not. cmp of two consecutive emissions: exit 0.

# Citations

[1] explicit_capture (2026-09-05T12:04:43.060Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:three-ways-the-identity-gate-passes-without-testing-anything-lib-cannot-see-src-","title":"Three ways the identity gate passes without testing anything: --lib cannot see src/linker or src/pipeline, --emit-asm writes a file rather than stdout, and nm on a stripped binary sees 222 symbols","summary":"Each of these produced a confident green that meant nothing, on 2026 09 05. 1. cargo test p elephc lib DOES NOT COMPILE src/linker/ or src/pipeline/ . src/lib.rs and src/main.rs declare overlapping but different module t","body":"Each of these produced a confident green that meant nothing, on 2026-09-05.\n\n1. `cargo test -p elephc --lib` DOES NOT COMPILE `src/linker/**` or `src/pipeline/**`. src/lib.rs and src/main.rs declare overlapping but different module trees, and `mod linker`, `mod pipeline`, `mod cli`, `mod link_plan` exist only in main.rs. Running `-p elephc --lib linker::` against a patch that adds a whole new file under src/linker prints \"Finished test profile\" and \"0 passed; 1655 filtered out\" — the lib target never saw a line of it, and `--list` on the test binary confirms none of its tests are present. Use `cargo test -j 2 -p elephc --bins <filter>` for bin-only modules; `--lib` genuinely does test ir, ir_lower, ir_passes, optimize, codegen_support, types and runtime_cache. A corollary worth knowing: a failure appearing in BOTH --lib and --bins for a patch under src/linker cannot have been caused by that patch, because --lib does not contain it — that is how 19 red tests were attributed to the branch rather than to the patch.\n\n2. `elephc --emit-asm foo.php` WRITES THE ASSEMBLY TO `foo.s` and prints a one-line confirmation on stdout. The runbook recipe `elephc --emit-asm probe.php > base.s` therefore captures 96 bytes of \"Emitted assembly ... -> ...\", not 34,949,666 bytes of assembly, and every `cmp` built on it is trivially green. Copy the emitted .s after each run instead.\n\n3. `nm -n` on a default-built binary is nearly empty because the binary is stripped: freshprobe2 has 222 symbols by default and 68,372 with `--keep-symbols`. An nm-shape gate must run on a --keep-symbols build or it proves nothing.\n\nAlso: `size -m` prints `Section __text:` / `Section __data:`, not the `__TEXT, __text` form some recipes grep for, so the section-size check silently matches nothing.\n\nWith the recipe corrected, the gate is genuinely sound on a release compiler: two consecutive `--emit-asm` runs of examples/symfony-app/freshprobe2.php produce byte-identical 34,949,666-byte assembly with identical stderr.\nEvidence: `cargo test -p elephc --bins linker::` ran 49 tests including 11 asm_split ones; the same filter under --lib ran 0 of 1655. `--list` on target/debug/deps/elephc-*.rs showed no asm_split test. `wc -c` on the redirected stdout gave 96 vs 34,949,666 for the emitted file. `wc -l` on nm output: 222 stripped vs 68,372 with --keep-symbols. grep 'Section __text' matched where '__TEXT, __text' did not. cmp of two consecutive emissions: exit 0.\nVerified by: cargo test -p elephc --lib/--bins with and without filters, --list on the test binary, wc -c/-l, cmp, size -m","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","testing","identity-gate","false-green","cargo","emit-asm"],"paths":["src/lib.rs","src/main.rs","src/linker/mod.rs","src/pipeline/backend.rs"],"stack":["rust","cargo"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-05T12:04:43.060Z"}],"context":{"fact":"Each of these produced a confident green that meant nothing, on 2026-09-05.","verification":"`cargo test -p elephc --bins linker::` ran 49 tests including 11 asm_split ones; the same filter under --lib ran 0 of 1655. `--list` on target/debug/deps/elephc-*.rs showed no asm_split test. `wc -c` on the redirected stdout gave 96 vs 34,949,666 for the emitted file. `wc -l` on nm output: 222 stripped vs 68,372 with --keep-symbols. grep 'Section __text' matched where '__TEXT, __text' did not. cmp of two consecutive emissions: exit 0."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-05T12:04:43.060Z","path_fingerprints":[{"path":"src/lib.rs","sha256":"74d1e386ddedabc3d5134f935d57e152bbde6b745b9dc0b6dafdc78ea4959d3b","size":4698},{"path":"src/main.rs","sha256":"d161d6362e03eb321bc0c8e8208cd6097df769480e217726b7c7e2283d217a41","size":5866},{"path":"src/linker/mod.rs","sha256":"5c456015176d4805726acd64d9631d75ebc2786f7e351abb72b885db39ffcf5e","size":15888},{"path":"src/pipeline/backend.rs","sha256":"5763bde9aff298ef85f5bc214e7daa26dd1b60ad3eaab7b5acdedca158f8b061","size":17079}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":645},"created_at":"2026-09-05T12:04:43.060Z","updated_at":"2026-09-05T12:04:43.060Z","author_branch":"reconcile/dirname-symfony"}
```

