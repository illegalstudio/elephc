---
title: Included-file debug artifacts need source-id markers, not just line/col
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 95 }
touches:
  - src/codegen/block_emit.rs
  - src/codegen/shared_state.rs
  - src/debug_info.rs
  - src/source_map.rs
  - docs/compiling/source-maps.md
  - docs/compiling/output-and-diagnostics.md
  - tests/codegen/cli.rs
---

## Decided

- Codegen now announces physical sources with
  `@file id=<source_id> path_hex=<hex>` before any `@fn`/`@src` marker, and
  stamps `file=<source_id>` on function and instruction markers. The source
  table is emitted once per module, so folded bodies and first-instruction scans
  cannot miss it.
- `--debug-info` assigns one `.file` number per physical path and uses it in
  `.loc`; `--source-map` emits version 3 with a `sources` array and
  `source_id` on functions/mappings/lines. The line inverse index is keyed by
  `(source_id, php_line)`, so equal line numbers in two files stay distinct.
- Main is always source id 0. Its first EIR instructions can come from an
  inlined include body, so scanning for the first span would mislabel `main`.
- The same physical file can be parsed under more than one span identity
  (include variants, autoload probes). Both readers deduplicate by path and
  alias the extra ids to the canonical file number/source id.

## Tried and ruled out

- Passing a source-id/path table from the pipeline into `debug_info` and
  `source_map`: the markers already travel through `--emit-asm`, and the
  assembly stays self-describing. A table would be another side channel to keep
  in sync.
- Raw paths in `@file`: they contain spaces, quotes and newlines. Hex encoding
  keeps the space-separated marker grammar unambiguous and is decoded by both
  readers.
- Keeping source-map version 2: the `lines` key changes from `php_line` to
  `(source_id, php_line)`, so version 2 consumers must treat version 3 as a new
  schema.

## Watch out

- `.file` directives must be emitted before the `.loc` that references them;
  `emit_source_file_markers` runs before function emission for that reason.
- A duplicate `@file` path is normal, not a compiler bug. Without alias
  deduplication, the executable gets two DWARF file entries and the map gets two
  `sources` rows for one file.
- `Span::source_path()` is the only reliable path source; synthetic/dummy spans
  must not acquire a `file=` token.
- The end-to-end regression is
  `test_cli_debug_info_and_source_map_keep_included_source_identity`; it checks
  `.file`/`.loc`, the map `sources` table, and a mapping attributed to the
  included id.
