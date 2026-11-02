---
id: gotcha-preloading-the-controller-chain-puts-a-compiled--979baebd
type: gotcha
title: "Preloading the controller chain puts a compiled Symfony route under 10ms; the Twig route stays interpreted"
description: "Measured p50s for both compiled --web binaries against php -S, and the two things that decide which side of 10ms a route lands on"
created: 2026-09-18
verified_by: "scratchpad/bench-one.sh, 15 warm-up requests then 40 samples, HTTP code recorded with every sample so a dead worker cannot pass as a 0.3ms response; routes byte-diffed against php -S by scratchpad/verify-hotpath.sh first"
sources:
  - path: examples/symfony-app/public/index_preload_hotpath.php
    blob: e8b652d89f5a0ff3c110d05122a91cc8517e574b
  - path: examples/symfony-app/public/index.php
    blob: c0037a8db4a454268d8c843a351964bed75d0df2
---

# Preloading the controller chain puts a compiled Symfony route under 10ms; the Twig route stays interpreted

## Fact

MEASURED, same machine, same minute, machine load average 44 (other sessions were running full test
suites -- the p50s are inflated and the MINs are the honest figures):

    route            index (baseline)      index_preload_hotpath     php -S
    /plain           p50 61ms (min 41)     p50 9.8ms  (min 8.8)      2.8ms
    /echo?a=1        --                    p50 9.8ms  (min 9.1)      --
    /greet/Bob       --                    p50 16.1ms (min 9.4)      --
    / (Twig)         p50 148-252ms         still interpreted         2.1-3.0ms

So a route whose controller is COMPILED is now under 10ms. `/` is not, and the reason is not the
framework: it is that `twig/twig` does not compile yet, so `Environment`, `Template`, the escaper
and the generated template class all run in the interpreter.

WHAT MADE THE DIFFERENCE. Four `require_once` lines added to the preload list:
`service-contracts/ServiceSubscriberInterface.php`,
`framework-bundle/Controller/AbstractController.php`, `src/Controller/HelloController.php` and
`var/cache/prod/ContainerXvDiqOo/getHelloControllerService.php`. Without the LAST one the container
requires the factory at run time, the interpreter declares it, and its
`new \App\Controller\HelloController()` builds a dynamic object that every compiled builtin reads
as a stdClass -- see the packet on that. Adding the factory alone is not enough either: the class
and its base have to be in the closed world for the compiled `new` to exist.

HOW TO MEASURE IT WITHOUT FOOLING YOURSELF, in order:
 1. `verify-hotpath.sh` FIRST. A response that is empty or an error page looks like a speedup.
 2. Record the HTTP code with every timing sample. A build where `/` succeeds ONCE per worker and
    then returns null scored `p50=0.0004s` before the codes were checked -- 37 of 40 samples were
    dead-worker connection refusals.
 3. Read the load average. At load 32 the same binary measured 12.0ms for `/plain`; at load 44 it
    measured 9.8ms; at load 9 the baseline measured 61ms where it had measured 93ms.

STILL OPEN on the preload build: a SECOND `/` request in the same worker returns null from the
controller (`ControllerDoesNotReturnResponseException`), so only the first Twig render per worker
succeeds. `/plain`, `/echo` and `/greet` repeat 40/40 without a miss, so it is specific to the Twig
render path, and it appeared only once `AbstractController` was compiled.

SEPARATELY, and PRE-EXISTING: the sixth request in one worker of the BASELINE build dies -- SIGSEGV,
or `TypeError: Argument must be of type ...BufferingLogger|null, object given` -- after the
sequence `/ /plain /greet/world /greet/accented /echo`. Proven pre-existing by building the same
entry with the autoload changes disabled and getting the identical failure at the identical step.

## Why

It is the first measurement that separates 'elephc is slow' from 'this route is interpreted', and it names the file list that made the difference
