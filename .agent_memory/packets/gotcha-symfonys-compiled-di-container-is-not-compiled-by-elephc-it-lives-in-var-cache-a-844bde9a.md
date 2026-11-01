---
type: "Gotcha"
title: "Symfony's compiled DI container is NOT compiled by elephc — it lives in var/cache and is included by a computed path, so everything it instantiates by name is eval-declared"
description: "MEASURED, and it reframes what is left of the Symfony web work. nm examples/symfony app/public/index | grep c App KernelDevDebugContainer is 0 . The container is var/cache/dev/ContainerTvPT0Dp/App KernelDevDebugContainer"
resource: "examples/symfony-app/public/index.php"
tags: ["session-learning", "symfony", "architecture", "autoload", "eval-bridge", "unserialize"]
timestamp: "2026-09-12T21:43:17.083Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:symfonys-compiled-di-container-is-not-compiled-by-elephc-it-lives-in-var-cache-a"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["examples/symfony-app/public/index.php", "src/codegen_support/runtime/system/unserialize/decoder_aarch64.rs", "crates/elephc-magician/src/ffi/class_autoload.rs", "src/pipeline.rs"]
x-kage-stack: ["php", "rust"]
---

# Symfony's compiled DI container is NOT compiled by elephc — it lives in var/cache and is included by a computed path, so everything it instantiates by name is eval-declared

> MEASURED, and it reframes what is left of the Symfony web work. nm examples/symfony app/public/index | grep c App Ker…

MEASURED, and it reframes what is left of the Symfony `--web` work.

`nm examples/symfony-app/public/index | grep -c App_KernelDevDebugContainer` is **0**. The container is `var/cache/dev/ContainerTvPT0Dp/App_KernelDevDebugContainer.php`, reached through a computed `require`, so the compiler never sees it: the trace shows its `__construct` and every `getXService()` running as eval-declared methods.

THE CONSEQUENCE, which is the part worth remembering: every class the container names with a literal `new \Foo\Bar(...)` is autoloaded AT RUNTIME into the interpreter and has NO AOT layout. Two neighbours in the same closure show it exactly —

```php
yield 0 => new \Symfony\Component\DependencyInjection\Config\ContainerParametersResourceChecker($container);
yield 1 => new \Symfony\Component\Config\Resource\SelfCheckingResourceChecker();
```

`SelfCheckingResourceChecker` has 21 symbols in the binary (some OTHER compiled file names it); `ContainerParametersResourceChecker` has 0, and neither does `ResourceCheckerConfigCacheFactory` or `getRouterService`. So which Symfony classes are compiled is decided by the *rest* of the program, never by the container.

WHY IT MATTERS BEYOND THIS RUNG. Any AOT machinery that needs a compiled class LAYOUT will meet eval-declared classes routinely on a Symfony request, not as an edge case:
- `unserialize` (the current frontier) builds `__PHP_Incomplete_Class` for them;
- `__rt_new_by_name` answers 0 for them, so the class-autoload hook added this session cannot rescue the case even when it reports `loaded=true`;
- anything else keyed on `_class_*_ptrs` by class id has the same exposure.

The autoload-hook work is still right, and so is looking for a cheaper lever — but "get those classes compiled" is NOT available: the container is data the compiler cannot read. Per-object delegation to the interpreter is the structural answer for `unserialize`, and the same question will come back for the next AOT/eval layout boundary.
Evidence: `nm public/index`: `App_KernelDevDebugContainer` 0 symbols, `getRouterService` 0, `ResourceCheckerConfigCacheFactory` 0, `SelfCheckingResourceChecker` 21, `ResourceCheckerConfigCache___` 88. Trace: the container's `__construct` and `getRouterService`-created services all run through `phase=dynamic_method_start` (eval-declared), while `ResourceCheckerConfigCache::isFresh` runs with `dynamic_class=None` (compiled).
Verified by: nm on the built binary plus the ELEPHC_EVAL_TRACE request trace.

## Verification

`nm public/index`: `App_KernelDevDebugContainer` 0 symbols, `getRouterService` 0, `ResourceCheckerConfigCacheFactory` 0, `SelfCheckingResourceChecker` 21, `ResourceCheckerConfigCache___` 88. Trace: the container's `__construct` and `getRouterService`-created services all run through `phase=dynamic_method_start` (eval-declared), while `ResourceCheckerConfigCache::isFresh` runs with `dynamic_class=None` (compiled).

# Citations

