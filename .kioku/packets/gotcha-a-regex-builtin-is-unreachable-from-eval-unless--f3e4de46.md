---
id: gotcha-a-regex-builtin-is-unreachable-from-eval-unless--f3e4de46
type: gotcha
title: "A regex builtin is unreachable from eval unless the COMPILED program calls one too"
description: "Interpreted preg_match fatals with 'unsupported Call expression' unless the compiled program also calls a regex builtin; declaring pcre2 in elephc.toml is not enough, and the message names the wrong culprit because it is dispatch.rs's generic fallback"
created: 2026-09-22
sources:
  - path: crates/elephc-magician/src/interpreter/builtins/regex/preg_match.rs
    blob: 04dc2025c9e46f08edf53e771c566143d1c16cc7
  - path: crates/elephc-magician/src/interpreter/statements/dispatch.rs
    blob: e9bb12f43ce430f22c9a13c0f50ee4e350db1af0
---

# A regex builtin is unreachable from eval unless the COMPILED program calls one too

## Fact

Interpreted code calling `preg_match` fatals with `unsupported Call expression` unless the
COMPILED program also calls a regex builtin. Measured as a one-line A/B on macos-aarch64 with the
same native cache and the same project manifest (which DOES declare pcre2 = "10.47"):

  pre2.php  namespaced fn -> eval('return preg_match("/b/", "abc");')
            => Fatal error: eval() runtime failed: unsupported Call expression
  pre3.php  identical, plus ONE top-level compiled `preg_match('/b/', 'abc')`
            => 1 / 1 / 1, byte-identical to php 8.5.10

Declaring pcre2 in elephc.toml is NOT enough; the compiled program must USE it. The native
dependency is linked from compiled call sites only, and the interpreter's own need for it is not
counted.

IT IS NOT "builtins need a compiled use". Seven builtins called only from eval all work with no
compiled use at all: strtoupper, str_pad, ucwords, iconv, preg_quote, array_product, levenshtein.
`preg_quote` is the tell -- a regex-named builtin that needs no pcre2, and it works. The rule is
about the NATIVE PACKAGE, not about eval.

THE MESSAGE NAMES THE WRONG CULPRIT, TWICE OVER. `unsupported Call expression` and
`unsupported NamespacedCall expression` are the generic fallback in
interpreter/statements/dispatch.rs:44, emitted when a handler returns RuntimeFatal without
describing itself. Both expression kinds ARE implemented (expressions.rs:378 dispatches
NamespacedCall; FunctionDecl is handled at dispatch.rs:196). Reading those messages literally sends
you to the parser or to namespace resolution, which is where two hours went. The same fallback hid
a `define_function` failure behind `unsupported FunctionDecl statement`.

WHY IT MATTERS: `Symfony\Polyfill\Mbstring\Mbstring::mb_strtoupper` runs interpreted and calls
`preg_match('//u', $s)` at Mbstring.php:326, so every polyfilled mb_* call dies this way in a
program whose compiled half happens to use no regex.

SEPARATE, SMALLER: compiled `preg_match('//u', "\xC3\x28")` returns int(0) where php returns
false. Both are falsy, so Mbstring's own `if (!preg_match(...))` is unaffected, but a `=== false`
caller would diverge.

## Why

It is the shape behind Symfony's mbstring polyfill failing at Mbstring.php:326, and the fatal sends you to the wrong subsystem
