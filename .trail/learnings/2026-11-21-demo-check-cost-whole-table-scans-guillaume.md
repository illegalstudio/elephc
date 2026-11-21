---
title: "Demo check cost: whole-table scans per call site"
author: Guillaume
date: 2026-11-21
branch: bead-check-prof
cost: { minutes: 102, tokens: 608545 }
touches:
  - src/types/checker/mixed_storage_scan.rs
  - src/pipeline.rs
---

A sample of the Symfony Demo --check (macOS sample on the release binary, several 60 s samples) put 90-95% of the type check in mixed_storage_scan::disqualify_named_method_arguments: it filtered every class's instance and static methods for the called name at every method-call node of every body on every method-pass round (call sites x methods). Cheap to see, invisible in the generics counters (the check, not instantiation, was the cost). Fix: Checker::named_method_sigs indexes (class, method key, static) by lowercased name once per class table (rebuilt when classes.len() changes, reset each method-pass round) and reads signatures fresh from classes; counter NAMED_METHOD_TABLE_SCANS pins it. After it, the next frame was pipeline::extends_a_php_load_fatal_class running realpath on every class file per refused file (getattrlist, 57% of front-end samples); it now shares the per-build canonical path cache. First-process check went 1826 s to 132 s wall at equal load. Method: sample the process during the phase, sort by top of stack, then look at the single function with the huge share before theorizing about generics. Remaining cost is structural: 2 erasure reruns (3 processes), and each process checks 1 + autoload rounds times.
