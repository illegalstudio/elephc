---
id: gotcha-a-substr-strrpos-psr-4-autoloader-is-silently-re-4a09c01c
type: gotcha
title: "A substr/strrpos PSR-4 autoloader is silently rejected, and nothing names it"
description: "The symbolic subset that autoload::run evaluates a spl_autoload_register body against omits substr and strrpos, so the canonical hand-written PSR-4 loader resolves no class and fails at run time as Class not found, with no compile-time diagnostic; measured against four autoloader shapes beside php"
created: 2026-09-21
sources:
  - path: docs/php/namespaces.md
    blob: 0d7fa86664f8c16101c7c2de7e9cc293233c579b
    lines: 379-429
    snip: 290f04c6f8c4
    anchor: "The compiler accepts three call shapes for `spl_autoload_register`:"
  - path: tests/compat_prelude_gate_tests.rs
    blob: 78ec52b67ba81ac923c0e3869d8f194ea42d72b2
    lines: 118-124
    snip: 18abd8377865
    anchor: "const MANIFEST: &str = r#\"{\"autoload\":{\"psr-4\":{\"App\\\\\":\"src/\"}}}\"#;"
superseded_by: "bug_fix-the-autoload-subset-now-folds-a-substr-strrpos-p-7476c4ff"
superseded_why: "The subset was widened with substr, strrpos, integer plus and unary minus; the canonical PSR-4 body now folds and nine shapes match php"
---

# A substr/strrpos PSR-4 autoloader is silently rejected, and nothing names it

## Fact

A hand-written PSR-4 autoloader fails to resolve ANY class, silently, when it extracts the
class leaf with `substr()`/`strrpos()` — the two functions missing from the symbolic subset
that `autoload::run` evaluates the closure body against.

`docs/php/namespaces.md:415-429` documents that subset exactly, and it is generous: `str_replace`,
`str_starts_with`, `str_ends_with`, `strtolower`, `strtoupper`, `sprintf` with `%s`, `dirname`,
`basename`, `realpath`, `pathinfo`, `file_exists`, `is_file`, `is_readable`, `is_dir`, `.`
concatenation, variable reads/writes, literal-folding `if`. `substr` and `strrpos` are not in it,
and the documented consequence of anything outside the subset is that the rule is "silently
rejected for that candidate".

So the failure has NO compile-time diagnostic. It surfaces at run time as
`Fatal error: Uncaught Error: Class "App\Probe" not found`, which names the class and never names
the autoloader that was thrown away. That is the expensive half: the message points at the call
site, not at the cause.

MEASURED, four shapes, one variable at a time, against `target/release/elephc`, each run beside
php 8.5.10 on identical source (`scratchpad/latephase/autoshape.sh`):

    closure, LITERAL require path                      MATCH   "reached"
    closure, path via substr()/strrpos() from $class   DIVERGE  Class "App\Probe" not found
    named function, literal path                       MATCH   "reached"
    static method [Cls::class,'load'], literal path     MATCH   "reached"

The discriminator matters: it is NOT the callable shape. Closure, named function and static
method all work. It is the two builtins in the body. A probe that varies the callable and the
path together — which is what I first wrote — blames the wrong one.

WHY THIS COSTS TIME: `substr($class, strrpos($class, '\\') + 1)` is the canonical way to take a
class's leaf name, so this is the shape a hand-written PSR-4 loader usually has. Composer-based
projects never hit it (the PSR-4 manifest index is tried first), which is why Symfony compiles
while a five-line autoloader does not.

WHEN WRITING A PROBE that needs a genuinely autoloaded class — the only way to test that a
usage-gated prelude is injected after `autoload-run` — use a PSR-4 `module.json`
(`{"autoload":{"psr-4":{"App\\":"src/"}}}` plus `src/Name.php`), or a closure with a literal
`require` path. Both are proven to work. `tests/compat_prelude_gate_tests.rs` uses the manifest.

TWO CANDIDATE FIXES, neither done: add `substr` and `strrpos` with literal/symbolic arguments to
the subset (they are pure string functions and the evaluator already carries eight comparable
ones); and emit a compile-time diagnostic when EVERY registered rule is rejected for a candidate
class, which turns a run-time mystery into a message that names the autoloader.

## Why

The callable shape is not the discriminator -- closure, named function and static method all work with a literal path. The two builtins in the body are. A probe that varies both at once blames the wrong one.