[1] explicit_capture (2026-09-12T21:43:17.083Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:symfonys-compiled-di-container-is-not-compiled-by-elephc-it-lives-in-var-cache-a","title":"Symfony's compiled DI container is NOT compiled by elephc — it lives in var/cache and is included by a computed path, so everything it instantiates by name is eval-declared","summary":"MEASURED, and it reframes what is left of the Symfony web work. nm examples/symfony app/public/index | grep c App KernelDevDebugContainer is 0 . The container is var/cache/dev/ContainerTvPT0Dp/App KernelDevDebugContainer","body":"MEASURED, and it reframes what is left of the Symfony `--web` work.\n\n`nm examples/symfony-app/public/index | grep -c App_KernelDevDebugContainer` is **0**. The container is `var/cache/dev/ContainerTvPT0Dp/App_KernelDevDebugContainer.php`, reached through a computed `require`, so the compiler never sees it: the trace shows its `__construct` and every `getXService()` running as eval-declared methods.\n\nTHE CONSEQUENCE, which is the part worth remembering: every class the container names with a literal `new \\Foo\\Bar(...)` is autoloaded AT RUNTIME into the interpreter and has NO AOT layout. Two neighbours in the same closure show it exactly —\n\n```php\nyield 0 => new \\Symfony\\Component\\DependencyInjection\\Config\\ContainerParametersResourceChecker($container);\nyield 1 => new \\Symfony\\Component\\Config\\Resource\\SelfCheckingResourceChecker();\n```\n\n`SelfCheckingResourceChecker` has 21 symbols in the binary (some OTHER compiled file names it); `ContainerParametersResourceChecker` has 0, and neither does `ResourceCheckerConfigCacheFactory` or `getRouterService`. So which Symfony classes are compiled is decided by the *rest* of the program, never by the container.\n\nWHY IT MATTERS BEYOND THIS RUNG. Any AOT machinery that needs a compiled class LAYOUT will meet eval-declared classes routinely on a Symfony request, not as an edge case:\n- `unserialize` (the current frontier) builds `__PHP_Incomplete_Class` for them;\n- `__rt_new_by_name` answers 0 for them, so the class-autoload hook added this session cannot rescue the case even when it reports `loaded=true`;\n- anything else keyed on `_class_*_ptrs` by class id has the same exposure.\n\nThe autoload-hook work is still right, and so is looking for a cheaper lever — but \"get those classes compiled\" is NOT available: the container is data the compiler cannot read. Per-object delegation to the interpreter is the structural answer for `unserialize`, and the same question will come back for the next AOT/eval layout boundary.\nEvidence: `nm public/index`: `App_KernelDevDebugContainer` 0 symbols, `getRouterService` 0, `ResourceCheckerConfigCacheFactory` 0, `SelfCheckingResourceChecker` 21, `ResourceCheckerConfigCache___` 88. Trace: the container's `__construct` and `getRouterService`-created services all run through `phase=dynamic_method_start` (eval-declared), while `ResourceCheckerConfigCache::isFresh` runs with `dynamic_class=None` (compiled).\nVerified by: nm on the built binary plus the ELEPHC_EVAL_TRACE request trace.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony","architecture","autoload","eval-bridge","unserialize"],"paths":["examples/symfony-app/public/index.php","src/codegen_support/runtime/system/unserialize/decoder_aarch64.rs","crates/elephc-magician/src/ffi/class_autoload.rs","src/pipeline.rs"],"stack":["php","rust"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T21:43:17.083Z"}],"context":{"fact":"MEASURED, and it reframes what is left of the Symfony `--web` work.","verification":"`nm public/index`: `App_KernelDevDebugContainer` 0 symbols, `getRouterService` 0, `ResourceCheckerConfigCacheFactory` 0, `SelfCheckingResourceChecker` 21, `ResourceCheckerConfigCache___` 88. Trace: the container's `__construct` and `getRouterService`-created services all run through `phase=dynamic_method_start` (eval-declared), while `ResourceCheckerConfigCache::isFresh` runs with `dynamic_class=None` (compiled)."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T21:43:17.083Z","path_fingerprints":[{"path":"examples/symfony-app/public/index.php","sha256":"c0696c30e0f1a223481e55a8078c5ebc600cf301b2cc9426c12f52ff8ffb861d","size":206},{"path":"src/codegen_support/runtime/system/unserialize/decoder_aarch64.rs","sha256":"425c17322c93cb2fe4a7b256059fc9dc2eab3c2cda8ef64d14978d316479e497","size":69276},{"path":"crates/elephc-magician/src/ffi/class_autoload.rs","sha256":"f9085f2f9f3c644a39d0c12f2f47c973d6d81b89b56c7018753b0629cc524425","size":3497},{"path":"src/pipeline.rs","sha256":"466fb30fa137e2353e69b7e5bfb35cc15ce21e620bd340cd9ab4e2f06020fc14","size":40369}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":60000,"discovery_tokens_estimated":false,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":622},"created_at":"2026-09-12T21:43:17.083Z","updated_at":"2026-09-12T21:43:17.083Z","author_branch":"reconcile/dirname-symfony"}
```

