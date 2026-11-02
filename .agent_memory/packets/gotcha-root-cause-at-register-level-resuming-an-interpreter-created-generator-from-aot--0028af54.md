---
type: "Gotcha"
title: "ROOT CAUSE at register level: resuming an interpreter-created Generator from AOT runs on a heap activation stack whose saved fp/lr are uninitialised, so the first ret jumps to 0x4"
description: "OPEN, NOT fixed, but diagnosed to the register. This is the last blocker of the Symfony web request, and the earlier framing of it as vague \"memory corruption\" is now superseded by an exact mechanism. THE REGISTER STATE"
resource: "crates/elephc-magician/src/interpreter/generators.rs"
tags: ["session-learning", "symfony-web", "generator", "eval-bridge", "activation-stack", "lldb", "root-cause", "open-bug"]
timestamp: "2026-09-12T19:02:02.037Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:root-cause-at-register-level-resuming-an-interpreter-created-generator-from-aot-"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/generators.rs", "src/codegen/lower_inst/generator_instructions.rs", "src/codegen/lower_inst/method_dispatch.rs", "src/codegen_support/runtime/mod.rs"]
x-kage-stack: ["rust", "php", "symfony", "macos-aarch64"]
---

# ROOT CAUSE at register level: resuming an interpreter-created Generator from AOT runs on a heap activation stack whose saved fp/lr are uninitialised, so the first ret jumps to 0x4

> OPEN, NOT fixed, but diagnosed to the register. This is the last blocker of the Symfony web request, and the earlier …

OPEN, NOT fixed, but diagnosed to the register. This is the last blocker of the Symfony `--web` request, and the earlier framing of it as vague "memory corruption" is now superseded by an exact mechanism.

