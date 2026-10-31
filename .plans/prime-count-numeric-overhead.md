# Prime-count numeric overhead

- [x] Profile the exact prime-count method from commit `1b5668b68d` on the current base.
- [x] Admit direct numeric comparisons only after confirming their frame-storage gap.
- [x] Verify the regression on base, allocation scaling, PHP semantics and every target emitter.
- [x] Compare matched baseline, candidate and PHP timings and record remaining costs.
- [x] Profile the first improvement and identify the remaining hot numeric operations.
- [x] Implement a further proven optimization without weakening overflow or warning semantics.
- [x] Validate the second improvement and measure against the frozen first improvement and PHP.
- [ ] Obtain a clean Grok review of the complete change.
- [ ] Publish the pull request and pass its exact-head CI and required review gates.
- [ ] Complete the native merge queue and verify the result on remote main.

The source task is Code Journal `43c8e42b`. The exported class example has not
reached this checkout's base, so the profiling fixture extracts its exact method
body into a standalone final class. This isolates native execution of the method
from the extension adapter. It does not establish the current extension boundary's
cost. Keep the runtime limit unknown and preserve the original inclusive bounds,
step sizes and trial-division algorithm.

The candidate is a lifetime-proof extension in `src/codegen/stack_numeric.rs`.
An owned checked arithmetic result can use its existing numeric frame cell when
an ordered comparison consumes its payload in the same block, provided every
comparison operand has a proven integer/float origin. Existing numeric comparison
lowering already preserves exact integer ordering, overflow-promoted floats and
unordered NaN results. Nonnumeric partners and observations in another block stay
outside this proof. No counter range, overflow check or call boundary changes.

Local evidence belongs under `target/task-43c8e42b/`: extracted PHP source,
baseline compiler and executable, emitted EIR/assembly, sampled native stacks,
raw timings and regression logs. Benchmark timers surround only the method call;
CLI startup is excluded. Pin uninstrumented timing runs to CPU 11, use a 1 MiB
native heap and run real PHP with `-n` so OPcache and JIT are disabled. Report
profiling percentages as samples, and keep profiler timing separate from runtime
comparisons.

## Baseline evidence

Base `9d84dbc20260f92bb041cd4f465c34b37c1b1c84`, Linux x86_64, PHP 8.5.12 NTS.
At limit 2,000,000 both implementations return 148,933. One warmup followed by
five rotating samples per mode gives medians of 3.644921 s native and 1.103016 s
PHP (MAD 2.42% and 3.70%). These standalone numbers reproduce the body slowdown;
they are separate from the task's earlier extension measurement.

A randomized GDB sampler collected 500 leaf samples from a symbol-preserving
build: 153 in the method body, 135 in numeric/boxing helpers, 86 in integer casts,
70 in heap allocation and 56 in retirement. Profiling time is excluded from the
benchmark. The initial stripped executable produced no samples and is not
profiling evidence. A first Cargo test build was killed by SIGKILL; its cause was
not established, and the retry uses two build jobs.

Instrumented runs return 168 primes with 3,028 allocations/frees at limit 1,000,
and 1,229 primes with 62,196 allocations/frees at limit 10,000. The EIR and assembly
show a runtime boxed multiply for the inner product while the numeric updates
already use frame cells. Direct comparison is absent from the producer's allowed
consumer list, despite existing numeric relational lowering.

## Result and validation

The focused proof extension removes the inner product's call to
`__rt_mixed_numeric_mul`. Its result stays in a frame cell and still carries its
integer or overflow-promoted float tag. No counter range proof changed.

The final comparison uses the same fixture, CPU, heap and timer, with one warmup
and five rotating samples for each implementation:

| Implementation | Median seconds | MAD |
| --- | ---: | ---: |
| Frozen baseline | 3.562068 | 1.61% |
| Candidate | 1.176880 | 1.59% |
| PHP | 1.183129 | 0.58% |

The candidate is 3.03 times faster than the baseline (66.96% less time). Its
difference from PHP is smaller than the observed timing variation; report this
as parity, not a demonstrated speed advantage over PHP. Every timed run returns
148,933. This measures the standalone method body, not the pending exported-class
adapter integration.

