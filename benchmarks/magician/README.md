# Magician Eval Benchmark Suite

This directory contains focused runtime benchmarks for the optional
`elephc-magician` eval bridge. The cases compare four paths for the same
workload:

- `elephc`-compiled native PHP without `eval`
- `elephc`-compiled PHP that enters magician through `eval`
- PHP interpreter execution without `eval`
- PHP interpreter execution through `eval`

Run the suite with:

```bash
python3 scripts/benchmark_magician.py
```

Useful options:

- `--iterations N` to control measured runs per variant
- `--warmup N` to control warmup runs per variant
- `--case NAME` to run a single benchmark
- `--list` to show available cases
- `--json PATH` to write machine-readable results
- `--markdown PATH` to write the markdown summary table

The runner builds `target/release/elephc` and `libelephc_magician.a` when
needed, compiles each `native.php` and `eval.php` fixture once in an isolated
temporary directory, then measures only repeated binary/PHP execution. Each run
checks stdout against `expected.txt`. When PHP is installed, the PHP native and
eval variants are checked against the same output so the benchmark doubles as a
small parity guard.

Every case has a `metadata.json` file describing eval invocation counts,
fragment source size, literal-vs-dynamic source shape, and whether the parse
cache should hit. The JSON output preserves these fields so timing artifacts can
be interpreted without reopening the fixture.
