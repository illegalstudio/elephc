---
title: APM page chrome and service CPU/memory measurement
author: Guillaume
date: 2026-11-15
branch: feat/monitor-apm
cost: { minutes: 83, tokens: 412053 }
touches:
  - README.md
  - docs/beyond-php/apm.md
  - docs/compiling/cli-reference.md
  - src/monitor/apm/view/panel.html
  - src/monitor/apm/view/script.js
  - src/monitor/apm/view/style.css
  - crates/elephc-probe/src/apm.rs
  - crates/elephc-probe/src/lib.rs
  - crates/elephc-probe/src/resources.rs
  - src/monitor/apm/dashboard.rs
---

## Decided
- The call-graph page's chrome (theme, header hints, the ☰ menu) lives in a `PAGE` block in `src/call_graph.rs` that runs before the graph script. The graph script returns early when there are no samples (a `monitor --apm --serve` page before its first capture); anything that must work on an empty page goes in `PAGE`, and the graph hooks in with `PAGE.onTheme` / `PAGE.onMenuOpen` / `PAGE.graphReady`.
- Service CPU/memory under `--apm` is measured by the probe endpoint thread, which lives in the master (`crates/elephc-probe/src/resources.rs`), over the process tree: CPU = each live process's own time plus its reaped children's (so it stays cumulative when a worker dies); memory = private memory summed (macOS `ri_phys_footprint`, Linux statm resident − shared). It rides in the snapshot as a `resources` line (older collectors skip unknown lines) and in windows as `res` (CPU delta, memory at end; coalesce sums CPU, keeps max memory).

## Tried and ruled out
- Comparing process counts in a probe unit test: other tests in the crate start and stop children concurrently; check that the specific child is listed instead.
- `libc::mach_timebase_info`: deprecated in libc (points at the mach2 crate); declare the two-field struct and the extern locally.

## Watch out
- `proc_pid_rusage` times (`ri_user_time`, `ri_system_time`, `ri_child_*`) are mach ticks, not nanoseconds: 125/3 ns per tick on Apple silicon. Convert with mach_timebase_info.
- Charting bytes with `niceMax` gives axes like 19 MB / 9.5 MB: chart memory in MiB.
- A navigate that only changes the hash does not reload the page: an "unchanged" page after a rebuild may just be the old document, not a cache.
- A disk at 100 % makes `elephc build --web` fail in tests and looks like a regression. Check `df -h /System/Volumes/Data` first.
