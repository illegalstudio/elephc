---
title: Capture values need three limits and one explicit redaction flag, not a global formatter change
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 140 }
touches:
  - crates/elephc-debug/src/value.rs
  - crates/elephc-debug/src/lib.rs
  - src/codegen/block_emit.rs
  - src/debug_metadata.rs
  - src/monitor/calls.rs
  - src/monitor/call_html.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/profiling.md
  - docs/compiling/debug-metadata.md
---

## Decided

- Call capture gets its own bounded renderer instead of changing the DAP/terminal
  `render`/`describe` output. The limits are 256 string bytes, 16 container
  children and 3 nested container levels.
- Markers live in the rendered value so the text table, JSONL `value` and HTML
  panel cannot disagree:
  - long string: `string(<full length>) "prefix…<N bytes omitted>"`;
  - wide container: `… +N more` after the 16th child;
  - deep container: `…<depth limit>` at depth 3.
- `render_capture` returns `CaptureRender { text, truncated, string_bytes_omitted,
  children_omitted, depth_truncated }`. JSONL argument/return records add
  `"truncated": true` and an `overflow` object with only the applicable fields.
  HTML adds `truncated` badges. Strings are read only up to the 256-byte prefix,
  so a very large string is never materialized for the report.
- `#[\\SensitiveParameter]` is authoritative for redaction. Codegen emits
  `sensitive=1` on the `@local` marker by reading the function signature's
  parameter attributes; `.edbg.json` carries `sensitive`; the capture arguments
  become `"value": "<redacted>"`, `"redacted": true`, with the type retained.
  `types` mode keeps the redaction flag but has no value.
- The DAP/terminal locals view is unchanged: this is a call-capture policy, not a
  global value-printing policy. `elephc_dump` uses the same capture renderer, so
  explicit snapshots are bounded/truncated too.
- Event-ring overflow is explicit in every surface: text/HTML already report
  dropped events; `--calls-out` now appends
  `{"kind":"capture_overflow","dropped":N}`.
- Overflow accounting is summed through nested children: a truncated child makes
  the parent `truncated` and contributes its string/child/depth counts.

## Tried and ruled out

- Lowering `MAX_STRING_BYTES`/`MAX_ARRAY_ELEMENTS` globally: DAP/terminal
  inspection has its own useful limits, and changing them would silently alter
  unrelated debugger output.
- Relying only on the value markers and no JSONL flags: a machine consumer must
  be able to tell a complete value from a truncated one without parsing the
  display text.
- Redacting by variable-name heuristics (`password`, `token`, …): wrong on both
  sides. PHP already provides `#[\\SensitiveParameter]`; use the declared marker.
- Redacting the return value of a function with a sensitive parameter: the
  attribute is per-parameter, not per-function. Only that argument is redacted.
- Throwing `ValueError` for an invalid dump destination: the event call is a
  native helper without a cheap Throwable construction path; it warns and does
  not record.
- Using the full rendered string in JSONL and truncating only in HTML: the three
  surfaces would disagree and a raw JSONL consumer would still see the secret or
  the unbounded payload.

## Watch out

- The string prefix is 256 bytes, not 256 Unicode characters. A multi-byte
  character split at the boundary is rendered through `from_utf8_lossy` with a
  replacement character.
- Arrays/hashes enumerate at most 1024 children before the 16-child display cap;
  the omitted count for a larger container is therefore capped at the enumerator
  limit, not the true length.
- Object dynamic properties count toward the 16-child display cap; virtual
  properties still show their marker and do not expand.
- `sensitive` is emitted for every parameter local, but only call capture reads
  it today. DAP locals still show the real value; that is deliberate for a
  stopped debugger view but should be revisited if DAP output is exported.
- `types` mode redacts by flag only (no value); `none` omits args entirely, so it
  has no redaction marker.
- `capture_overflow` is appended after the raw events, not interleaved at the
  dropped position. Consumers should treat it as a session-level trailer.
- The event ring's dropped count is process-global and only reflects events
  refused by the 10,000-event cap, not container/string truncation.
- The e2e fixture uses a 400-byte string, a 20-element array, a four-level nested
  array and a `#[\\SensitiveParameter]` password; it asserts the three markers,
  `redacted`, the JSONL `truncated`/`overflow` fields, the metadata `sensitive`
  flag, and the HTML badges.
