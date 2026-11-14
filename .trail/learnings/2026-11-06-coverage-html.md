---
title: Coverage HTML is a source-rendering view over the line/branch report model
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 90 }
touches:
  - src/test_command/coverage.rs
  - src/test_command/args.rs
  - tests/codegen/testing_command.rs
  - docs/beyond-php/testing.md
  - docs/compiling/cli-reference.md
  - docs/compiling/debug-metadata.md
---

## Decided

- `CoverageFormat::Html` is a third renderer over the existing `CoverageReport`;
  it does not re-collect counters or re-parse metadata.
- The report is self-contained: inline CSS, no JavaScript, no external assets.
  A CI artifact can be opened directly from disk.
- Each source becomes a file section with a table of every source line:
  - `covered` (green tint) when the line has hits;
  - `uncovered` (red tint) when the line is executable and has zero hits;
  - `neutral` for non-executable lines, still shown for context;
  - branch lines carry `T <hits>` / `E <hits>` edge badges (green/red) and a
    per-file line/branch summary.
- Source text is read from the path recorded in metadata. A missing/unreadable
  source still gets its coverage rows and a warning, rather than failing the
  report.
- HTML escaping covers `& < > " '`, so PHP operators (`>`, `<`, `->`) render
  correctly and cannot inject markup.
- `--coverage-format html` selects stdout; `--coverage-out report.html` infers
  HTML from `.html`/`.htm` and still prints the text summary to stdout.

## Tried and ruled out

- Embedding JSON and rendering with JavaScript: a self-contained static report is
  more portable and works in CI artifact viewers without script execution.
- Reusing the Cobertura XML renderer as the HTML body: XML is machine-readable,
  not a line-oriented source view; the HTML renderer needs source text and row
  classes.
- Failing on a missing source: a coverage artifact from another checkout may not
  have the sources; showing the counters with a warning is more useful.
- Reading sources at collection time: the existing report model is pure data, and
  rendering is the only place that needs file I/O. Keeping the model pure makes
  unit tests and future formats simpler.

## Watch out

- The source file is read at render time; if it changed after compilation, the
  line text may not match the compiled sites. Coverage does not verify source
  hashes (unlike DAP breakpoints).
- A very large source renders every line. There is no pagination or filtering
  yet; the report is intended for CI artifacts, not a huge monorepo.
- Branch rows show edge counts, not a branch-condition percentage; the per-file
  summary carries the aggregate.
- The HTML report is not linked from the monitor HTML or the JUnit report.
- `--coverage-format html` to stdout mixes the test runner's output with the HTML
  document. Use `--coverage-out` for a clean file.
