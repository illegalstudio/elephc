# Benchmark Suite

This directory contains small, deterministic benchmark programs used to compare:

- `elephc`-compiled native binaries
- the PHP interpreter
- equivalent `C` implementations

Run the suite with:

```bash
python3 scripts/benchmark_suite.py
```

Useful options:

- `--iterations N` to control measured runs per case
- `--warmup N` to control warmup runs per case
- `--case NAME` to run a single benchmark

The script builds `target/release/elephc` automatically if needed, compiles the
PHP and C fixtures in isolated temp directories, verifies their stdout against
`expected.txt`, and prints a comparison table.
