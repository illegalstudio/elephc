---
id: gotcha-a-byte-identical-route-gate-does-not-prove-the-c-ac4dccd0
type: gotcha
title: "A byte-identical route gate does not prove the code is compiled"
description: "The interpreter silently covers whatever the compiler did not take, so a half-compiled binary answers every route identically and 2.3x slower. Two causes, both silent: elephc writes next to the source so every build shared public/index and clobbered the last, and --ini opcache.preload is ignored unless the preload file is already in the closed world. Pair every gate run with a compiled-ness check"
created: 2026-09-21
verified_by: "Two binaries of the same app measured minutes apart at load 14: 110 MB with 0 container symbols at 16.15ms, 174 MB with 1191 at 6.97ms, both 7/7 byte-identical to php -S"
sources:
  - path: src/pipeline.rs
    blob: e5c499f758110f8cca99b80d5319101482f8b0c4
---

# A byte-identical route gate does not prove the code is compiled

## Fact

A BYTE-IDENTICAL ROUTE GATE DOES NOT PROVE THE CODE IS COMPILED. The interpreter silently covers
whatever the compiler did not take, so a half-compiled binary answers every route exactly like a
fully compiled one — only slower. `verify-index.sh` returning 7/7 `fail=0` says the program is
CORRECT; it says nothing about how much of it is native. Two binaries of the same app, measured
minutes apart at load 14, both 7/7 identical to `php -S`:

    public/index        110 MB   ContainerXvDiqOo strings = 0      /plain 16.15ms   / 25.59ms
    public/index_eager  174 MB   ContainerXvDiqOo strings = 1191   /plain  6.97ms   /  9.70ms
    php -S                                                          /plain  1.12ms   /  1.38ms

2.3-2.6x apart, same responses. So ALWAYS pair the gate with a compiled-ness check. The cheap one
is `strings <binary> | grep -c <a marker only compiled code carries>` plus the binary size;
`ContainerXvDiqOo` works for a Symfony build because the DI container is the thing the preload
decides.

TWO CAUSES, BOTH SILENT:

1. **A SHARED OUTPUT PATH.** `elephc` writes next to the source and ignores `-o`, so every script
   compiling `public/index.php` wrote `public/index`. A 178 MB build finished at 13:39 and a
   different 110 MB build overwrote the same path at 13:44. Every latency, census and symbol
   measurement taken afterwards described a binary nobody had built on purpose — including a
   `--keep-symbols` twin whose symbol table I then cross-referenced against a trace from the OTHER
   binary, which produced the wholly false conclusion "the container is compiled and re-declared
   anyway". GIVE EVERY BUILD ITS OWN OUTPUT NAME (copy the entry to `index_<tag>.php`).

2. **`--ini opcache.preload=FILE` IS IGNORED UNLESS FILE IS ALREADY IN THE CLOSED WORLD.** The
   compiler warns and continues:

       warning: opcache.preload: '…/elephc-preload.php' is not in this binary's compile-time
       OPcache script manifest (the entry file, its statically-resolved includes, and its
       autoloaded files), so it is not compiled into the binary

   That is a chicken-and-egg: the preload file exists to ADD sources to the closed world, and it
   is honoured only if it is already reachable from the entry. The warning is one line in a
   600-warning build log and decides whether the DI container is compiled at all. Grep for it
   explicitly, or promote it — `build-symfony.sh` now prints "preload honoured? NO" and the
   container symbol count.

THE CORRECTED NUMBERS, on a binary that carries the container: **3681 eval crossings per request**
on `/plain` (not 11374), **11 interpreted includes** (not 71), **6 class declarations** (not 55).
The earlier figures are all from the clobbered binary and must not be quoted.

SMELL TEST FOR NEXT TIME: this app's `--web` binary is 174-263 MB. Anything materially smaller is
missing part of the closed world, whatever the route gate says.

## Why

Cross-referencing a symbol table from one binary against a trace from another produced the wholly false conclusion that the DI container was compiled and re-declared anyway. The corrected per-request census is 3681 crossings, 11 includes, 6 class declarations -- not 11374, 71 and 55.
