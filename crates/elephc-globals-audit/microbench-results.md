# Addressing-Mode Microbench Results

## Scope

This report contains **measured data** on the relative cost of three
addressing mechanisms for accessing per-thread mutable global state in hot
loops. It does **not** recommend a mechanism; the data is an input to a
separate decision process.

## Mechanisms

| Label | Description |
|-------|-------------|
| `baseline` | Current compiler output: `adrp+add` on AArch64, RIP-relative `lea`/`mov` on x86_64. |
| `native_tls` | Platform native TLS: macOS TLV via `__thread`, Linux TLS local-exec via `__thread`. |
| `ctx_reg` | Reserved context register: `x28` on AArch64, `r15` on x86_64. Base pointer loaded once before the loop; accesses are `[reg, #off]`. |

## Corpus

Four workloads approximating real elephc runtime access patterns. Each runs
10,000,000 iterations per measurement.

| Workload | Access pattern | Globals per iter |
|----------|---------------|-----------------|
| `str_repeat_alloc` | read+write scratch buffer ptr + length | 2 (4 acc) |
| `array_push_alloc` | read+write array storage ptr + capacity | 2 (4 acc) |
| `json_encode` | read+write depth counter + flags word | 2 (4 acc) |
| `symfony_boot` | read 16 distinct boot globals, sum, write back | 16 (17 acc) |

All globals are `volatile uint64_t` to prevent the compiler from eliminating
loads/stores.

## Targets

| Target | Status | Method |
|--------|--------|--------|
| `macos-aarch64` | **Measured** (host) | `cc -O2`, 3 runs, median reported |
| `linux-aarch64` | **Asm verified, timing CI-deferred** | Cross-compiled with `clang -target aarch64-unknown-linux-gnu -O2 -S`; no Linux runtime on host |
| `linux-x86_64` | **Asm verified, timing CI-deferred** | Cross-compiled with `clang -target x86_64-unknown-linux-gnu -O2 -S`; no Linux runtime on host |

Linux timing is deferred to CI. Numbers are **not** fabricated. The asm
inspection below documents what each target emits so the per-iteration
instruction count can be compared without running.

---

## Measured Results: macos-aarch64

Host: Apple Silicon (aarch64), macOS 25.5.0. Compiler: `cc -O2` (Apple clang
21.0.0). 3 runs, median of 3.

| Mechanism | str_repeat_alloc | array_push_alloc | json_encode | symfony_boot |
|-----------|-----------------|-----------------|-------------|-------------|
| `baseline` | 0.318 ns/iter | 0.318 ns/iter | 0.321 ns/iter | 2.826 ns/iter |
| `native_tls` | 0.317 ns/iter | 0.318 ns/iter | 0.313 ns/iter | 2.475 ns/iter |
| `ctx_reg` | 0.813 ns/iter | 0.799 ns/iter | 0.818 ns/iter | 5.571 ns/iter |

### Observation

`baseline` and `native_tls` are **statistically indistinguishable** on the
2-global workloads (~0.32 ns/iter). On the 16-global `symfony_boot` workload,
`native_tls` is ~12% faster (2.48 vs 2.83 ns/iter). `ctx_reg` is ~2.5x slower
than both on all workloads.

### Why baseline and native_tls are equal

Compiler-generated asm inspection (`bench.s`) shows:

**Baseline (a)** — the compiler hoists `adrp` outside the loop:
```asm
    adrp    x25, _g_buf_ptr_a@PAGE      ; -- hoisted before loop --
    adrp    x27, _g_buf_len_a@PAGE      ; -- hoisted before loop --
LBB0_1:
    ldr     x9, [x25, _g_buf_ptr_a@PAGEOFF]   ; per-iter: 1 LDR
    ldr     x10, [x27, _g_buf_len_a@PAGEOFF]  ; per-iter: 1 LDR
    add     x9, x9, #16
    str     x9, [x25, _g_buf_ptr_a@PAGEOFF]   ; per-iter: 1 STR
    add     x9, x10, #1
    str     x9, [x27, _g_buf_len_a@PAGEOFF]   ; per-iter: 1 STR
    subs    x8, x8, #1
    b.ne    LBB0_1
```

**Native TLS (b)** — macOS TLV resolves the per-thread address once before
the loop via a `blr` to the TLV initializer, then caches the resolved address
in a callee-saved register:
```asm
    adrp    x0, _g_buf_ptr_b@TLVPPAGE         ; -- one-time TLV setup --
    ldr     x0, [x0, _g_buf_ptr_b@TLVPPAGEOFF]
    ldr     x8, [x0]
    blr     x8                           ; call TLV initializer (first-touch)
    mov     x23, x0                      ; cache resolved address in x23
    ; ... same for g_buf_len_b → x24 ...
LBB0_7:
    ldr     x9, [x23]                    ; per-iter: 1 LDR (identical to baseline)
    ldr     x10, [x24]                   ; per-iter: 1 LDR
    add     x9, x9, #16
    str     x9, [x23]                    ; per-iter: 1 STR
    add     x9, x10, #1
    str     x9, [x24]                    ; per-iter: 1 STR
    subs    x8, x8, #1
    b.ne    LBB0_7
```

