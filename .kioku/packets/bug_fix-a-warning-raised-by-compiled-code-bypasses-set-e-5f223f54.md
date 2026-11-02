---
id: bug_fix-a-warning-raised-by-compiled-code-bypasses-set-e-5f223f54
type: bug_fix
title: "A warning raised by compiled code bypasses set_error_handler and writes straight to stderr"
description: "PHP routes every diagnostic through the installed handler and a true return suppresses display; elephc's __rt_diag_warning only checks the @ depth, so a framework in production prints warnings php never shows"
created: 2026-09-20
sources:
  - path: src/codegen_support/runtime/diagnostics.rs
    blob: 5c542ae03fb4a007ebbf9b637e846f214a6af96e
    lines: 54-62
    snip: d2182c72a966
    anchor: "emitter.label_global(\"__rt_diag_warning\");"
  - path: src/web_prelude/build.rs
    blob: 001f734bc97857b43f18914d7d4f59817cba3c5f
    lines: 2005-2015
    snip: 0671538ab9b7
    anchor: "fn decl_fn_set_error_handler() -> Stmt {"
---

# A warning raised by compiled code bypasses set_error_handler and writes straight to stderr

## Fact

`__rt_diag_warning` (src/codegen_support/runtime/diagnostics.rs) does exactly two things: it
reads `_rt_diag_suppression` (the `@` operator's nesting depth) and, if that is zero, issues a
`write(2)` to fd 2. There is no path from it to the callable `set_error_handler` stored --- the
registry exists (`__elephc_error_handler_state`, declared in the web prelude) and nothing in
the compiled diagnostic path ever reads it.

PHP's contract, verified with the probe in scratchpad/errhandler.php:

    set_error_handler(fn($sev, $msg) => (print "HANDLER SAW: $msg\n") && true);
    $a = [0 => 'zero'];
    $x = $a[1];              # php: HANDLER SAW: Undefined array key 1   (nothing displayed)
    restore_error_handler();
    $y = $a[2];              # php: Warning: Undefined array key 2 in FILE on line 19

Two divergences in one:

1. **The handler never runs.** Any program that installs one to log, count or convert
   diagnostics silently loses every diagnostic raised by compiled code.
2. **The default display is wrong.** A handler returning `true` suppresses the message
   entirely; elephc prints it anyway. And the message elephc prints omits the
   ` in FILE on line N` suffix PHP always appends.

How it showed up: a served Symfony page's response bodies were byte-identical to `php -S`, but
the compiled server's stderr carried `Warning: Undefined array key 1` and `... 2` on EVERY
request while `php -S` printed none. The array read is the same on both sides -- PHP's is
swallowed by the framework's handler.

A second, smaller gap found next to it: `set_error_handler` / `get_error_handler` /
`restore_error_handler` are declared only in the `--web` prelude
(`src/web_prelude/build.rs`), so a plain CLI program gets
`Call to undefined function set_error_handler()`. They are standard PHP in every SAPI.

Fixing it needs three things the compiled path does not have today: the handler state
reachable from the runtime helper, a file and line at the raise site (the helper is handed only
a message string), and a bridge call to invoke the callable and honour its return value.

## Why

Found by diffing stderr, not bodies: the Symfony route bodies were byte-identical to php -S while the compiled binary printed two warnings per request that php printed none of.
