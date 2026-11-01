---
id: bug_fix-php-s-global-fallback-for-a-bare-namespaced-call-31852a58
type: bug_fix
title: "PHP's global fallback for a bare namespaced call now comes from the DECLARATION, not an allow-list"
description: "An eager autoload.files entry's global functions are published before any class file is resolved, so a bare call inside a namespaced autoloaded class resolves without naming the function in a hand-maintained list; this is what unblocked trigger_deprecation in Symfony's bin/console"
created: 2026-09-22
verified_by: "three-file fixture, cargo test --lib against the 29-failure baseline, compat_prelude_gate_tests 7/7, Symfony --web 8/8"
sources:
  - path: src/eager_globals.rs
    blob: b09c8467640bdf1e013f5715ceabcef7646fe14e
    lines: 1-40
    snip: 9acb9edbd0ad
    anchor: "use std::cell::RefCell;"
  - path: src/name_resolver/symbols.rs
    blob: 6036ebefb987c51f92df675b448028d725438b00
    lines: 20-50
    snip: 6bb452381544
    anchor: "pub(super) fn canonical_function(&self, name: &str) -> Option<String> {"
  - path: src/autoload/mod.rs
    blob: 41972971ad06a7578f305418cc2349cc4923972f
    lines: 160-180
    snip: 68693635b69d
    anchor: "let bound = &loaded_sources.bound_conditional_declaration_names;"
---

# PHP's global fallback for a bare namespaced call now comes from the DECLARATION, not an allow-list

## Fact

`name_resolver::canonical_compat_prelude_function_name` is a HAND-MAINTAINED list of ~35 function
names that receive PHP's global fallback when called bare from inside a namespace. Its own
comments record three separate names found missing only when a program died at run time
(`trigger_error`, `ini_get_all`, `var_export`). Symfony's `bin/console` needed a fourth,
`trigger_deprecation`, which is a Symfony package function — putting it in the list would be
framework-specific compiler code.

THE GENERAL RULE, and why it is expressible: Composer's eager `autoload.files` entries are loaded
FIRST in `autoload::run_collecting_included_with_defines_and_sources`, before the class fixpoint
resolves a single class file. So every global function they declare is already known when a
namespaced class file is name-resolved. `eager_globals::set` publishes those names there and
`Symbols::canonical_function` consults them last, after the declared-function lookup, so a user's
own `App\trigger_deprecation()` still wins.

COLLECT AT DEPTH, NOT AT FILE SCOPE. Every polyfill writes
`if (!function_exists('f')) { function f() {…} }`, so `declaration_sources.functions` — which
records file-scope declarations — is EMPTY for exactly this shape. Measured: the first version
read that map and changed nothing. `eager_global_function_keys` walks nested bodies instead.

REPRODUCTION, three files, no framework: `module.json` with one `files` entry declaring a guarded
global `probe_upper`, plus a PSR-4 class in `namespace App` calling it bare. Before:
`probe_upper('ab')` at global scope answers `AB` while the class answers `Call to undefined
function App\probe_upper()` — an internal contradiction, so php is not even needed as the oracle.

STILL OPEN, same shape: a bare namespaced call in the ENTRY file cannot be answered this way. The
entry is name-resolved BEFORE the autoload pass runs, so the set is empty at that point.
Symfony's entry is global-scope code, so it is not blocked by this.

MEASURED AFTER: Symfony `--web` 8/8 prod routes byte-identical, 87s, and the emitted assembly GREW
by ~772 KB — more of the program now binds into the compiled world.

## Why

The allow-list it replaces had been caught missing a name three times, and adding a Symfony package function to it would have been framework-specific code
