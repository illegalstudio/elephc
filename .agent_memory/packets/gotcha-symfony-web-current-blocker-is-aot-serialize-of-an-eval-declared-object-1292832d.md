---
type: "Gotcha"
title: "Symfony --web: current blocker is AOT serialize() of an eval-declared object"
description: "State of the examples/symfony app web goal as of this session: the request reaches the ROUTING build, writes a VALID var/cache/dev/url matching routes.php with an empty route table which is what RouterListener::onKernelE"
resource: "src/codegen_support/runtime/system/serialize.rs"
tags: ["session-learning", "symfony", "serialize", "eval-bridge", "hook-slot", "next-step"]
timestamp: "2026-09-13T08:32:35.676Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:symfony-web-current-blocker-is-aot-serialize-of-an-eval-declared-object-17892883"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/codegen_support/runtime/system/serialize.rs", "crates/elephc-magician/src/ffi/unserialize_objects.rs", "src/codegen_support/runtime/data/fixed.rs", "examples/symfony-app/public/index.php"]
---

# Symfony --web: current blocker is AOT serialize() of an eval-declared object

> State of the examples/symfony app web goal as of this session: the request reaches the ROUTING build, writes a VALID …

State of the `examples/symfony-app` `--web` goal as of this session: the request reaches the ROUTING build, writes a VALID `var/cache/dev/url_matching_routes.php` with an empty route table (which is what `RouterListener::onKernelException` needs to answer the "Welcome to Symfony!" page with HTTP 404), and then fails reading its own metadata back.

The remaining failure, verified end to end:
- `ResourceCheckerConfigCache::write()` is a NATIVE (AOT) method -- confirmed in the trace as `method_dispatch method="write" dynamic_class=None runtime_class=Some("Symfony\Component\Config\ResourceCheckerConfigCache")` -- so its `serialize($metadata)` is the AOT builtin, not the interpreter's.
- The metadata array holds a `ContainerParametersResource` the interpreter built. The AOT serializer reads the class from the object header, which for every eval object is `stdClass`, and its dynamic-property hash, which is empty because the overlay lives in the eval context. It wrote `a:2:{i:0;O:8:"stdClass":0:{}…}`.
- The next `isFresh()` unserializes that and `ContainerParametersResourceChecker::supports(ResourceInterface $metadata)` refuses the bare stdClass with a TypeError.

The fix is the hook-slot pattern, symmetric with `_elephc_eval_unserialize_object_fn`: a `_elephc_eval_serialize_object_fn` slot consulted at `__rt_serialize_object` (src/codegen_support/runtime/system/serialize.rs, both arches) before the class-id lookup, handing the raw object pointer (which IS the eval identity) to the interpreter and appending the `O:…` fragment it returns. The open design question is reference numbering: the AOT serializer maintains its own `r:<idx>` dedup table, and an interpreter-rendered subtree numbers its own references independently, so a shared reference spanning the boundary would disagree with php.

Verified-correct reducer for the gap: scratchpad red32/b.php -- cross-eval `serialize()` of an eval object is byte-correct, AOT `serialize()` of the SAME object is `O:8:"stdClass":0:{}`.
Evidence: The request now returns a rendered HTTP 500 naming the TypeError; var/cache/dev/url_matching_routes.php.meta contains `a:2:{i:0;O:8:"stdClass":0:{}…}`.
Verified by: examples/symfony-app --web request with ELEPHC_EVAL_TRACE=1

## Verification

The request now returns a rendered HTTP 500 naming the TypeError; var/cache/dev/url_matching_routes.php.meta contains `a:2:{i:0;O:8:"stdClass":0:{}…}`.

# Citations

