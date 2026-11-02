---
type: "Gotcha"
title: "interface_exists could not autoload because its helper took the context immutably"
description: "eval interface exists name took &ElephcEvalContext rather than &mut , so it structurally could not run the SPL autoload chain, while PHP's interface exists does by default. A missing interface therefore reported false wi"
resource: "crates/elephc-magician/src/interpreter/builtins/symbols/interface_exists.rs"
tags: ["session-learning", "autoload", "interface-exists", "eval-interpreter"]
timestamp: "2026-09-06T06:03:24.417Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:interface-exists-could-not-autoload-because-its-helper-took-the-context-immutabl"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/builtins/symbols/interface_exists.rs", "crates/elephc-magician/src/interpreter/builtins/symbols/class_exists.rs"]
x-kage-stack: ["rust", "php", "elephc-magician"]
---

# interface_exists could not autoload because its helper took the context immutably

> eval interface exists name took &ElephcEvalContext rather than &mut , so it structurally could not run the SPL autolo…

`eval_interface_exists_name` took `&ElephcEvalContext` rather than `&mut`, so it structurally could not run the SPL autoload chain, while PHP's `interface_exists()` does by default. A missing interface therefore reported false without ever asking the registered autoloaders, and no test caught it because the signature made the omission look deliberate.

Found while writing an autoload-probe test for `debug_backtrace`: `class_exists` ran the autoloader and `interface_exists` silently did not, so the second half of the expected output never appeared. The shape of the bug is worth remembering on its own -- an immutable borrow in a helper is a load-bearing decision about what that helper is allowed to DO, and here it quietly removed a php behaviour.

Reported fixed on the streams branch by 9331b9bcac; verify before relying on it.
Evidence: php -n 8.5.6 with a registered autoloader: `class_exists('MissingProbeOne'); interface_exists('MissingProbeTwo');` invokes the autoloader twice. The interpreter invoked it once; `eval_interface_exists_name(name, context: &ElephcEvalContext, values)` has no way to call eval_spl_autoload_class, which needs `&mut`.
Verified by: Observed as a test diff while building the debug_backtrace autoload probe; the test was retargeted at spl_autoload_call, which does autoload.

## Verification

php -n 8.5.6 with a registered autoloader: `class_exists('MissingProbeOne'); interface_exists('MissingProbeTwo');` invokes the autoloader twice. The interpreter invoked it once; `eval_interface_exists_name(name, context: &ElephcEvalContext, values)` has no way to call eval_spl_autoload_class, which needs `&mut`.

# Citations

[1] explicit_capture (2026-09-06T06:03:24.417Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:interface-exists-could-not-autoload-because-its-helper-took-the-context-immutabl","title":"interface_exists could not autoload because its helper took the context immutably","summary":"eval interface exists name took &ElephcEvalContext rather than &mut , so it structurally could not run the SPL autoload chain, while PHP's interface exists does by default. A missing interface therefore reported false wi","body":"`eval_interface_exists_name` took `&ElephcEvalContext` rather than `&mut`, so it structurally could not run the SPL autoload chain, while PHP's `interface_exists()` does by default. A missing interface therefore reported false without ever asking the registered autoloaders, and no test caught it because the signature made the omission look deliberate.\n\nFound while writing an autoload-probe test for `debug_backtrace`: `class_exists` ran the autoloader and `interface_exists` silently did not, so the second half of the expected output never appeared. The shape of the bug is worth remembering on its own -- an immutable borrow in a helper is a load-bearing decision about what that helper is allowed to DO, and here it quietly removed a php behaviour.\n\nReported fixed on the streams branch by 9331b9bcac; verify before relying on it.\nEvidence: php -n 8.5.6 with a registered autoloader: `class_exists('MissingProbeOne'); interface_exists('MissingProbeTwo');` invokes the autoloader twice. The interpreter invoked it once; `eval_interface_exists_name(name, context: &ElephcEvalContext, values)` has no way to call eval_spl_autoload_class, which needs `&mut`.\nVerified by: Observed as a test diff while building the debug_backtrace autoload probe; the test was retargeted at spl_autoload_call, which does autoload.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","autoload","interface-exists","eval-interpreter"],"paths":["crates/elephc-magician/src/interpreter/builtins/symbols/interface_exists.rs","crates/elephc-magician/src/interpreter/builtins/symbols/class_exists.rs"],"stack":["rust","php","elephc-magician"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-06T06:03:24.417Z"}],"context":{"fact":"`eval_interface_exists_name` took `&ElephcEvalContext` rather than `&mut`, so it structurally could not run the SPL autoload chain, while PHP's `interface_exists()` does by default. A missing interface therefore reported false without ever asking the registered autoloaders, and no test caught it because the signature made the omission look deliberate.","verification":"php -n 8.5.6 with a registered autoloader: `class_exists('MissingProbeOne'); interface_exists('MissingProbeTwo');` invokes the autoloader twice. The interpreter invoked it once; `eval_interface_exists_name(name, context: &ElephcEvalContext, values)` has no way to call eval_spl_autoload_class, which needs `&mut`."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-06T06:03:24.417Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/builtins/symbols/interface_exists.rs","sha256":"01a72b7b7b0dded8f074c322d72ce6229bb4df15647802872267c0135632586f","size":4595},{"path":"crates/elephc-magician/src/interpreter/builtins/symbols/class_exists.rs","sha256":"a78004c3bfaeeca63c142d8ec522b38498a20f3c67134330103d621e4c822757","size":4738}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":100,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","concise but substantive","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":329},"created_at":"2026-09-06T06:03:24.417Z","updated_at":"2026-09-06T06:03:24.417Z","author_branch":"reconcile/dirname-symfony"}
```