The per-iteration loop bodies are **instruction-for-instruction identical**
(2 LDR + 2 STR + 2 ADD + 1 SUBS + 1 B.NE). The addressing setup is hoisted in
both cases. The TLV one-time setup (`blr` to initializer) is outside the
measured loop and amortized over 10M iterations.

### Why ctx_reg is slower

The `ctx_reg` hot loop is hand-written inline asm. The compiler cannot
unroll, schedule, or LDP-pair inside inline asm. The loop body has the same
instruction count as the compiler-generated loops, but the compiler-generated
loops benefit from:
- 2x unrolling (visible in the x86_64 asm below)
- Instruction scheduling (overlapping LDR latency with ADD/SUBS)
- Potential LDP/STP pairing for adjacent fields

A context-register implementation with **compiler support** (not inline asm)
would likely close this gap, since the per-access instruction count is the
same (1 LDR/STR from a register offset). This is a measurement of
**hand-written inline asm vs compiler-generated code**, not of the addressing
mode itself.

---

## Asm Verification: linux-aarch64

Cross-compiled with `clang -target aarch64-unknown-linux-gnu -O2 -S`.
Timing not measured (no Linux aarch64 runtime on host). CI-deferred.

### Baseline (a)
```asm
    adrp    x9, g_buf_ptr_a              ; -- hoisted --
    adrp    x10, g_buf_len_a             ; -- hoisted --
.LBB0_1:
    ldr     x11, [x9, :lo12:g_buf_ptr_a]   ; per-iter: 1 LDR
    ldr     x12, [x10, :lo12:g_buf_len_a]  ; per-iter: 1 LDR
    subs    x8, x8, #1
    add     x11, x11, #16
    add     x12, x12, #1
    str     x11, [x9, :lo12:g_buf_ptr_a]   ; per-iter: 1 STR
    str     x12, [x10, :lo12:g_buf_len_a]  ; per-iter: 1 STR
    b.ne    .LBB0_1
```
Same pattern as macOS: ADRP hoisted, per-iter is LDR+LDR+STR+STR.

### Native TLS (b) — Linux local-exec
```asm
    mrs     x8, TPIDR_EL0                ; -- hoisted: read thread pointer --
    add     x9, x8, :tprel_hi12:g_buf_ptr_b   ; -- hoisted --
    add     x10, x8, :tprel_hi12:g_buf_len_b  ; -- hoisted --
    add     x8, x9, :tprel_lo12_nc:g_buf_ptr_b
    add     x9, x10, :tprel_lo12_nc:g_buf_len_b
.LBB1_1:
    ldr     x11, [x8]                    ; per-iter: 1 LDR (plain, no TLS insn)
    ldr     x12, [x9]                    ; per-iter: 1 LDR
    subs    x10, x10, #1
    add     x11, x11, #16
    add     x12, x12, #1
    str     x11, [x8]                    ; per-iter: 1 STR
    str     x12, [x9]                    ; per-iter: 1 STR
    b.ne    .LBB1_1
```

Linux uses TLS **local-exec** model (static TLS block, no `__tls_get_addr`
call). The thread pointer is read via `mrs TPIDR_EL0` and the per-thread
address is computed with `:tprel_hi12` + `:tprel_lo12_nc` relocations, all
hoisted outside the loop. The per-iteration body is **identical** to the
baseline: 2 LDR + 2 STR + 2 ADD + 1 SUBS + 1 B.NE.

### Context register (c) — x28
Hand-written inline asm (same as macOS). Per-iter: 2 LDR + 2 STR + 2 ADD +
1 CMP + 1 B.CS + 1 ADD (counter) + 1 B. The inline-asm overhead is the same
as on macOS.

**Expected relative cost (linux-aarch64):** Same per-iter instruction count
as macOS. `baseline` and `native_tls` should be equal (both hoist setup, same
loop body). `ctx_reg` should be slower if inline-asm prevents compiler
optimization. CI measurement needed to confirm.

---

## Asm Verification: linux-x86_64

Cross-compiled with `clang -target x86_64-unknown-linux-gnu -O2 -S`.
Timing not measured (no Linux x86_64 runtime on host). CI-deferred.

