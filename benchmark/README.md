# Benchmarks

All benchmark fixtures, runners, saved CI histories, and reports live here.
The process suites use one runner and the same versioned JSON format:

| Suite | Workload | Variants |
|---|---|---|
| `native` | Integer sum, array reads, string concatenation | elephc, PHP, equivalent C (`-O2`) |
| `magician` | Literal/dynamic eval, arithmetic, arrays, builtins, callables, includes | elephc native/eval, PHP native/eval |
| `hot-path` | Equivalent position updates through different storage | elephc buffer, indexed array, associative array |

## Running

```bash
python3 benchmark/run.py --list
python3 benchmark/run.py --suite all
python3 benchmark/run.py --suite native --case sum_loop --iterations 7 --warmup 2
python3 benchmark/run.py --case magician/arithmetic_loop --json results.json --markdown results.md
```

The runner asks Cargo to refresh the release compiler before measuring, even if
the binary already exists. `--no-build` reuses a compiler built immediately
beforehand, as CI does. `--compiler PATH` selects an explicitly managed compiler
without building. Neither mode rewrites the selected compiler.

`--require-baselines` fails before building if PHP or a C compiler needed by the
selected cases is missing. CI enables this flag and explicitly installs PHP 8.5
without coverage drivers. Local runs can still skip unavailable comparison tools.

PHP, elephc, and C are measured again in every CI run. Each case gets one warmup
and three measured executions in CI; PHP results are not cached between runs.

Each fixture is copied to a temporary directory, each native variant is compiled
once, and only repeated process execution is timed. Every warmup and measured
execution must produce `expected.txt` exactly. Temporary sources and binaries
are removed on success and failure. Use `--timeout SECONDS` to bound individual
compilation/execution commands (default: 120 seconds).

Missing PHP or C tools appear as skipped baselines, never zero times. PHP CLI
OPcache and JIT are explicitly disabled for new measurements. The compiler,
Rust, C, PHP, runner, architecture, iterations, warmup, and fixture hash are
recorded with available raw samples. A result's commit identifies the measured
source, not the later statistics publication commit.

The hot-path variants include initialization and final checksum computation in
their process time. They replace the former combined in-process stopwatch and
have no historical series until the first CI run using this harness.

## Reports and dashboard

Native medians and recent trends:

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/native-dark.svg">
  <img src="assets/native-light.svg" alt="Native benchmark medians and trends">
</picture>

<details>
<summary>Magician native/eval measurements</summary>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/magician-dark.svg">
  <img src="assets/magician-light.svg" alt="Magician eval medians and native/eval ratios">
</picture>

</details>

<details>
<summary>Buffer and array storage measurements</summary>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/hot-path-dark.svg">
  <img src="assets/hot-path-light.svg" alt="Buffer and array storage benchmark medians">
</picture>

</details>

```bash
python3 benchmark/dashboard.py
python3 benchmark/dashboard.py --port 9000 --no-open
python3 benchmark/report.py
```

The dashboard runs on localhost and works offline, without CDN dependencies.
Choose a suite, time window, or absolute/relative metric; hover a point for its
commit and measured environment. SVG reports are deterministic and work directly
in GitHub README files, with light/dark variants. The root README embeds the
native report and links here for the other suites.

## CI and saved history

The existing `Benchmark Suite` job runs on pushes to `main`, using the release
compiler on `macos-15`, three measured executions and one warmup. It uploads one
`benchmark-results` artifact containing `results.json` and `results.md` for all
three suites. PRs run focused tooling tests and the normal fixture correctness
tests; they do not append performance samples to the product history.

Historical native and Magician artifacts have no PHP timings because the old
macOS job did not install PHP. Those values cannot be recovered by fetching the
old artifacts again. New CI runs calculate the PHP baseline once with
OPcache/JIT disabled in each run and record its installed version.

The first available PHP measurement also supplies a reference for earlier points
that lack PHP timings. Reports use this reference to fill historical gaps. The
raw `median_ms` remains null; a separate `reference` records the value, PHP
version/environment, fixture hash, source commit/run, and measurement timestamp.
It is an estimate for historical comparisons, not a PHP execution from that
date. The dashboard draws reference segments with a dashed line and shows their
origin on hover. Known platform or fixture mismatches are never backfilled. Unknown legacy
metadata can use the first available reference. Existing PHP measurements and
references are preserved; each new CI run adds its own freshly measured PHP data.

The existing nightly `Repository Stats` workflow fetches available benchmark
artifacts, merges them into `data/{native,magician,hot-path}.json`, regenerates
`assets/*.svg`, and publishes them together with repository statistics. It uses
the existing Illegal Bot installation identity for a `[skip ci]` commit. Artifact
reads use an Actions-read workflow token. Publication uses an isolated checkout
of current `main`; concurrent updates trigger another merge and normal push,
without changing the source checkout or force-pushing.

Each suite keeps the latest sample per **UTC artifact-upload day and platform**.
A newer run can replace the existing sample for that day. Older dates remain
saved after artifact expiry. Selection happens after reading each artifact's
suite and platform. `data/imports.json` checkpoints sample metadata for each
artifact, so unchanged or superseded artifacts and legacy archives lacking
newer suites are not downloaded again every night. Fetch failures or malformed
saved results fail the refresh before replacing any history.

Manual refresh:

```bash
python3 benchmark/fetch.py
python3 benchmark/report.py
```

