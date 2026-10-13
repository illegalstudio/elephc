# Unified benchmark tooling

- [x] Consolidate fixtures and tooling under `benchmark/` with one runner.
- [x] Preserve native history and recover available native/Magician artifacts.
- [x] Generate deterministic SVG reports and a dashboard for every suite.
- [x] Extend nightly Repository Stats to refresh and publish benchmark outputs.
- [x] Update README, documentation, fixture tests, and path classification.
- [x] Validate focused tooling, migration, publication, and fixture checks.

Measurement stays in the existing main-only CI benchmark job. Nightly Repository
Stats imports its artifacts, updates daily series, and publishes generated files
with the existing Illegal Bot identity and `[skip ci]`. This reuses the existing
writer and avoids another release build or scheduling workflow.

Keep legacy points after artifact expiry. Replace a day's sample only with a
newer result on the same platform. Preserve measurement metadata and distinguish
changes in environment, fixture, or measurement scope in reports. Native,
Magician, and buffer/array measurements remain separate suites.

Internal Rust microbenchmarks requiring private APIs retain their owning modules
and are exposed through the common entry point for manual investigation. They
do not enter process-execution time series.

PHP is remeasured in every CI run. The first available PHP sample backfills
historical gaps through a separate reference field while preserving null raw
measurements. Reference provenance includes PHP version/environment, fixture,
source commit/run, and timestamp. Known environment/fixture mismatches are not
backfilled. CI installs PHP explicitly and requires all comparison tools. Remove
explanatory missing-baseline banners from the dashboard.

Validation: focused host build, four codegen benchmark tests, all 16 process
cases, 27 real PHP baseline measurements, Python tooling
regressions, path classification, workflow syntax/action pins, deterministic SVG
generation, real artifact download, and browser suite/filter checks passed.
Automation takes effect after integration into main.

The user clarified that PHP must be measured in every CI run, with historical
backfill to avoid empty past values. Persistent PHP caching was removed.
