---
id: bug_fix-autoload-was-breadth-first-and-an-autoloaded-int-c0aed21c
type: bug_fix
title: "Autoload was breadth-first and an autoloaded interface never activated, so interface_exists() answered false for the whole program"
description: "Two independent bugs that met in symfony/string's AsciiSlugger: the dependency landed after the file that demanded it, and nothing ever wrote the activation cell interface_exists() reads"
created: 2026-09-18
verified_by: "tests/codegen/spl/autoload.rs::test_an_autoloaded_file_scope_guard_sees_the_interface_it_implements and tests/codegen/type_builtins/includes/declaration_activation.rs::test_an_entry_program_interface_is_visible_to_interface_exists; both fail with ELEPHC_AUTOLOAD_LEGACY=1 and pass without. error_tests 1431/99 identical names, codegen::regressions 408/6 identical, codegen::types 343/16 identical, spl::autoload 104/2 identical (and test_interface_exists_literal_triggers_autoload went from failing to passing)"
sources:
  - path: src/autoload/mod.rs
    blob: 8dec7d8d7797e722093d1dd98abc1f5745f6511a
  - path: src/autoload/walk.rs
    blob: fee9ee9c4d14e4fd8d58715c792b99a29a21e955
  - path: src/codegen_support/runtime/system/rt_interface_exists.rs
    blob: a68e1f8f67581399744363bee68ca1c1bb81ecec
  - path: src/codegen/classlike_activation.rs
    blob: ff531a2c7f1f6ddd2c36e1aea9dc3f0ba8c0d4bf
---

# Autoload was breadth-first and an autoloaded interface never activated, so interface_exists() answered false for the whole program

## Fact

SYMPTOM. `index_preload_hotpath.php` compiled and linked, then died before `echo` #1:

    Fatal error: Uncaught LogicException: You cannot use the
    "Symfony\Component\String\Slugger\AsciiSlugger" as the "symfony/translation-contracts"
    package is not installed.

The package IS installed. `php -n` says `interface_exists(LocaleAwareInterface::class)` is true.

MINIMAL REPRO, no Symfony (scratchpad/gprobe): composer.json with `psr-4: {"P\\": "src/"}`,
`src/Dep.php` declaring `interface Dep {}`, `src/Guarded.php` opening with
`if (!interface_exists(Dep::class)) { throw ... }` then `class Guarded implements Dep`, and a
`main.php` that does `new \P\Guarded()`. Threw.

BUG 1 -- BREADTH-FIRST SPLICING. `autoload::run` collected reference points over the whole program
and inserted each discovered file AT ITS OWN FIRST REFERENCE POINT. That is breadth-first. PHP's
loader is DEPTH-FIRST: asking for `AsciiSlugger` runs the loader again for `LocaleAwareInterface`
before the class binds. The trace (`ELEPHC_AUTOLOAD_TRACE=1`, temporary) showed it exactly:

    ref_idx=34 insert_at=86 len=2 decls=...AsciiSlugger          <- guard at 86, class at 87
    ref_idx=41 insert_at=95 len=1 decls=...LocaleAwareInterface   <- NINE statements too late

`included` is keyed by path, so once the interface was claimed for position 95 the guard's own
reference point could not move it. FIX: `load_autoloaded_bundle` loads a file together with the
files its FILE-SCOPE execution demands, dependency-first, capped at depth 64 with `included`
inserted before the recursion so a cycle terminates. `walk::collect_file_scope_dependencies`
supplies the demands: the binding dependencies of every declaration (extends/implements/trait use)
plus the reference points of the file's executable top-level statements. Method bodies are
DELIBERATELY EXCLUDED -- following those drags the transitive vendor closure to the front of the
program. Side effect: the fixpoint made fewer, bigger insertions (442 splice events -> 281).

BUG 2 -- THE ACTIVATION CELL NOTHING WROTE. Ordering alone did not fix it. `interface_exists`
lowers to `__rt_interface_exists`, which searches `_interface_activation_table` FIRST and returns
that entry's request cell; `collect_activation_registry_names` gives a cell to EVERY interface with
a real span. The cell is only ever written by a `ClassLikeActivate` event, and the only thing that
emitted one was `resolver::declarations::strip_stmt`, i.e. INCLUDED files. An autoloaded file never
travels that path, and neither does the entry file's own declaration. Measured on a 12-line program
with no autoloading at all:

    interface_exists('A') before the decl / after / after a class implements it / class_exists
    php    -> 1:1:1:1        elephc -> 0:0:0:1

So `interface_exists()` was false for EVERY interface outside an include. In the emitted asm the
cell is `.comm _classlike_active___interface___p_N_dep, 8, 3`, referenced by the table and written
by nothing.

FIX: `autoload::activate_interface_declarations` adds the event next to each interface an
autoloaded file declares -- KEEPING the declaration, because unlike the include path nothing has
extracted it yet -- and hoists the event to the front of the file's statements when the interface
`extends` nothing, which is PHP's early binding. `activate_entry_interface_declarations` does the
same for the entry program, from `pipeline::compile` and from the `tests/codegen/support/compiler`
harness, which mirrors the pipeline by hand.

THE TRAP IN FIX 2, and it cost two wrong versions. An INCLUDED file's declarations are HOISTED to
the entry program's top level while its activation event stays behind at the include site, nested
and possibly never reached. From the entry pass those declarations are indistinguishable from the
entry file's own. Adding a second event for one is a runtime redeclaration -- `Cannot redeclare
interface Symfony\...\KernelInterface` on a front controller that `require_once`s a vendor
interface file -- and adding one for a file the program never enters makes `interface_exists()`
answer true for an interface PHP never declared (5 `declaration_activation` tests caught this). The
rule that works is: skip any interface that ALREADY carries an activation event anywhere in the
program, at any depth. An earlier attempt keyed on "the autoload registry cannot resolve this name"
instead; it fixed the redeclaration and left the unentered-source tests failing.

## Why

It was the boot blocker on the compiled Symfony container: the app threw 'the symfony/translation-contracts package is not installed' before the entry file's first statement ran