At limit 1,000, allocations/frees fall from 3,028 to 510. At limit 10,000 they
fall from 62,196 to 5,010. Allocation growth has not been eliminated from the
whole algorithm: the divisor's repeated scalar seed still boxes into its Mixed
local. Future counter or seed specialization needs its own proof and a fresh
profile. Unknown inclusive bounds are still outside the existing integer proof.

Validation passed a warning-free `cargo build -j 2 -p elephc`, three new proof
tests, three existing lifetime controls and three new end-to-end tests. The
positive proof test fails on the unchanged implementation; both negative controls
pass there. End-to-end checks cover allocation scaling, exact integer ordering
above double precision, overflow floats, NaN, both IR modes, real PHP and clean
heap retirement. Assembly emission covers macOS ARM64, iOS device, iOS Simulator,
Linux ARM64 and Linux x86_64. The typed prime example matches PHP with a clean
heap. Diff hygiene and assembly-comment checks pass.

An initial integration compile failed because the nested test module needed an
explicit path. That registration was corrected before the successful run. The
passing compiler build and six unit results were retained; only integration and
benchmark steps were retried. Task-owned test temporary directories were removed;
raw evidence and reproducibility scripts remain in the named target directory.

## Second optimization round

The user requested further work after reaching PHP parity. A fresh randomized
profile of the frozen first improvement collected 563 samples: 381 in the method
body, 128 in integer-cast helpers, 27 in unboxing, 21 in decref and six elsewhere.
Integer conversion and unboxing account for 27.53% of these samples.

`src/codegen/lower_inst/numeric_cast.rs` now reads a proven numeric frame cell's
integer payload directly after checking its tag. Eligibility uses the existing
selected frame values or proven numeric local slots, with no additional graph
walks. Float tags use the unchanged boxed conversion on a cold path. Unknown
origins, reference storage, implicit casts and string-offset diagnostics retain
their original paths. Neither counter representation nor range proofs change.

Seven rotating timed samples per mode, following one warmup, use the same source,
CPU 11, 1 MiB heap, runtime input and body-only timer:

| Implementation | Median seconds | MAD |
| --- | ---: | ---: |
| Frozen first improvement | 1.133746 | 1.38% |
| Second improvement | 0.677734 | 0.91% |
| PHP | 1.106987 | 3.66% |

The second improvement is 1.67 times faster than the first improvement and 1.63
times faster than PHP on this kernel. Every timed run returns 148,933. Instrumented
allocation counts remain 5,010 allocations/frees at limit 10,000; this change
removes conversion calls rather than heap seeds. Raw evidence is under
`target/task-43c8e42b/round2/`.

The three new integration tests cover all five target emitters, exact integers
above double precision, int-to-float continuation, NaN/INF conversion, unknown
tags, reference-backed storage and string-offset warnings under a user handler.
Three previous comparison tests and the two exact existing overflow/ownership
tests also pass. Both IR modes and real PHP run with heap validation. The
stage1 assembly lacks the new cast path, establishing the structural regression.

An initial fixture incorrectly retained C-export annotations on an unknown Mixed
parameter while running it as an executable; those annotations are now removed
for runtime checks. A warning-parity control then confirmed the existing
`0cc561c9` discrepancy: explicit boxed float casts omit warnings that PHP 8.5 emits.
The frozen first improvement and this improvement produce identical output for
the reproducer. Follow-up task `a5150e15` tracks that existing diagnostic problem;
this optimization keeps the original float conversion and diagnostic setup.
Numeric boundary tests suppress those inherited warnings in both implementations;
the separate string-offset test verifies a supported warning path is retained.

The final binding control additionally covers a conditional `unset` followed by
a suppressed cast, confirming that a killed binding does not inherit a numeric
pointer assumption. The typed prime example also matches PHP with a clean heap
under the second compiler. All eight selected tests and final hygiene checks
pass; no task-owned `checks-*` temporary directories remain. Publication and
review evidence are tracked in Code Journal plan `d6045879`.
