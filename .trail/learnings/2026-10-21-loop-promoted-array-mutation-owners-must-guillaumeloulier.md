---
title: "Loop-promoted array mutation owners must be cleared, not merely released"
author: Guillaumeloulier
date: 2026-10-21
branch: reconcile/dirname-symfony
touches:
  - src/ir_lower/context.rs
  - tests/codegen/references.rs
  - src/ir_lower/tests/reference_loop_cleanup.rs
---

## Decided

Consuming local array mutations must retire and clear their old boxed owner through
the existing deferred ReleaseLocalSlot machinery. A LoadLocal followed by Release
does not clear the referenced value. When a later PromoteLocalRefCell reaches that
operation through a loop back-edge, the storeback releases the same box again.
prepare_mutated_local_owner_impl and store_mutated_local_impl now reuse the
representation-aware overwrite cleanup in src/ir_lower/context.rs.

Evidence: three executable regressions failed before the change with null nested
paths or a heap-debug bad refcount. Four executable regressions now pass against
PHP 8.5.10, including indexed and associative parents, dynamic keys, recursive
escaping paths and destructuring; the ownership regression reports a clean heap.
The EIR regression emits correctly on all five supported targets. See
tests/codegen/references.rs and src/ir_lower/tests/reference_loop_cleanup.rs.
Native execution here was macOS ARM64, not native Linux execution.
All 66 codegen reference tests also pass. A fresh standard bin/console build now
gets cache:clear past the PhpDumper nested-array crash; it exits with a separate
dynamic logger/global-view failure instead. This closes that crash, not the command.

## Tried and ruled out

Simple escaped-reference/destructuring probes already passed. Destructuring alone
was not the cause. The separate C-int PCNTL status fix gets past an earlier crash
but does not repair nested-array lifetime semantics. No application/vendor source
or framework-specific production branch is needed for this repair.

## Watch out

The real container cache-clear command is still unaccepted. Evidence lives under
target/framework-audit-20261020.Wcomr7/console-isolated.zi471L/native-reference-loop-cache-clear/.
Direct append and direct string-key mutation neighbors still leak 4 blocks/256
bytes and 7 blocks/1360 bytes respectively while returning PHP-equivalent values.
Their consuming runtime paths and Mixed wrapper owners need an independent
retirement/publication audit; do not extrapolate one clean nested-path heap
regression to all writes. A destructor-observability probe also fails on both old
and new compilers with an invalid allocation-size request; that pre-existing
representation defect remains open.

Parse new Rust tests before an expensive build, especially around raw PHP strings:
rustfmt with emit stdout and skip_children validates syntax without editing files.
Use a focused development-profile build for tiny TDD cycles, and reserve an
optimized compiler for application timing. Scope Cargo to package elephc explicitly
so selecting lib does not build every workspace member's unit harness.