`fetch.py` imports both the original native/Magician artifact formats and the
new shared format. It filters artifacts by name, branch, expiry, and repository
origin. Use `--repo owner/repo` explicitly when needed. A different branch or
`--all-runs` sampling requires a separate `--out-dir`, preserving the main daily
history's meaning. No destructive rebuild option is provided.

Legacy native points were migrated with their timing values intact, including
expired artifacts. Available Magician history was recovered too. Legacy samples
lack runner/toolchain/fixture metadata and some raw samples; these fields remain
unknown rather than inferred. New environment or fixture versions start a new
plot segment. Hosted runner results describe trends, not universal performance
claims or hard regression thresholds. Eval/native ratios include process startup,
and eligible literal eval fragments can be AOT-lowered; fragment metadata in the
Magician history explains each case's intended path.

## Internal measurements

For an isolated numeric accumulator, compile `benchmark/diagnostics/numeric_local.php`
and pass the iteration count at runtime. Run the same source and arguments with
`php -n`, with CLI OPcache and JIT disabled. Output is `checksum|body_ns`; argument
parsing and process startup precede the body clock. Measure several iteration
counts with alternating compiler/PHP executions and report dispersion. Collect
`--gc-stats` in separate diagnostic binaries so timings use ordinary code generation.

For controlled revision comparisons of the mixed workload, select each compiler
explicitly and supply its source revision. This diagnostic runner is separate
from the historical process-only suites:

```bash
python3 benchmark/compare.py --compiler main=/path/to/main/elephc \
  --revision main=FULL_COMMIT_SHA --bridge-dir main=/path/to/main/archives \
  --compiler release=/path/to/release/elephc --revision release=v0.27.3 \
  --bridge-dir release=/path/to/release/archives --require-idle --json mixed-results.json
```

Each compiler builds the same PHP fixture once with the selected PHP semantic
profile, optimizer setting and heap size. Supply matching release-built Magician
archives using `--bridge-dir` when they are not adjacent to that compiler. Opaque
eval can also link callbacks from other bridges, so supply their matching
archives in the same directory. Every supplied archive is fingerprinted and its
library-directory override replaces inherited overrides from other builds.
Compilation happens before timing. PHP is required and runs with `-n`, OPcache
and JIT disabled. Two warmups precede 21 measured rounds with rotating variant
order. On Linux, `--cpu N` pins every measured child to an available CPU.

The output separates arithmetic, closure, append, dynamic eval, JSON decode with
function teardown, total workload body and whole-process wall time. JSON input
is prepared before timing, and the eval phase includes its first activation.
Use `--size`, `--rows`, `--columns` and `--eval-count` to investigate scaling;
changes to these parameters describe different workloads. Compiler/binary/input
hashes, supplied revisions, the exact PHP version and all raw samples are saved.
Every compiler label requires a matching `--revision`; missing revisions are
rejected before compilation because a compiler can come from another checkout.
SHA-256 fingerprints use streaming reads without Python 3.11-only hashing APIs.

Every warmup and measured run must produce the expected checksum and consistent
phase clocks. A phase whose median absolute deviation exceeds 5% of its median
marks the comparison noisy, preserves the results and exits nonzero. Run on a
quiet machine with no concurrent builds. `--require-idle` additionally checks
`ps` before and after each execution and rejects visible compiler/linker processes;
measurement start/end timestamps are saved. Neither this guard nor the noise
gate can detect all systematic contention or make different architectures comparable. These samples are not
appended to the historical dashboard. Startup-sensitive whole-process ratios and
body ratios must be reported separately, and isolated speedups must not be multiplied.

To separate JSON decode from explicit value release, compile
`benchmark/diagnostics/json_lifecycle.php` and pass `INPUT_JSON REPEATS DRAIN_CACHES`.
The last argument is `0` or `1`. Run the same source with `php -n` and the same
arguments. Its output is `outer_count_sum|decode_ns:release_ns:drain_ns`; input
loading is outside all three clocks. With cache draining disabled, the last
clock only measures stopwatch overhead. `unset` includes any collector work
triggered by the normal lowering. Repeat counts and explicit cache draining
change allocation reuse, so report them alongside the heap size and keep them
identical across compiler revisions. Cache-drain timings are separate from the
mixed fixture, which does not call `gc_mem_caches()`.

Rust stopwatches that require private compiler/interpreter APIs retain their
owning test modules, with commands exposed through the same entry point:

```bash
python3 benchmark/run.py --internal script-cache
python3 benchmark/run.py --internal null-representation
```

For an mbstring bridge footprint probe, compile
`benchmark/diagnostics/mbstring.php` with matching release-built archives and
compare its output with real PHP with mbstring enabled. It exercises encoding
enumeration, lookup and multiple codec consumers in one executable. Compare
executable bytes and ELF/Mach-O section sizes separately from execution time;
`--keep-symbols` and the default stripped mode are different measurements.
Use a separate `CARGO_TARGET_DIR` for each revision when building bridge archives.
An older worktree sharing a target directory can otherwise reuse another
revision's artifacts based on their timestamps.

These manual measurements have different scopes (including parse/deserialize or
compile/link time). They do not join the process-execution histories or run in
the trend CI job.

## Tooling checks

```bash
python3 -B -m unittest discover -s benchmark/tests -v
python3 -B benchmark/report.py
git diff --check
```

Tests cover expired-history migration, later samples on the same day, reruns,
multiple platforms/suites, skipped baselines, invalid data, deterministic SVGs,
offline dashboard embedding, and publication racing against a newer main commit.
