---
title: PDO_PGSQL ATTR_CHUNK_SIZE client-side chunking
author: Guillaume
date: 2026-11-26
branch: feat/php-8.6-support
touches:
  - crates/elephc-pdo/src/lib.rs
  - crates/elephc-pdo/src/pg.rs
  - crates/elephc-pdo/src/pg_libpq.rs
  - docs/php/pdo.md
  - docs/php/php-8.6.md
  - src/pdo_prelude/bridge_externs.rs
  - src/pdo_prelude/build.rs
  - tests/codegen/pdo_pgsql.rs
---

## Decided
- Implement PHP 8.6 `Pdo\Pgsql::ATTR_CHUNK_SIZE` as client-side grouping over the existing demand-row streams, not `PQsetChunkedRowsMode`, so it works with every PostgreSQL server version and with both the pure-Rust and libpq drivers.
- Resolve precedence in the PDO prelude exactly like php-src: constructor/connection default; prepare option overrides; explicit `ATTR_PREFETCH` clears an inherited chunk; a positive chunk wins over a buffering prefetch; a scrollable cursor rejects an explicit statement chunk and ignores an inherited one; 0 opts out.
- Publish the active chunk length through the existing `conn.changes`/`rowCount` path after every pgsql fetch, including prefetch-off single rows (rowCount 1, matching php-src 8.5/8.6).

## Tried and ruled out
- Relying on libpq 17 `PQsetChunkedRowsMode`: the pure-Rust tokio-postgres backend has no equivalent, and the feature must not require a newer server/client.
- Leaving prefetch-off rowCount at 0: php-src reports 1 for the single-row chunk; the old test expectation was a deviation.

## Watch out
- The PDO prelude is transcribed; edits in `src/pdo_prelude/build.rs` must keep the AST shape and oracle expectations. The `ATTR_CHUNK_SIZE` class constant is 8.6-only, while the numeric attribute branch is shared.
- Chunked statements must stay unbuffered even when `ATTR_PREFETCH` is true; `set_chunk_size` recomputes `buffered` from both fields.
