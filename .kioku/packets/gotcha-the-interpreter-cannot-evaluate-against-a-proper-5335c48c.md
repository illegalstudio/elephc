---
id: gotcha-the-interpreter-cannot-evaluate-against-a-proper-5335c48c
type: gotcha
title: "The interpreter cannot evaluate ??= against a property array element"
description: "Why a Symfony container fragment left interpreted dies on unsupported NullCoalesceAssign, and what forces it to be preloaded"
tags: [symfony, interpreter, null-coalesce]
created: 2026-09-16
verified_by: "elephc-web worker log, examples/symfony-app APP_ENV=prod, GET /"
sources:
  - path: examples/symfony-app/public/index_preload_hotpath.php
    blob: 94669d72c30992d456aa8f1ac80c9c47191550bd
---

# The interpreter cannot evaluate ??= against a property array element

## Fact

Symfony's generated container fragments are full of

    $a = ($container->services['request_stack'] ??= new RequestStack());

and a fragment that stays INTERPRETED dies on it:

    Fatal error: eval() runtime failed: unsupported NullCoalesceAssign expression
    in var/cache/prod/ContainerXnM4g5h/getErrorHandler_ErrorRenderer_HtmlService.php on line 21

Compiled code handles the same shape. The workaround is to PRELOAD the fragment so it is compiled
AOT, which is what `public/index_preload_hotpath.php` does.

Non-obvious trap when building that preload from `get_included_files()`: a collector that calls
`Request::create('/')` in-process never renders the HTML error page, so this fragment is absent
from the trace even though a real HTTP 404 loads it. The generated list has to be topped up by
hand, or collected from an actual HTTP request instead.