[1] explicit_capture (2026-09-13T08:32:35.676Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:symfony-web-current-blocker-is-aot-serialize-of-an-eval-declared-object-17892883","title":"Symfony --web: current blocker is AOT serialize() of an eval-declared object","summary":"State of the examples/symfony app web goal as of this session: the request reaches the ROUTING build, writes a VALID var/cache/dev/url matching routes.php with an empty route table which is what RouterListener::onKernelE","body":"State of the `examples/symfony-app` `--web` goal as of this session: the request reaches the ROUTING build, writes a VALID `var/cache/dev/url_matching_routes.php` with an empty route table (which is what `RouterListener::onKernelException` needs to answer the \"Welcome to Symfony!\" page with HTTP 404), and then fails reading its own metadata back.\n\nThe remaining failure, verified end to end:\n- `ResourceCheckerConfigCache::write()` is a NATIVE (AOT) method -- confirmed in the trace as `method_dispatch method=\"write\" dynamic_class=None runtime_class=Some(\"Symfony\\Component\\Config\\ResourceCheckerConfigCache\")` -- so its `serialize($metadata)` is the AOT builtin, not the interpreter's.\n- The metadata array holds a `ContainerParametersResource` the interpreter built. The AOT serializer reads the class from the object header, which for every eval object is `stdClass`, and its dynamic-property hash, which is empty because the overlay lives in the eval context. It wrote `a:2:{i:0;O:8:\"stdClass\":0:{}…}`.\n- The next `isFresh()` unserializes that and `ContainerParametersResourceChecker::supports(ResourceInterface $metadata)` refuses the bare stdClass with a TypeError.\n\nThe fix is the hook-slot pattern, symmetric with `_elephc_eval_unserialize_object_fn`: a `_elephc_eval_serialize_object_fn` slot consulted at `__rt_serialize_object` (src/codegen_support/runtime/system/serialize.rs, both arches) before the class-id lookup, handing the raw object pointer (which IS the eval identity) to the interpreter and appending the `O:…` fragment it returns. The open design question is reference numbering: the AOT serializer maintains its own `r:<idx>` dedup table, and an interpreter-rendered subtree numbers its own references independently, so a shared reference spanning the boundary would disagree with php.\n\nVerified-correct reducer for the gap: scratchpad red32/b.php -- cross-eval `serialize()` of an eval object is byte-correct, AOT `serialize()` of the SAME object is `O:8:\"stdClass\":0:{}`.\nEvidence: The request now returns a rendered HTTP 500 naming the TypeError; var/cache/dev/url_matching_routes.php.meta contains `a:2:{i:0;O:8:\"stdClass\":0:{}…}`.\nVerified by: examples/symfony-app --web request with ELEPHC_EVAL_TRACE=1","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony","serialize","eval-bridge","hook-slot","next-step"],"paths":["src/codegen_support/runtime/system/serialize.rs","crates/elephc-magician/src/ffi/unserialize_objects.rs","src/codegen_support/runtime/data/fixed.rs","examples/symfony-app/public/index.php"],"stack":[],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-13T08:32:35.676Z"}],"context":{"fact":"State of the `examples/symfony-app` `--web` goal as of this session: the request reaches the ROUTING build, writes a VALID `var/cache/dev/url_matching_routes.php` with an empty route table (which is what `RouterListener::onKernelException` needs to answer the \"Welcome to Symfony!\" page with HTTP 404), and then fails reading its own metadata back.","verification":"The request now returns a rendered HTTP 500 naming the TypeError; var/cache/dev/url_matching_routes.php.meta contains `a:2:{i:0;O:8:\"stdClass\":0:{}…}`."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-13T08:32:35.676Z","path_fingerprints":[{"path":"src/codegen_support/runtime/system/serialize.rs","sha256":"77c4f2b6a69f8f5e25268f730ac191f1948a2e8bfc155e67fc5e28d6371680fe","size":170258},{"path":"crates/elephc-magician/src/ffi/unserialize_objects.rs","sha256":"1375057bf35095b706a66f995f7749a8faa9e287df961b19f7e1efba3942831a","size":4980},{"path":"src/codegen_support/runtime/data/fixed.rs","sha256":"978326026b1da4f6768fbd0b5df8b8f3d2bfa27eb357a64e5ea693fe64b24e22","size":110696},{"path":"examples/symfony-app/public/index.php","sha256":"c0696c30e0f1a223481e55a8078c5ebc600cf301b2cc9426c12f52ff8ffb861d","size":206}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":559},"created_at":"2026-09-13T08:32:35.676Z","updated_at":"2026-09-13T08:32:35.676Z","author_branch":"reconcile/dirname-symfony"}
```

