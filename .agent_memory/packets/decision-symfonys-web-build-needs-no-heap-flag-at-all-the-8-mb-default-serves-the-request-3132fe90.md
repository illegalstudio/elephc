---
type: "Decision"
title: "Symfony's --web build needs no heap flag at all: the 8 MB default serves the request, 1 GiB was never a requirement"
description: "Compiling examples/symfony app with heap size=1073741824 was a session convention carried forward in the local build script, not a measured requirement, and nothing in the repo prescribes it: README, docs/compiling/cli r"
resource: "examples/symfony-app/public/index.php"
tags: ["session-learning", "symfony", "heap-size", "measurement", "benchmark-harness"]
timestamp: "2026-09-13T13:40:20.472Z"
x-kage-id: "repo:lazy-petting-popcorn:decision:symfonys-web-build-needs-no-heap-flag-at-all-the-8-mb-default-serves-the-request"
x-kage-type: "decision"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["examples/symfony-app/public/index.php", "docs/compiling/cli-reference.md"]
x-kage-stack: ["php", "symfony"]
---

# Symfony's --web build needs no heap flag at all: the 8 MB default serves the request, 1 GiB was never a requirement

> Compiling examples/symfony app with heap size=1073741824 was a session convention carried forward in the local build …

Compiling examples/symfony-app with `--heap-size=1073741824` was a session convention carried forward in the local build script, not a measured requirement, and nothing in the repo prescribes it: README, docs/compiling/cli-reference.md and docs/internals/memory-model.md all document the 8 MB default, and the only other mention is an older memory packet that simply repeats the flag it was invoked with.

Measured by bisection on the compiled `--web` binary, one cold request each, every step producing HTTP 404 with the "Welcome to Symfony!" page: 1024 MiB OK, 256 MiB OK, 64 MiB OK, 16 MiB OK, 8 MiB (the DEFAULT) OK. So the flag can be dropped entirely.

What the bisection nearly got wrong, and the reason to keep the harness honest: the first runs reported FAIL at 256 MiB and even at 1 GiB. Both were harness artifacts -- the script polled with a fixed `sleep 3` while the ~170 MB binary needs longer to bind, so a connection failure was being read as a heap failure. The harness now polls for "listening on" before issuing the request, and 1 GiB was re-run as a control before any smaller value was believed.

Heap size is also NOT what limits sustained serving. Six consecutive requests fail identically at 8 MiB and at 1 GiB -- the second request dies in `ContainerParametersResourceChecker::isFresh`, not on memory -- so the two questions are independent. See the packet on that rung.
Evidence: heap-try.sh bisection at 1024/256/64/16/8 MiB against examples/symfony-app/public/index.php, each compiled with the same `native install` + `--web --keep-symbols` recipe and differing only in --heap-size; each run polled for the listener then issued one cold request and grepped the body for the welcome marker.
Verified by: five compile-and-serve runs plus a 1 GiB control re-run after the harness was corrected

## Verification

heap-try.sh bisection at 1024/256/64/16/8 MiB against examples/symfony-app/public/index.php, each compiled with the same `native install` + `--web --keep-symbols` recipe and differing only in --heap-size; each run polled for the listener then issued one cold request and grepped the body for the welcome marker.

# Citations

[1] explicit_capture (2026-09-13T13:40:20.472Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:decision:symfonys-web-build-needs-no-heap-flag-at-all-the-8-mb-default-serves-the-request","title":"Symfony's --web build needs no heap flag at all: the 8 MB default serves the request, 1 GiB was never a requirement","summary":"Compiling examples/symfony app with heap size=1073741824 was a session convention carried forward in the local build script, not a measured requirement, and nothing in the repo prescribes it: README, docs/compiling/cli r","body":"Compiling examples/symfony-app with `--heap-size=1073741824` was a session convention carried forward in the local build script, not a measured requirement, and nothing in the repo prescribes it: README, docs/compiling/cli-reference.md and docs/internals/memory-model.md all document the 8 MB default, and the only other mention is an older memory packet that simply repeats the flag it was invoked with.\n\nMeasured by bisection on the compiled `--web` binary, one cold request each, every step producing HTTP 404 with the \"Welcome to Symfony!\" page: 1024 MiB OK, 256 MiB OK, 64 MiB OK, 16 MiB OK, 8 MiB (the DEFAULT) OK. So the flag can be dropped entirely.\n\nWhat the bisection nearly got wrong, and the reason to keep the harness honest: the first runs reported FAIL at 256 MiB and even at 1 GiB. Both were harness artifacts -- the script polled with a fixed `sleep 3` while the ~170 MB binary needs longer to bind, so a connection failure was being read as a heap failure. The harness now polls for \"listening on\" before issuing the request, and 1 GiB was re-run as a control before any smaller value was believed.\n\nHeap size is also NOT what limits sustained serving. Six consecutive requests fail identically at 8 MiB and at 1 GiB -- the second request dies in `ContainerParametersResourceChecker::isFresh`, not on memory -- so the two questions are independent. See the packet on that rung.\nEvidence: heap-try.sh bisection at 1024/256/64/16/8 MiB against examples/symfony-app/public/index.php, each compiled with the same `native install` + `--web --keep-symbols` recipe and differing only in --heap-size; each run polled for the listener then issued one cold request and grepped the body for the welcome marker.\nVerified by: five compile-and-serve runs plus a 1 GiB control re-run after the harness was corrected","type":"decision","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony","heap-size","measurement","benchmark-harness"],"paths":["examples/symfony-app/public/index.php","docs/compiling/cli-reference.md"],"stack":["php","symfony"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-13T13:40:20.472Z"}],"context":{"fact":"Compiling examples/symfony-app with `--heap-size=1073741824` was a session convention carried forward in the local build script, not a measured requirement, and nothing in the repo prescribes it: README, docs/compiling/cli-reference.md and docs/internals/memory-model.md all document the 8 MB default, and the only other mention is an older memory packet that simply repeats the flag it was invoked with.","verification":"heap-try.sh bisection at 1024/256/64/16/8 MiB against examples/symfony-app/public/index.php, each compiled with the same `native install` + `--web --keep-symbols` recipe and differing only in --heap-size; each run polled for the listener then issued one cold request and grepped the body for the welcome marker."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-13T13:40:20.472Z","path_fingerprints":[{"path":"examples/symfony-app/public/index.php","sha256":"c0696c30e0f1a223481e55a8078c5ebc600cf301b2cc9426c12f52ff8ffb861d","size":206},{"path":"docs/compiling/cli-reference.md","sha256":"e99c8755533cacdf18ee22a1d02f69a6a7afcdf5fd1cbb92469d440260ea7c40","size":55408}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":4000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":455},"created_at":"2026-09-13T13:40:20.472Z","updated_at":"2026-09-13T13:40:20.472Z","author_branch":"reconcile/dirname-symfony"}
```

