- [x] Measure the unchanged Ivory consumer on exact post-PR2087 source through elvm.
- [x] Remove remaining proven dispatch/inlining barriers with regression coverage.
- [x] Account for hot-loop EIR and assembly, preserving checked access semantics.
- [x] Validate exceptions, aliases, close, integer semantics, allocations and all targets.
- [x] Repeat paired C/raw/managed measurements and record acceptance evidence.

# Checked managed-buffer acceptance

Code Journal task: `c548e591`. Journal plan: `2f93a3fa`.

The consumer is Ivory's `feat-managed-primitives` checkout, using its unchanged
`src/Memory/Buffer.lfc` and `bench/buffer/main.lfc`. Keep its release pin unchanged.
Use `make bench BENCHES=buffer ELEPHC="elvm exec c548e591" ARGS="10000 1000"`
through the machine-wide build queue. Compare repeated median, p99, maximum,
checksums and paired C controls. Measure allocation counters separately.

Baseline compiler source is `9bf91ee5ffe4fb8a821447e585105810a03ec714`, a descendant
of merged PR2087 (`a0aad2e46def8bdb711824d5562dd9616bd84267`). Earlier measurements
predate that fix and do not establish the remaining overhead.

Preserve the owned generic API, OutOfRangeException for invalid indices,
LogicException after close, idempotent close, shared object aliases, and PHP
integer overflow behavior. Object-argument generic variance must retain its
runtime-selected implementation. Every changed backend rule must agree across
all five supported targets.

Initial inspection found conservative generic dispatch and a method-inlining
allowlist that omits buffer writes. Verify their effect in fresh consumer EIR
before changing them. Use existing analyses for subsequent redundancy removal;
do not infer purity or move faulting reads from the benchmark alone.


## Implementation and validation

Scalar generic receiver dispatch and method inlining share one variance proof.
Buffer writes are eligible for frame-independent method splicing. Receiver leases
backed by unchanged local owners are removed before inlining, with reassignment
and aliases retaining their protection. Checked prefixes cache stable state,
length and storage only on taken loop entries. Terminal cold paths preserve
initial SSA operands, source spans, lexical scopes and owner records. Repeated
SSA guards fold only after a dominating guard edge; backward entry edges do not
establish a fact.

Scalar object-property reuse excludes packed field values, which a packed write
can change while preserving the record address and buffer descriptor. Fix commit
`2279b63e5f8e90d631689c300c9bebfa930aa7b1` passes a warning-free build and the
following focused validation.

Six focused library tests cover scalar versus object generic instantiations,
local receiver ownership, guard facts and checked-loop EIR/assembly on every
supported target. Seven native regressions
cover checksums, bounds, aliases, close, zero-trip loops, PHP overflow, allocation
counters, receiver rebinding and packed field writes. Behavioral cases run with IR optimization on
and off under strict heap verification; the optimized allocation window is
allocation-free. The late-index fixture also covers a negative initial index
through the cloned cold path. Packed read freshness is covered by
`test_checked_managed_buffer_packed_reads_observe_writes` and
`checked_managed_buffer_packed_reads_survive_writes_on_every_target`.

The original acceptance at `c91474f99ceaea3c5a6a46244bba5f1439c0a786` also ran
four existing library regressions covering receiver aliases, object-loop
descriptor hoisting and generic variance, plus four existing codegen tests for
direct dispatch on all five targets, variance, destructor timing and throwing-loop scratch.
Local execution is Linux x86_64; the other targets have emitter coverage.

## Consumer measurements

All consumer timing, EIR/assembly and allocation measurements in this section
belong to `c91474f99ceaea3c5a6a46244bba5f1439c0a786`, before the packed-field
exclusion in `2279b63e5f8e90d631689c300c9bebfa930aa7b1`. Post-fix evidence is the
focused validation above.

The final consumer sources match the baseline byte for byte:

- `src/Memory/Buffer.lfc`: `71fd3c98c8bde90f899101910c070d62218e0b573e4867076c02a8901a7a2297`
- `bench/buffer/main.lfc`: `50ca952f02dbd8eab8734ef5b1497223561dcda18440df97daf73b95b9ea4ebc`
- `bench/buffer/main.c`: `6f7b431ab3834a749c355352bd2864aad730e1dd70e67209bc4500d7473e88ac`

Five baseline runs gave raw medians 0.052-0.062 ms and managed medians
0.519-0.632 ms. The baseline already inlines assertIndex/assertOpen inside the
accessors; its remaining overhead is the outer generic get/set boundary and
receiver lifetime work.

Seven CLI trials on the implementation, 10000 items and 1000 frames:

| Trial | Raw median / p99 (ms) | Managed median / p99 (ms) | C median / p99 (ms) | Managed/raw |
|---|---|---|---|---|
| 1 | 0.053 / 0.066 | 0.040 / 0.046 | 0.004 / 0.004 | 0.755x |
| 2 | 0.053 / 0.074 | 0.041 / 0.088 | 0.004 / 0.006 | 0.774x |
| 3 | 0.053 / 0.066 | 0.040 / 0.057 | 0.004 / 0.005 | 0.755x |
| 4 | 0.053 / 0.072 | 0.041 / 0.075 | 0.004 / 0.005 | 0.774x |
| 5 | 0.053 / 0.068 | 0.041 / 0.045 | 0.004 / 0.005 | 0.774x |
| 6 | 0.051 / 0.066 | 0.040 / 0.060 | 0.004 / 0.004 | 0.784x |
| 7 | 0.051 / 0.064 | 0.040 / 0.044 | 0.004 / 0.004 | 0.784x |

All checksums are 327761016. The median paired ratio is 0.774x, with range
0.755-0.784x and absolute median deviation 0.011x. The medians of reported
per-run p99 values are raw 0.066 ms, managed 0.057 ms and C 0.005 ms. These
workload measurements establish comparability without promising exact zero
abstraction cost in arbitrary programs.

Actual CLI EIR retains only the three FrameStats calls outside the inner update
loop. The update uses resolved native buffer accesses, native I64 arithmetic
and masking. Checked arithmetic retains overflow behavior and buffer accesses
retain validity/bounds checks. Property reads and descriptor resolution are
outside the hot loop. Runtime count/frame arguments and matching C checksums
rule out folding the workload to a constant.

A separate 20-frame counter probe reports zero managed update allocations,
frees and live-byte change; raw, managed and C checksum 322350648. Whole-program
heap debug retains two scalar cells on the baseline and candidate. Allocation
traffic outside the timed update also increases on the candidate. Their cause
is not localized and is tracked separately in task `df3dd618` and Code Journal entry `8f086851`;
these figures must not be described as whole-program zero allocation or a clean
consumer exit. The focused owner safety regressions have clean heap summaries.
Ivory's release pin remains 0.27.6.