THE REGISTER STATE AT THE FAULT, from lldb on the reducer binary:
      x0 = 0x0000000100a37220  survive`heap_buf + 1440
      x1 = 0x0000000100a37270  survive`heap_buf + 1520
      fp = 0x0000000000000002
      sp = 0x0000000100a372c0  survive`heap_buf + 1600
      lr = 0x0000000000000004
      pc = 0x0000000000000004
  stop reason = EXC_BAD_ACCESS (code=1, address=0x4)
`sp` is INSIDE `heap_buf` -- execution is on a heap-allocated activation stack, which is how a
generator/fiber frame is carried. On that stack the frame record is not a frame record: fp = 2 and
lr = 4 are small tag-shaped integers where a saved {x29, x30} pair belongs, so the epilogue's `ret`
branches to 0x4. PC and LR are BOTH 4, which is the signature of `ret` with a clobbered x30 rather
than an indirect call through a bad pointer.

SO THE MECHANISM IS: the AOT Generator/Iterator protocol resumes a generator by switching to its
activation stack and returning into it, and an interpreter-created generator's activation was never
initialised for native resumption -- it has an interpreter frame (registered by
`eval_generator_new` -> `register_eval_generator`, keyed by object identity) and no valid native
{fp, lr} pair. An AOT-created generator has both, which is why the identical probe is correct for
one and fatal for the other.

WHY IT LOOKED LIKE MOVING CORRUPTION, and this is worth knowing so the next session does not chase
ghosts: the crash point shifts with any nearby code change (`[inst][valid=1][cur=` in one variant,
`[valid=` in another, before any output in a third) because the bogus `ret` happens wherever the
layout puts the first epilogue on that stack -- not because a scribble lands somewhere different.

TOOLING NOTE THAT UNBLOCKED THIS. The runbook's lldb recipe failed earlier against the `--web`
binary because the prefork worker is respawned and the pid goes stale. It works perfectly against
a plain reducer binary, which does not fork: `lldb --batch -k "register read ..." -k quit -o run
./probe`. Use `-k` (commands to run when the target stops) rather than trailing `-o` commands,
which do not execute after a crash. `--heap-debug` is useless here: it prints its leak summary at
exit and the process never reaches it.

WHERE TO START. Compare what an AOT generator's activation carries against what
`eval_generator_new` builds (crates/elephc-magician/src/interpreter/generators.rs): the fix is
either to give an interpreter-owned generator a real native activation, or to make the AOT
Generator intrinsics and the Iterator-protocol dispatch detect one and route through the bridge
instead of switching stacks. Note the dispatch side already half-works -- `instanceof Iterator`
answers true and one `valid()` returned the correct value in one layout -- so the routing exists;
it is the stack switch underneath that is invalid.
Evidence: scratchpad/red10/survive.php + survivegen.php, compiled with --keep-symbols and run under `lldb --batch -k "register read x0 x1 x2 x29 sp lr pc" -k quit -o run`: the register dump quoted above, with `sp` symbolicated by lldb as `survive\`heap_buf + 1600`. php -n 8.5.10 prints `EVAL n=1 after` for the same program. The same probe with an AOT-declared generator is correct in every variant. Three earlier variants of the probe crash at three different points (after `[inst][valid=1][cur=`, after `[valid=`, and before any output), which is what the single bogus `ret` explains. Symfony `--web` remains `worker terminated by signal 11`, crash report index-2026-09-12-205359.ips, KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`.
Verified by: lldb register dump on a non-forking reducer binary, against php -n 8.5.10 as the oracle for the expected output

## Verification

scratchpad/red10/survive.php + survivegen.php, compiled with --keep-symbols and run under `lldb --batch -k "register read x0 x1 x2 x29 sp lr pc" -k quit -o run`: the register dump quoted above, with `sp` symbolicated by lldb as `survive\`heap_buf + 1600`. php -n 8.5.10 prints `EVAL n=1 after` for the same program. The same probe with an AOT-declared generator is correct in every variant. Three earlier variants of the probe crash at three different points (after `[inst][valid=1][cur=`, after `[valid=`, and before any output), which is what the single bogus `ret` explains. Symfony `--web` remains `worker terminated by signal 11`, crash report index-2026-09-12-205359.ips, KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`.

# Citations

[1] explicit_capture (2026-09-12T19:02:02.037Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:root-cause-at-register-level-resuming-an-interpreter-created-generator-from-aot-","title":"ROOT CAUSE at register level: resuming an interpreter-created Generator from AOT runs on a heap activation stack whose saved fp/lr are uninitialised, so the first ret jumps to 0x4","summary":"OPEN, NOT fixed, but diagnosed to the register. This is the last blocker of the Symfony web request, and the earlier framing of it as vague \"memory corruption\" is now superseded by an exact mechanism. THE REGISTER STATE","body":"OPEN, NOT fixed, but diagnosed to the register. This is the last blocker of the Symfony `--web` request, and the earlier framing of it as vague \"memory corruption\" is now superseded by an exact mechanism.\n\nTHE REGISTER STATE AT THE FAULT, from lldb on the reducer binary:\n      x0 = 0x0000000100a37220  survive`heap_buf + 1440\n      x1 = 0x0000000100a37270  survive`heap_buf + 1520\n      fp = 0x0000000000000002\n      sp = 0x0000000100a372c0  survive`heap_buf + 1600\n      lr = 0x0000000000000004\n      pc = 0x0000000000000004\n  stop reason = EXC_BAD_ACCESS (code=1, address=0x4)\n`sp` is INSIDE `heap_buf` -- execution is on a heap-allocated activation stack, which is how a\ngenerator/fiber frame is carried. On that stack the frame record is not a frame record: fp = 2 and\nlr = 4 are small tag-shaped integers where a saved {x29, x30} pair belongs, so the epilogue's `ret`\nbranches to 0x4. PC and LR are BOTH 4, which is the signature of `ret` with a clobbered x30 rather\nthan an indirect call through a bad pointer.\n\nSO THE MECHANISM IS: the AOT Generator/Iterator protocol resumes a generator by switching to its\nactivation stack and returning into it, and an interpreter-created generator's activation was never\ninitialised for native resumption -- it has an interpreter frame (registered by\n`eval_generator_new` -> `register_eval_generator`, keyed by object identity) and no valid native\n{fp, lr} pair. An AOT-created generator has both, which is why the identical probe is correct for\none and fatal for the other.\n\nWHY IT LOOKED LIKE MOVING CORRUPTION, and this is worth knowing so the next session does not chase\nghosts: the crash point shifts with any nearby code change (`[inst][valid=1][cur=` in one variant,\n`[valid=` in another, before any output in a third) because the bogus `ret` happens wherever the\nlayout puts the first epilogue on that stack -- not because a scribble lands somewhere different.\n\nTOOLING NOTE THAT UNBLOCKED THIS. The runbook's lldb recipe failed earlier against the `--web`\nbinary because the prefork worker is respawned and the pid goes stale. It works perfectly against\na plain reducer binary, which does not fork: `lldb --batch -k \"register read ...\" -k quit -o run\n./probe`. Use `-k` (commands to run when the target stops) rather than trailing `-o` commands,\nwhich do not execute after a crash. `--heap-debug` is useless here: it prints its leak summary at\nexit and the process never reaches it.\n\nWHERE TO START. Compare what an AOT generator's activation carries against what\n`eval_generator_new` builds (crates/elephc-magician/src/interpreter/generators.rs): the fix is\neither to give an interpreter-owned generator a real native activation, or to make the AOT\nGenerator intrinsics and the Iterator-protocol dispatch detect one and route through the bridge\ninstead of switching stacks. Note the dispatch side already half-works -- `instanceof Iterator`\nanswers true and one `valid()` returned the correct value in one layout -- so the routing exists;\nit is the stack switch underneath that is invalid.\nEvidence: scratchpad/red10/survive.php + survivegen.php, compiled with --keep-symbols and run under `lldb --batch -k \"register read x0 x1 x2 x29 sp lr pc\" -k quit -o run`: the register dump quoted above, with `sp` symbolicated by lldb as `survive\\`heap_buf + 1600`. php -n 8.5.10 prints `EVAL n=1 after` for the same program. The same probe with an AOT-declared generator is correct in every variant. Three earlier variants of the probe crash at three different points (after `[inst][valid=1][cur=`, after `[valid=`, and before any output), which is what the single bogus `ret` explains. Symfony `--web` remains `worker terminated by signal 11`, crash report index-2026-09-12-205359.ips, KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`.\nVerified by: lldb register dump on a non-forking reducer binary, against php -n 8.5.10 as the oracle for the expected output","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony-web","generator","eval-bridge","activation-stack","lldb","root-cause","open-bug"],"paths":["crates/elephc-magician/src/interpreter/generators.rs","src/codegen/lower_inst/generator_instructions.rs","src/codegen/lower_inst/method_dispatch.rs","src/codegen_support/runtime/mod.rs"],"stack":["rust","php","symfony","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T19:02:02.037Z"}],"context":{"fact":"OPEN, NOT fixed, but diagnosed to the register. This is the last blocker of the Symfony `--web` request, and the earlier framing of it as vague \"memory corruption\" is now superseded by an exact mechanism.","verification":"scratchpad/red10/survive.php + survivegen.php, compiled with --keep-symbols and run under `lldb --batch -k \"register read x0 x1 x2 x29 sp lr pc\" -k quit -o run`: the register dump quoted above, with `sp` symbolicated by lldb as `survive\\`heap_buf + 1600`. php -n 8.5.10 prints `EVAL n=1 after` for the same program. The same probe with an AOT-declared generator is correct in every variant. Three earlier variants of the probe crash at three different points (after `[inst][valid=1][cur=`, after `[valid=`, and before any output), which is what the single bogus `ret` explains. Symfony `--web` remains `worker terminated by signal 11`, crash report index-2026-09-12-205359.ips, KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T19:02:02.037Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/generators.rs","sha256":"856f5f3567ad004147f370c85af2f8d377e568a57f552ac35ec76ecce4f0d8dc","size":37150},{"path":"src/codegen/lower_inst/generator_instructions.rs","sha256":"8a225bce7db9536a1f638d4b2a296e40b5639871e7901a9627304624737a92b5","size":9865},{"path":"src/codegen/lower_inst/method_dispatch.rs","sha256":"e9b56433ab4773e863eeaa0b310f74170d5b1cd2e590562c054e5a830546e9b0","size":37731},{"path":"src/codegen_support/runtime/mod.rs","sha256":"93817a0fe881c18f81615d43010b47b3219795eb7eaf68f251920fb33a5d71ea","size":4126}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":991},"created_at":"2026-09-12T19:02:02.037Z","updated_at":"2026-09-12T19:02:02.037Z","author_branch":"reconcile/dirname-symfony"}
```

