---
id: bug_fix-the-second-request-per-worker-returned-null-beca-dfaf3146
type: bug_fix
title: "The SECOND / request per worker returned null because a re-required container factory built a NULL string argument"
description: "Traced end to end with ELEPHC_EVAL_TRACE: the failure is one line, and it is not in the controller"
created: 2026-09-18
verified_by: "Probe error_log()s in HelloController::home showed 'has twig => yes' then nothing; the eval trace showed dynamic_method_bind_error on FilesystemLoader::addPath with arg_tags=[\"1:0:<unresolved>\"] and then [probe] twig => null"
sources:
  - path: examples/symfony-app/public/index_preload_hotpath.php
    blob: e2a83018a3e0786e8419f65c83f19d5dee2ebbb5
  - path: examples/symfony-app/var/cache/prod/ContainerXvDiqOo/getTwigService.php
    blob: b9556b9db8e52f70ef2f3d0b0a410f2b0ce8e653
---

# The SECOND / request per worker returned null because a re-required container factory built a NULL string argument

## Fact

SYMPTOM. `index_preload_hotpath` served `/` on request 1 of a worker and answered
`ControllerDoesNotReturnResponseException: the controller ... returned null` on request 2, then
exited 255. `/plain`, `/greet` and `/echo` repeated cleanly. `--web-isolation request` did NOT
change it.

THE CHAIN, from the eval trace (`ELEPHC_EVAL_TRACE=1`, ~3800 lines for two requests):

    phase=dynamic_method_start  Twig\Loader\FilesystemLoader::addPath   (getTwigService.php:23)
    phase=dynamic_method_bind_error method="…::addPath" status=UncaughtThrowable
        arg_count=1 arg_tags=["1:0:<unresolved>:header=None"]
    phase=statement_error status=UncaughtThrowable   (getTwigService.php)
    phase=try_uncaught pending=true catches=1
    [probe] twig => null

`getTwigService::do()` calls `$a->addPath((\dirname(__DIR__, 4).'/templates'))`. On the SECOND
request that one argument arrives as tag 1 (string) with a NULL payload pointer, the parameter bind
throws, the generated container's own `try { } catch (\Exception)` swallows it, `get('twig')`
answers null, and `$twig->render(...)` on null returns null quietly -- so the controller "returns
null" with nothing in the log.

WHY ONLY THE SECOND. The factory file is `require`d at RUNTIME (it is not in the preload list), and
`__elephc_eval_include_request_reset` clears the include-once state between requests, so request 2
re-executes it. Request 1's execution is the one that works.

WORKAROUND IN PLACE: `getTwigService.php` joined the preload list, so the factory is COMPILED and
its `dirname(__DIR__, 4)` is resolved at build time.

THE REAL BUG IS STILL OPEN: a string built by interpreted code re-executed after a per-request
reset can come out as a null pointer. Suspects, in order: the concat arena (`_concat_off` and the
heap arena are both wiped in `__rt_web_reset`), and the per-file `__DIR__` context that
`reset_global_eval_included_files` drops. A single-process double `require` of the same file
reproduces NEITHER -- the reset is required, so any reduction needs the `--web` loop.

HOW TO REPEAT THE DIAGNOSIS: probe the app's OWN controller with `error_log()` (never vendor code),
then run two requests with the eval trace on and read backwards from the first
`status=UncaughtThrowable`.

## Why

It is why the Twig route served every OTHER request, and the diagnosis method is reusable for any --web state bug