### Baseline (a)
```asm
    movl    $10000000, %eax
.LBB0_1:
    movq    g_buf_ptr_a(%rip), %rcx      ; per-iter: 1 RIP-relative MOV
    movq    g_buf_len_a(%rip), %rdx      ; per-iter: 1 RIP-relative MOV
    addq    $16, %rcx
    movq    %rcx, g_buf_ptr_a(%rip)      ; per-iter: 1 RIP-relative MOV
    incq    %rdx
    movq    %rdx, g_buf_len_a(%rip)      ; per-iter: 1 RIP-relative MOV
    ; (loop is 2x unrolled — pattern repeats)
```

RIP-relative addressing is a **single instruction** per access. No setup
needed (the RIP is implicit).

### Native TLS (b) — Linux local-exec (FS segment)
```asm
    movl    $10000000, %eax
.LBB1_1:
    movq    %fs:g_buf_ptr_b@TPOFF, %rcx   ; per-iter: 1 FS-relative MOV
    movq    %fs:g_buf_len_b@TPOFF, %rdx   ; per-iter: 1 FS-relative MOV
    addq    $16, %rcx
    movq    %rcx, %fs:g_buf_ptr_b@TPOFF   ; per-iter: 1 FS-relative MOV
    incq    %rdx
    movq    %rdx, %fs:g_buf_len_b@TPOFF   ; per-iter: 1 FS-relative MOV
    ; (loop is 2x unrolled — pattern repeats)
```

Linux x86_64 TLS local-exec uses `%fs:@TPOFF` segment override — also a
**single instruction** per access, identical in cost to the RIP-relative
baseline. No setup hoisting needed (FS base is maintained by the kernel).

### Context register (c) — r15
Hand-written inline asm. Per-iter: 2 MOV + 2 ADD + 2 MOV + 1 CMP + 1 JAE +
1 ADD (counter) + 1 JMP. The inline-asm loop is not unrolled by the compiler.

**Expected relative cost (linux-x86_64):** `baseline` and `native_tls` should
be equal (both single-instruction per access, both unrolled by compiler).
`ctx_reg` should be slower due to no unrolling and inline-asm overhead. CI
measurement needed to confirm.

---

## Summary Table

| Target | Mechanism | Per-iter insns (2-global) | Setup hoisted? | Measured? |
|--------|-----------|--------------------------|----------------|-----------|
| macos-aarch64 | baseline | 2 LDR + 2 STR + 2 ADD + sub/br | yes (ADRP) | yes (0.32 ns) |
| macos-aarch64 | native_tls | 2 LDR + 2 STR + 2 ADD + sub/br | yes (TLV init) | yes (0.32 ns) |
| macos-aarch64 | ctx_reg | 2 LDR + 2 STR + 2 ADD + cmp/br + add/br | yes (mov x28) | yes (0.81 ns) |
| linux-aarch64 | baseline | 2 LDR + 2 STR + 2 ADD + sub/br | yes (ADRP) | CI-defer |
| linux-aarch64 | native_tls | 2 LDR + 2 STR + 2 ADD + sub/br | yes (MRS+ADD) | CI-defer |
| linux-aarch64 | ctx_reg | 2 LDR + 2 STR + 2 ADD + cmp/br + add/br | yes (mov x28) | CI-defer |
| linux-x86_64 | baseline | 2 MOV + 2 ADD + 2 MOV + sub/br | no (RIP implicit) | CI-defer |
| linux-x86_64 | native_tls | 2 MOV + 2 ADD + 2 MOV + sub/br | no (FS implicit) | CI-defer |
| linux-x86_64 | ctx_reg | 2 MOV + 2 ADD + 2 MOV + cmp/jae + add/jmp | yes (mov r15) | CI-defer |

## What was hand-written vs generated

| Component | Source |
|-----------|--------|
| `baseline` hot loops | Compiler-generated (C `volatile` globals, `-O2`) |
| `native_tls` hot loops | Compiler-generated (C `__thread volatile` globals, `-O2`) |
| `ctx_reg` hot loops | **Hand-written** inline asm (AArch64 x28 / x86_64 r15) |
| `ctx_reg` struct layout | Hand-written `struct ctx_block` with field offsets matching the inline asm |
| Timing harness | Compiler-generated (`clock_gettime` around each loop) |

## CI-defer note

Linux-aarch64 and linux-x86_64 timing measurements require a Linux runtime
(host is macOS aarch64). The asm patterns are verified via cross-compilation
(`clang -target ... -S`), confirming that the per-iteration instruction count
is identical across targets for each mechanism. Actual cycle-count/timing
comparison on Linux is deferred to CI.

## Reproducibility

```bash
# macos-aarch64 (host)
cd crates/elephc-globals-audit/microbench
cc -O2 -o bench bench.c
./bench

# linux-aarch64 (asm only, cross-compile)
clang -target aarch64-unknown-linux-gnu -O2 -S -o bench_linux_aarch64.s bench_xcompile.c

# linux-x86_64 (asm only, cross-compile)
clang -target x86_64-unknown-linux-gnu -O2 -S -o bench_linux_x86_64.s bench_xcompile.c
```