---
id: gotcha-preloading-composer-s-autoloader-makes-the-app-r-502a703c
type: gotcha
title: "Preloading Composer's autoloader makes the app return an empty 200 that LOOKS 5x faster"
description: "Why vendor/autoload.php and its machinery must stay dynamic in a --web preload, and how the failure disguises itself as a speedup"
tags: [symfony, preload, performance]
created: 2026-09-16
verified_by: "HTTP 200 SIZE=0 with 1.4 ms latency vs the correct HTTP 404 SIZE=993 at ~8 ms, examples/symfony-app APP_ENV=prod"
sources:
  - path: examples/symfony-app/public/index_preload_hotpath.php
    blob: 790e6364a75364450b1ac8e614460e012199f11c
---

# Preloading Composer's autoloader makes the app return an empty 200 that LOOKS 5x faster

## Fact

`vendor/autoload_runtime.php` opens with

    if (true === (require_once __DIR__.'/autoload.php') || empty($_SERVER['SCRIPT_FILENAME'])) {
        return;
    }

A `require_once` that ALREADY RAN evaluates to `true`, so as soon as `vendor/autoload.php` is in
the preload — directly, or transitively because the compiler followed a static require chain into
it — the runtime returns before the application starts.

The symptom is not an error. It is **HTTP 200 with an empty body**, and the request gets FASTER:
1.4 ms instead of ~8 ms, because nothing runs. A benchmark alone reports that as a 5x win. Only
the byte-comparison against `php -S` catches it.

Measured while trying to compile Composer's bootstrap to remove `__elephc_eval_include` from the
profile (it was ~44% of non-idle samples). The composer machinery, the `files` polyfill
bootstraps, and `vendor/autoload.php` all have to stay dynamic; the gain is not available this
way.

RULE: every `--web` performance measurement on this app must be paired with
`diff <php -S body> <elephc body>`. A number without that diff means nothing.
