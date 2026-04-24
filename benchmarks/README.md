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
- `--json PATH` to write machine-readable results for CI artifacts
- `--markdown PATH` to write the markdown summary table to a file

The script builds `target/release/elephc` automatically if needed, compiles the
PHP and C fixtures in isolated temp directories, verifies their stdout against
`expected.txt`, and prints a comparison table.

## CI automation

Pull requests run the benchmark suite in the `Benchmark Suite` CI job. The job
builds the release compiler once, runs every benchmark case with a small sample
count, writes a markdown table to the GitHub Actions step summary, and uploads
both markdown and JSON result files as artifacts.

The CI job is a correctness and trend-tracking gate. It fails when a benchmark
fixture fails to compile, exits with the wrong output, or the harness itself
breaks. It does not currently enforce hard performance thresholds because
GitHub-hosted runners are noisy enough that absolute timings should be treated
as directional trend data.

If PHP or a C compiler is not available in an environment, that baseline is
reported as `n/a` in the table and marked as skipped in the JSON output.
