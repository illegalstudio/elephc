---
title: LCOV and Cobertura are renderers over the same per-line hit model
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 80 }
touches:
  - src/test_command/coverage.rs
  - src/test_command/args.rs
  - src/test_command.rs
  - tests/codegen/testing_command.rs
  - docs/beyond-php/testing.md
  - docs/compiling/cli-reference.md
  - docs/compiling/debug-metadata.md
---

## Decided

- `test --coverage` collects one `CoverageReport` first: user sources, each with
  `(line, hits)` pairs. Hits on a line are the sum of its site counters, so split
  functions and loop iterations aggregate naturally.
- Text, LCOV and Cobertura are pure renderers over that model. Text keeps the
  existing summary; LCOV writes `SF:`/`DA:`/`LF:`/`LH:`/`end_of_record`; Cobertura
  writes one `<class>` per source with `<line number hits/>` and overall
  `line-rate`/`lines-covered`.
- `--coverage-format text|lcov|cobertura` selects stdout. `--coverage-out FILE`
  infers the format from the extension (`.lcov`/`.info` → LCOV, `.xml` →
  Cobertura, else text), but an explicit `--coverage-format` wins. Both flags
  imply `--coverage`.
- Writing a file still prints the text summary on stdout and a one-line stderr
  note. A CI log stays readable without having to cat the XML.
- Cobertura's `<source>` is the component-wise common parent directory of every
  reported source; `filename` is relative to it. A single source rooted at
  `tests/` therefore emits `filename="CounterTest.php"`, while a multi-directory
  project emits `tests/CounterTest.php`.
- XML metacharacters in paths and class names are escaped. A source file with `&`
  becomes `a&amp;b.php`, so the report stays well-formed.

## Tried and ruled out

- Writing LCOV/Cobertura directly from `render(metadata, counts)`: it duplicated
  the source/line walk and made text the only testable form. One report model is
  smaller and keeps the totals identical across formats.
- Branch records in LCOV/Cobertura: the compiler emits one guard per source line,
  not branch edges. Emitting `BRF`/`BRH` or `<branch>` rows would invent data.
  Those stay tied to the branch-coverage work.
- HTML coverage output: a separate presentation layer. `--coverage-out` remains
  machine-readable formats for now.
- Suppressing the text summary when a file is written: the log would show only a
  confirmation line, and a human reading CI would lose the covered/total figure.

## Watch out

- `--filter` selects test methods at runtime, but the runner still compiles every
  discovered test class, so filtered-out classes can appear as uncovered files.
  That is compiled-code coverage, not a bug; the docs now say so.
- Cobertura `line-rate` is rounded to four decimals in attributes while the text
  percentage is one decimal; do not compare the two strings.
- LCOV has no function/branch sections yet. `LF`/`LH` are line totals.
- The report model excludes the synthesized `.elephc-test.lfc` entry, which always
  runs and would add a fixed 100% row.
- The e2e test asserts both `--coverage-format lcov` on stdout and
  `--coverage-out coverage.xml` (file + text summary). The project fixture has
  `tests/` and `tests/Unit/`, so the Cobertura filename is `tests/CounterTest.php`.
