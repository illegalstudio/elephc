---
id: gotcha-the-bigger-the-web-preload-the-sooner-an-interpr-e2f0a28b
type: gotcha
title: "The bigger the --web preload, the sooner an interpreted ??= meets a COMPILED container"
description: "Why the 192-file preload builds and then kills every worker, and what has to be fixed to use it"
tags: [symfony, magician, web]
created: 2026-09-16
verified_by: "scratchpad/verify-max.sh: 10 workers died on startup, scratchpad/max-srv.log"
sources:
  - path: examples/symfony-app/public/index_preload_max.php
    blob: 042917593b8e769b60d600cf42398e474b9eb42a
  - path: crates/elephc-magician/src/interpreter/expressions/null_coalesce_assign.rs
    blob: d37be5634a2cfb65696fbd2fe133b42dcf57b806
superseded_by: "bug_fix-a-runtime-declared-subclass-reaches-a-compiled-p-5b66a254"
---

# The bigger the --web preload, the sooner an interpreted ??= meets a COMPILED container

## Fact

Compiling 192 of a real request's 227 files (everything except symfony/cache) type-checks clean and
links. It then dies on the FIRST request:

    Fatal error: eval() runtime failed: unsupported NullCoalesceAssign expression in
    var/cache/prod/Container*/getErrorHandler_ErrorRenderer_HtmlService.php on line 21
    elephc-web: 10 workers died on startup

Line 21 is `$a = ($container->services['request_stack'] ??= new RequestStack());`. Container
fragments live in `var/cache` and are `require`d at runtime, so they are always INTERPRETED — but
with the larger preload the `$container` they write through is a COMPILED object, and the
interpreter's `??=` cannot reach an array element inside a compiled object's property.

The 165-file hot-path preload does not hit it and still answers a byte-identical 404 (993 bytes).
So the order of work to grow the preload is: fix the interpreted `??=` against compiled property
storage FIRST, then the symfony/cache set (by-reference arguments that do not define their
variable, the six-member Redis union, APC_ITER_KEY, touch() timestamps).
