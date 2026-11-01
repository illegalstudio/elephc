---
id: bug_fix-php-stdin-bounded-readers-share-cursor-across-na-52befb81
type: bug_fix
title: "PHP stdin bounded readers share cursor across native and eval descriptors"
description: "Duplicate stdin and use EOF readers; bridge native STDIN into eval without consuming capped remainder"
tags: [stdin, native-eval]
created: 2026-10-21
verified_by: "cargo test -p elephc --test codegen_tests file_get_contents_stdin_ -- --test-threads=1; cargo test -p elephc --lib stdin_reader_is_emitted_for_every_supported_target"
sources:
  - path: src/codegen_support/runtime/io/file_get_contents_stdin.rs
    blob: 938ae06584e90a847e19a0e11effef87626a599e
  - path: crates/elephc-magician/src/stream_resources/operations.rs
    blob: 108fdfbc0f70e8802b68bac45689917d49e2b5ec
  - path: tests/codegen/io/streams.rs
    blob: 2ba49a17d3074bc1c9625cb4fa0781a1db23f8d0
---

# PHP stdin bounded readers share cursor across native and eval descriptors

## Fact

The filesystem-sized reader cannot implement php://stdin: pipe input has no stat-sized payload, and bounded reads must leave the shared process cursor intact. Native dispatch recognizes the case-insensitive URL once, duplicates fd zero, optionally seeks, reads only the requested bytes with shared stream readers, persists the returned string, and closes only its duplicate. Eval mirrors the descriptor semantics; native STDIN resource payload zero needs an explicit bridge because eval resources use high tagged ids. Preserve close-on-exec for cloned standard streams. Seven focused native tests pass, including opaque eval, large data, bounded cursor sharing, seekable and pipe offsets, validation and closed stdin. Five-target emission is checked separately. Symfony acceptance and native Linux execution remain unverified for this slice. The neighboring wrapped named-params notification callback still crashes in both old and new compiler artifacts; it is an independent open gate.
