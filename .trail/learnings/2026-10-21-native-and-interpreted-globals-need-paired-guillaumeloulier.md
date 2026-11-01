---
title: Native and interpreted globals need paired constructor and method handoffs
author: Guillaumeloulier
date: 2026-10-21
branch: reconcile/dirname-symfony
touches:
  - crates/elephc-magician/src/context/runtime_state.rs
  - crates/elephc-magician/src/ffi/object_construction.rs
  - tests/codegen/eval_global_scope.rs
  - crates/elephc-magician/src/ffi/context.rs
---

## Decided

A native entry into an interpreted constructor or instance method must import the
live native globals and publish interpreted changes on ordinary and error return.
An imported class context can outlive its declaration frame without retaining that
frame's scope. The existing module transfer hooks must then receive an owned scope,
not a detached environment snapshot. See with_native_global_sync in
crates/elephc-magician/src/context/runtime_state.rs and the constructor/method
boundaries in crates/elephc-magician/src/ffi/object_construction.rs.

The reverse boundary is equally mandatory: an interpreted call of a native
constructor or instance method publishes its globals before the native body and
refreshes them after it, including errors. Otherwise the outer entry restores its
stale snapshot over writes made by native code. with_eval_global_sync implements
that pair for interpreter/statements/native_constructor_defaults.rs and
interpreter/statements/native_method_execution.rs under crates/elephc-magician/src/.
Registered native functions already had this reverse protocol. No ABI or framework
special case is needed.

Evidence: tests/codegen/eval_global_scope.rs has nine passing native regressions,
including retained-class reads after loader return, native constructor writes and
inherited native method writes on success and exception. Four hook order/ownership
units pass in crates/elephc-magician/src/ffi/context.rs. Host execution was macOS
ARM64 with PHP comparison, not executable coverage of every target.

## Tried and ruled out

The logger's outer unsupported-Match error did not mean match syntax was missing.
The constructor and match worked before its loader's scope expired. LLDB scope
tracing isolated a fallback context with no live global view. Keeping every class
declaration context indefinitely would conceal a lifetime problem by retention.

Adding only the forward transfers passed seven focused tests but regressed the
application's native constructor writes. A TypeError then crashed its object
renderer. Direct-only native construction passed; adding a runtime child exposed
the lost writes. The constructor and inherited-method reducers failed before the
reverse repair and pass after it. Never stop at a forward-only transfer test.

## Watch out

Reusing unchanged AOT objects and swapping only the rebuilt runtime archive made a
controlled old/new diagnostic pair possible: the old pair reproduced the startup
crash; the repaired pair ran the real cache:clear twice with exit zero and empty
stderr. Relinking took about 1.65 seconds; that is not full compilation time.
Artifacts and input hashes are under target/framework-audit-20261020.Wcomr7/.
Fresh CLI compilation and actual HTTP acceptance remain separate gates. They now
pass for a route-less default Welcome fixture: a full release compiler builds its
unchanged public/index.php in web mode, and 500 requests return the standard 404
with HTML identical to PHP except the documented random hue. The binary is about
204 MiB. Build time, 527 warnings, broader acceptance and a roughly 44 KiB/request
worker RSS trend remain open; generic direct/buffered 40 KB web controls plateau.

The full console matrix still has vault/secret-command failures and a callable
reflection discrepancy. Measured static-method, magic-property and string-context
global-transfer gaps are scheduled next; do not extrapolate constructor/method
coverage to every native entry family or claim full PHP/global lifetime parity.
