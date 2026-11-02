---
id: gotcha-a-usage-gated-prelude-must-be-injected-after-aut-c38be2db
type: gotcha
title: "A usage-gated prelude must be injected after autoload-run, or it never sees the caller"
description: "The CLI error prelude sat in the web-prelude phase and its gate saw only the 21-line entry; a required file is visible there and an autoloaded class is not, so the binary died at run time on an undefined function"
created: 2026-09-21
sources:
  - path: src/pipeline.rs
    blob: fe7f75c6e8ad30939dbbd9755b350617e2dbc092
    lines: 520-600
    snip: aad9bed6a81e
    anchor: "timings.record_since(\"autoload-run\", phase_started);"
  - path: src/error_handling_prelude.rs
    blob: d49d138f4c0fe6b723c423a5cee503133b697470
    lines: 700-726
    snip: 5c774ed44b86
    anchor: "pub(crate) fn without_shadowing_declarations(program: Program) -> Program {"
---

# A usage-gated prelude must be injected after autoload-run, or it never sees the caller

## Fact

A prelude whose injection is gated on "does the program name this?" must run in the
`compat-preludes` phase of `src/pipeline.rs`, AFTER `autoload-run` — never in the earlier
`web-prelude` phase. That phase's own comment already states the rule:

    Inject compatibility functions only after autoload expansion has exposed the complete
    closed-world program.

The failure it prevents is silent and looks like the gate is broken. The CLI error-handling
prelude was placed in the `web-prelude` phase and its gate saw only `bin/console` -- 21 lines
that mention nothing -- so nothing was injected and the binary died at RUN time with
`Call to undefined function error_reporting()`. The call lives in
`Symfony\Component\Runtime\Internal\BasicErrorHandler`, a class the autoload pass splices in
at `autoload-run`, which is roughly 90 lines further down the pipeline.

The tell, if you are bisecting this: a `require`d file IS visible to a gate in the early phase
and an AUTOLOADED class is NOT. A two-file probe passes and the real program fails.

    # passes with the gate in either phase
    lib.php:  function probe(): int { return error_reporting(); }
    main.php: require __DIR__ . "/lib.php"; echo probe();

    # only passes with the gate in `compat-preludes`
    a class reached by the autoloader that calls the same function

Check the emitted assembly to tell "not injected" from "injected but broken": the NAME appears
either way (the runtime's undefined-function message carries it), the MANGLED SYMBOL only when
the declaration was really compiled.

    grep -c "error_reporting" out.s        # 2 -- the error string
    grep -c "_fn_error_u_reporting" out.s  # 0 -- never declared

Neither the `--web` suite (52 tests) nor the CLI error-surface suite (8 tests) caught this,
because every probe in both is a single file whose entry names the surface itself. A regression
test for this needs a SECOND file reached by autoload, not by `require`.

## Why

Two test suites totalling 60 tests missed it because every probe is a single file that names the surface itself; the real program names it from a class the autoloader brings in 90 pipeline lines later.
