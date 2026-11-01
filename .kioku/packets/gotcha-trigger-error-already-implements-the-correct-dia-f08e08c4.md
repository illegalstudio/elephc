---
id: gotcha-trigger-error-already-implements-the-correct-dia-f08e08c4
type: gotcha
title: "trigger_error already implements the correct diagnostic dispatch; engine-raised warnings just never reach it"
description: "The prelude's trigger_error consults error_reporting, the handler and its level mask before rendering; __rt_diag_warning renders unconditionally, so the fix is to route engine diagnostics through the path that already exists"
created: 2026-09-20
sources:
  - path: src/web_prelude/build.rs
    blob: 001f734bc97857b43f18914d7d4f59817cba3c5f
    lines: 2176-2200
    snip: 70b7315d2598
    anchor: "fn decl_fn_trigger_error() -> Stmt {"
  - path: src/codegen_support/runtime/diagnostics.rs
    blob: 5c542ae03fb4a007ebbf9b637e846f214a6af96e
    lines: 54-62
    snip: d2182c72a966
    anchor: "emitter.label_global(\"__rt_diag_warning\");"
---

# trigger_error already implements the correct diagnostic dispatch; engine-raised warnings just never reach it

## Fact

`decl_fn_trigger_error` in the web prelude does exactly what PHP does:

    if (error_level & error_reporting()) === 0        -> return true, render nothing
    handler = __elephc_error_handler_state()
    if handler !== null && (error_level & handler_mask) !== 0
        -> return (bool) handler(error_level, message, __FILE__, __LINE__)
    ...only then render "Notice: ..." to STDERR

So the dispatch is written, correct and already in PHP form. What does NOT use it is every
diagnostic the ENGINE raises -- undefined array key, undefined variable, array offset on null,
the NaN/bool coercion warning, and the interpreter's `values.warning()`, which goes to
`__elephc_eval_warning` and lands in the same place. All of them reach
`__rt_diag_warning`, which checks only the `@` suppression depth and then `write(2)`s to fd 2.

Consequences, both observed: a framework's handler never sees an engine diagnostic, and the
message is displayed even when a handler would have returned `true` to suppress it. On a served
Symfony page that is two `Warning: Undefined array key` lines per request that `php -S` does
not print.

The work is therefore smaller than "build dispatch":

1. lift the dispatch out of `trigger_error` into a prelude function both can call
   (it is already AST-built, so this stays within the "preludes must be AST" rule);
2. give `__rt_diag_warning` a bridge entry that calls it and reports whether it was handled,
   falling back to the current `write(2)` when it was not;
3. supply the missing input -- the raise sites hand the helper only a message string, with no
   file or line, and PHP's contract passes both to the handler AND appends
   ` in FILE on line N` to the default rendering, which elephc currently omits too.

Note `set_error_handler` / `get_error_handler` / `restore_error_handler` are declared only in
the WEB prelude, so a CLI program still gets `Call to undefined function set_error_handler()`.
And `tests/web_session_tests.rs` covers the registry (set/get/restore return values) but never
asserts the handler is invoked, which is why the gap survived.

## Why

It turns a 'design and build dispatch' job into a 'reach the dispatch that is written' job, and names the one input the runtime helper is missing.
