---
id: bug_fix-the-autoload-subset-now-folds-a-substr-strrpos-p-7476c4ff
type: bug_fix
title: "The autoload subset now folds a substr/strrpos PSR-4 loader"
description: "Adding substr, strrpos, integer plus and unary minus to the autoload symbolic subset makes the canonical hand-written PSR-4 autoloader resolve; nine shapes run beside php all match. Carries the php edge cases pinned by measurement and the mutation lesson that a strrpos test is blind unless the body compares against false before any arithmetic"
created: 2026-09-21
verified_by: "Nine autoloader shapes beside php 8.5.10; five mutations each built, run and reverted; --lib comm against baseline empty in both directions"
sources:
  - path: src/autoload/interpret.rs
    blob: 7e697da5ec4b1f055536665d191f90c209cbd147
    lines: 195-320
    snip: 5bdedcb0978b
    anchor: "BinOp::Add => {"
  - path: docs/php/namespaces.md
    blob: 7bc7f7531c1986afe54c8feffbd9754e7a1562cd
    lines: 415-432
    snip: 22e79596d680
    anchor: "The interpreter understands a deliberate subset of PHP, enough for typical autoloaders:"
  - path: tests/codegen/spl/autoload.rs
    blob: 84494c16f0cd2b853d6d3254c2459daa7f400955
    lines: 795-990
    snip: 66409a30758f
    anchor: "fn test_register_with_substr_strrpos_leaf_closure() {"
supersedes: "gotcha-a-substr-strrpos-psr-4-autoloader-is-silently-re-4a09c01c"
---

# The autoload subset now folds a substr/strrpos PSR-4 loader

## Fact

RESOLVED. A hand-written PSR-4 autoloader that extracts the class leaf with
`substr($class, strrpos($class, '\\') + 1)` now folds. `src/autoload/interpret.rs` gained
`substr`, `strrpos`, `BinOp::Add` and `ExprKind::Negate`; `docs/php/namespaces.md` states the
widened subset. Nine autoloader shapes run beside php now MATCH, where four diverged before.

WHY FOUR ADDITIONS AND NOT TWO. Integer `+` is needed because the canonical body is
`strrpos(...) + 1`. Unary minus is needed because the parser keeps `-6` as
`Negate(IntLiteral(6))`, never as a negative literal — so without it NO negative offset or length
can be written at all, and every php-exact negative path is unreachable from source. That was
found by bisect, not by reading: `substr($class, 0, $pos)` folded while `substr($class, 0, -6)`
did not.

THE MUTATION LESSON, which is the part worth carrying to other work. The obvious mutation for
this feature — make `strrpos` return `Int(0)` instead of `Bool(false)` on a miss — is INVISIBLE
to every test whose autoloader body is the canonical one, because `false + 1` and `0 + 1` are
both `1`. Measured: under that mutation 109 tests still passed and the global-namespace test
still resolved. A `strrpos` test only has teeth if the body compares against `false` DIRECTLY,
before any arithmetic. I proposed that mutation as one four existing tests would catch; they
could not, and a fifth test branching on `$pos === false` had to be written for it.

PHP SEMANTICS PINNED BY MEASUREMENT (php 8.5.10), the non-obvious ones:

    strrpos("Probe", "\\")            false        strrpos("\\Probe", "\\")   0     <- the false/0 trap
    strrpos("a\\b\\c","\\", 5)        false              offset == strlen is legal
    strrpos("a\\b\\c","\\", 10)       ValueError         offset outside the haystack throws
    strrpos("a\\b\\c","\\", -5)       false        (-6) ValueError
    strrpos("abXYc","XY", -3)         2            (-4) false
        => the rule is match START <= strlen+offset; the needle MAY extend past that point
    strrpos("abc", "")                3            empty needle lands at strlen
    substr("Hello", 10)               ""           php 8; offset past the end is not false
    substr("Hello", 2, -10)           ""           negative length clamps at 0
    substr("Hello", PHP_INT_MIN)      "Hello"      clamps, does not overflow

A GLOBAL-NAMESPACE CLASS LOSES ITS FIRST CHARACTER, and elephc now REPRODUCES that rather than
improving on it: `strrpos` misses, `false + 1` is `1`, so `substr("Gadget", 1)` is `"adget"` and
php loads `src/adget.php`. There is a test whose fixture is deliberately named `adget.php`.

WHAT ABORTS RATHER THAN GUESSES (returns `None`, so the rule falls through to the next): a
`ValueError` case, any non-string subject or needle that php would coerce, a non-`Int` offset, a
`+` with a string or null operand, a byte window splitting a multi-byte UTF-8 sequence, and named
arguments. A dropped rule falls through; a wrong answer includes the wrong file.

TWO HAZARDS LEFT BEHIND. `Eq` and `StrictEq` are conflated in `values_equal`, so `0 == false`
folds to `false` where php says `true` — pre-existing, and a separate semantics job. And a
`substr` result carries `is_class_name: false` by the established convention, so
`substr($class, $pos+1) === 'Foo'` is case-SENSITIVE in the interpreter while a bare
`$class === 'App\Foo'` is not; that is php-correct but surprising, and `values_equal` is the only
consumer of the flag.

## Why

A global-namespace class legitimately loses its first character in php, and elephc now reproduces that rather than improving on it -- the improvement would load a different file than php does.
