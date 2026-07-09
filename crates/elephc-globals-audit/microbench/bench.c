// Addressing-mode microbench for mutable global access.
//
// Measures the relative overhead of three addressing mechanisms for accessing
// per-thread mutable state in a hot loop, across four corpus workloads that
// approximate real elephc runtime access patterns:
//
//   1. str_repeat_alloc: read+write a scratch buffer ptr + length (2 globals)
//   2. array_push_alloc: read+write an array storage ptr + capacity (2 globals)
//   3. json_encode:      read+write a depth counter + flags word (2 globals)
//   4. symfony_boot:     read 16 distinct globals once per iteration (cold-ish)
//
// Mechanisms:
//   (a) baseline   — ADRP+ADD on AArch64, LEA RIP-relative on x86_64
//   (b) native_tls — __thread (macOS TLV / Linux TLS local-exec)
//   (c) ctx_reg    — x28 (AArch64) / r15 (x86_64) context base pointer
//
// Each mechanism runs the same access pattern N iterations in a tight loop.
// Wall-clock time is measured with clock_gettime(CLOCK_MONOTONIC). Results
// are printed as ns/iteration to stdout, one line per (mechanism, workload).
//
// Build (macos-aarch64):
//   cc -O2 -o bench bench.c
//   ./bench
//
// Build (linux-aarch64 cross):
//   clang -target aarch64-unknown-linux-gnu -O2 -o bench_linux_aarch64 bench.c
//
// Build (linux-x86_64 cross):
//   clang -target x86_64-unknown-linux-gnu -O2 -o bench_linux_x86_64 bench.c

#include <stdint.h>
#include <stdio.h>
#include <time.h>

#define ITERS 10000000  // 10M iterations per measurement

// ---- Mechanism (a): baseline globals (ADRP+ADD / RIP-relative LEA) ----
static volatile uint64_t g_buf_ptr_a;
static volatile uint64_t g_buf_len_a;
static volatile uint64_t g_arr_ptr_a;
static volatile uint64_t g_arr_cap_a;
static volatile uint64_t g_depth_a;
static volatile uint64_t g_flags_a;
static volatile uint64_t g_boot[16];

// ---- Mechanism (b): native TLS (__thread / TLV) ----
static __thread volatile uint64_t g_buf_ptr_b;
static __thread volatile uint64_t g_buf_len_b;
static __thread volatile uint64_t g_arr_ptr_b;
static __thread volatile uint64_t g_arr_cap_b;
static __thread volatile uint64_t g_depth_b;
static __thread volatile uint64_t g_flags_b;
static __thread volatile uint64_t g_boot_b[16];

// ---- Mechanism (c): context register (x28 on AArch64, r15 on x86_64) ----
// The context block is a plain struct; the hot loop loads a base pointer into
// the reserved register once and accesses fields as offsets from it.
//
// On AArch64, x28 is callee-saved and not used by the ABI for argument
// passing, making it the conventional choice for a thread-context register.
// On x86_64, r15 is callee-saved and available.
//
// The inline-asm hot loops below use the context register to access the block.
// We simulate the "load base once, access many" pattern that a context-register
// runtime would use: the base pointer is loaded into the reserved register
// before the loop, and every global access is a single LDR/STR [reg, #off].

struct ctx_block {
    uint64_t buf_ptr;
    uint64_t buf_len;
    uint64_t arr_ptr;
    uint64_t arr_cap;
    uint64_t depth;
    uint64_t flags;
    uint64_t boot[16];
};

static struct ctx_block g_ctx;

/// Returns current monotonic time in nanoseconds.
static uint64_t now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

/// Hot loop for mechanism (a) baseline — str_repeat_alloc pattern.
/// Read+write buf_ptr and buf_len, 4 global accesses per iteration.
static uint64_t bench_a_str_repeat(void) {
    uint64_t start = now_ns();
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t p = g_buf_ptr_a;
        uint64_t l = g_buf_len_a;
        g_buf_ptr_a = p + 16;
        g_buf_len_a = l + 1;
    }
    return now_ns() - start;
}

static uint64_t bench_a_array_push(void) {
    uint64_t start = now_ns();
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t p = g_arr_ptr_a;
        uint64_t c = g_arr_cap_a;
        g_arr_ptr_a = p + 8;
        g_arr_cap_a = c + 1;
    }
    return now_ns() - start;
}

static uint64_t bench_a_json_encode(void) {
    uint64_t start = now_ns();
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t d = g_depth_a;
        uint64_t f = g_flags_a;
        g_depth_a = d + 1;
        g_flags_a = f ^ 1;
    }
    return now_ns() - start;
}

static uint64_t bench_a_symfony_boot(void) {
    uint64_t start = now_ns();
    for (uint64_t i = 0; i < ITERS; i++) {
        // Read all 16 boot globals — simulates one request's cold reads.
        uint64_t sum = 0;
        sum += g_boot[0];  sum += g_boot[1];  sum += g_boot[2];  sum += g_boot[3];
        sum += g_boot[4];  sum += g_boot[5];  sum += g_boot[6];  sum += g_boot[7];
        sum += g_boot[8];  sum += g_boot[9];  sum += g_boot[10]; sum += g_boot[11];
        sum += g_boot[12]; sum += g_boot[13]; sum += g_boot[14]; sum += g_boot[15];
        // Sink to prevent DCE.
        g_boot[0] = sum;
    }
    return now_ns() - start;
}

// ---- Mechanism (b) native TLS — same patterns via __thread globals ----

static uint64_t bench_b_str_repeat(void) {
    uint64_t start = now_ns();
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t p = g_buf_ptr_b;
        uint64_t l = g_buf_len_b;
        g_buf_ptr_b = p + 16;
        g_buf_len_b = l + 1;
    }
    return now_ns() - start;
}

static uint64_t bench_b_array_push(void) {
    uint64_t start = now_ns();
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t p = g_arr_ptr_b;
        uint64_t c = g_arr_cap_b;
        g_arr_ptr_b = p + 8;
        g_arr_cap_b = c + 1;
    }
    return now_ns() - start;
}

static uint64_t bench_b_json_encode(void) {
    uint64_t start = now_ns();
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t d = g_depth_b;
        uint64_t f = g_flags_b;
        g_depth_b = d + 1;
        g_flags_b = f ^ 1;
    }
    return now_ns() - start;
}

static uint64_t bench_b_symfony_boot(void) {
    uint64_t start = now_ns();
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t sum = 0;
        sum += g_boot_b[0];  sum += g_boot_b[1];  sum += g_boot_b[2];  sum += g_boot_b[3];
        sum += g_boot_b[4];  sum += g_boot_b[5];  sum += g_boot_b[6];  sum += g_boot_b[7];
        sum += g_boot_b[8];  sum += g_boot_b[9];  sum += g_boot_b[10]; sum += g_boot_b[11];
        sum += g_boot_b[12]; sum += g_boot_b[13]; sum += g_boot_b[14]; sum += g_boot_b[15];
        g_boot_b[0] = sum;
    }
    return now_ns() - start;
}

// ---- Mechanism (c): context register ----
//
// On AArch64 (macOS/Linux), the hot loop loads &g_ctx into x28 before the
// loop and accesses fields as [x28, #offset]. The inline asm below is
// hand-written for AArch64. For x86_64, r15 is used with [r15 + offset].
//
// The access pattern matches the other mechanisms exactly: read+write the
// same two fields per iteration for the first three workloads, and read
// all 16 boot fields for the fourth.

#if defined(__aarch64__)

/// Times the context-register str_repeat hot loop using clock_gettime around
/// the inline asm block.
static uint64_t run_c_str_repeat(void) {
    uint64_t start = now_ns();
    register void *ctx asm("x28") = &g_ctx;
    uint64_t iters = ITERS;
    asm volatile(
        "mov x0, #0\n\t"
        "1:\n\t"
        "cmp x0, %1\n\t"
        "b.cs 2f\n\t"
        "ldr x1, [%0, #0]\n\t"
        "ldr x3, [%0, #8]\n\t"
        "add x1, x1, #16\n\t"
        "add x3, x3, #1\n\t"
        "str x1, [%0, #0]\n\t"
        "str x3, [%0, #8]\n\t"
        "add x0, x0, #1\n\t"
        "b 1b\n\t"
        "2:\n\t"
        :
        : "r"(ctx), "r"(iters)
        : "x0", "x1", "x3", "memory", "cc"
    );
    return now_ns() - start;
}

/// Context-register array_push hot loop (AArch64).
/// Field offsets: arr_ptr=16, arr_cap=24.
static uint64_t run_c_array_push(void) {
    uint64_t start = now_ns();
    register void *ctx asm("x28") = &g_ctx;
    uint64_t iters = ITERS;
    asm volatile(
        "mov x0, #0\n\t"
        "1:\n\t"
        "cmp x0, %1\n\t"
        "b.cs 2f\n\t"
        "ldr x1, [%0, #16]\n\t"    // x1 = ctx->arr_ptr
        "ldr x3, [%0, #24]\n\t"    // x3 = ctx->arr_cap
        "add x1, x1, #8\n\t"
        "add x3, x3, #1\n\t"
        "str x1, [%0, #16]\n\t"
        "str x3, [%0, #24]\n\t"
        "add x0, x0, #1\n\t"
        "b 1b\n\t"
        "2:\n\t"
        :
        : "r"(ctx), "r"(iters)
        : "x0", "x1", "x3", "memory", "cc"
    );
    return now_ns() - start;
}

/// Context-register json_encode hot loop (AArch64).
/// Field offsets: depth=32, flags=40.
static uint64_t run_c_json_encode(void) {
    uint64_t start = now_ns();
    register void *ctx asm("x28") = &g_ctx;
    uint64_t iters = ITERS;
    asm volatile(
        "mov x0, #0\n\t"
        "1:\n\t"
        "cmp x0, %1\n\t"
        "b.cs 2f\n\t"
        "ldr x1, [%0, #32]\n\t"    // x1 = ctx->depth
        "ldr x3, [%0, #40]\n\t"    // x3 = ctx->flags
        "add x1, x1, #1\n\t"
        "eor x3, x3, #1\n\t"
        "str x1, [%0, #32]\n\t"
        "str x3, [%0, #40]\n\t"
        "add x0, x0, #1\n\t"
        "b 1b\n\t"
        "2:\n\t"
        :
        : "r"(ctx), "r"(iters)
        : "x0", "x1", "x3", "memory", "cc"
    );
    return now_ns() - start;
}

/// Context-register symfony_boot hot loop (AArch64).
/// Field offsets: boot[0]=48, boot[15]=48+15*8=168.
static uint64_t run_c_symfony_boot(void) {
    uint64_t start = now_ns();
    register void *ctx asm("x28") = &g_ctx;
    uint64_t iters = ITERS;
    asm volatile(
        "mov x0, #0\n\t"
        "1:\n\t"
        "cmp x0, %1\n\t"
        "b.cs 2f\n\t"
        // Read all 16 boot fields as offsets from x28.
        "ldr x1,  [%0, #48]\n\t"   // boot[0]
        "ldr x3,  [%0, #56]\n\t"   // boot[1]
        "ldr x4,  [%0, #64]\n\t"   // boot[2]
        "ldr x5,  [%0, #72]\n\t"   // boot[3]
        "ldr x6,  [%0, #80]\n\t"   // boot[4]
        "ldr x7,  [%0, #88]\n\t"   // boot[5]
        "ldr x8,  [%0, #96]\n\t"   // boot[6]
        "ldr x9,  [%0, #104]\n\t"  // boot[7]
        "ldr x10, [%0, #112]\n\t"  // boot[8]
        "ldr x11, [%0, #120]\n\t"  // boot[9]
        "ldr x12, [%0, #128]\n\t"  // boot[10]
        "ldr x13, [%0, #136]\n\t"  // boot[11]
        "ldr x14, [%0, #144]\n\t"  // boot[12]
        "ldr x15, [%0, #152]\n\t"  // boot[13]
        "ldr x16, [%0, #160]\n\t"  // boot[14]
        "ldr x17, [%0, #168]\n\t"  // boot[15]
        // Sum to prevent DCE.
        "add x1, x1, x3\n\t"
        "add x1, x1, x4\n\t"
        "add x1, x1, x5\n\t"
        "add x1, x1, x6\n\t"
        "add x1, x1, x7\n\t"
        "add x1, x1, x8\n\t"
        "add x1, x1, x9\n\t"
        "add x1, x1, x10\n\t"
        "add x1, x1, x11\n\t"
        "add x1, x1, x12\n\t"
        "add x1, x1, x13\n\t"
        "add x1, x1, x14\n\t"
        "add x1, x1, x15\n\t"
        "add x1, x1, x16\n\t"
        "add x1, x1, x17\n\t"
        "str x1, [%0, #48]\n\t"    // boot[0] = sum (sink)
        "add x0, x0, #1\n\t"
        "b 1b\n\t"
        "2:\n\t"
        :
        : "r"(ctx), "r"(iters)
        : "x0", "x1", "x3", "x4", "x5", "x6", "x7", "x8",
          "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17",
          "memory", "cc"
    );
    return now_ns() - start;
}

#elif defined(__x86_64__)

/// Context-register str_repeat hot loop (x86_64, r15 base).
/// Field offsets: buf_ptr=0, buf_len=8.
static uint64_t run_c_str_repeat(void) {
    uint64_t start = now_ns();
    register void *ctx asm("r15") = &g_ctx;
    uint64_t iters = ITERS;
    asm volatile(
        "xor %%rax, %%rax\n\t"
        "1:\n\t"
        "cmp %1, %%rax\n\t"
        "jae 2f\n\t"
        "movq 0(%0), %%rcx\n\t"
        "movq 8(%0), %%rdx\n\t"
        "addq $16, %%rcx\n\t"
        "addq $1, %%rdx\n\t"
        "movq %%rcx, 0(%0)\n\t"
        "movq %%rdx, 8(%0)\n\t"
        "addq $1, %%rax\n\t"
        "jmp 1b\n\t"
        "2:\n\t"
        :
        : "r"(ctx), "r"(iters)
        : "rax", "rcx", "rdx", "memory", "cc"
    );
    return now_ns() - start;
}

/// Context-register array_push hot loop (x86_64, r15 base).
static uint64_t run_c_array_push(void) {
    uint64_t start = now_ns();
    register void *ctx asm("r15") = &g_ctx;
    uint64_t iters = ITERS;
    asm volatile(
        "xor %%rax, %%rax\n\t"
        "1:\n\t"
        "cmp %1, %%rax\n\t"
        "jae 2f\n\t"
        "movq 16(%0), %%rcx\n\t"
        "movq 24(%0), %%rdx\n\t"
        "addq $8, %%rcx\n\t"
        "addq $1, %%rdx\n\t"
        "movq %%rcx, 16(%0)\n\t"
        "movq %%rdx, 24(%0)\n\t"
        "addq $1, %%rax\n\t"
        "jmp 1b\n\t"
        "2:\n\t"
        :
        : "r"(ctx), "r"(iters)
        : "rax", "rcx", "rdx", "memory", "cc"
    );
    return now_ns() - start;
}

/// Context-register json_encode hot loop (x86_64, r15 base).
static uint64_t run_c_json_encode(void) {
    uint64_t start = now_ns();
    register void *ctx asm("r15") = &g_ctx;
    uint64_t iters = ITERS;
    asm volatile(
        "xor %%rax, %%rax\n\t"
        "1:\n\t"
        "cmp %1, %%rax\n\t"
        "jae 2f\n\t"
        "movq 32(%0), %%rcx\n\t"
        "movq 40(%0), %%rdx\n\t"
        "addq $1, %%rcx\n\t"
        "xorq $1, %%rdx\n\t"
        "movq %%rcx, 32(%0)\n\t"
        "movq %%rdx, 40(%0)\n\t"
        "addq $1, %%rax\n\t"
        "jmp 1b\n\t"
        "2:\n\t"
        :
        : "r"(ctx), "r"(iters)
        : "rax", "rcx", "rdx", "memory", "cc"
    );
    return now_ns() - start;
}

/// Context-register symfony_boot hot loop (x86_64, r15 base).
static uint64_t run_c_symfony_boot(void) {
    uint64_t start = now_ns();
    register void *ctx asm("r15") = &g_ctx;
    uint64_t iters = ITERS;
    asm volatile(
        "xor %%rax, %%rax\n\t"
        "1:\n\t"
        "cmp %1, %%rax\n\t"
        "jae 2f\n\t"
        "movq 48(%0), %%rcx\n\t"
        "addq 56(%0), %%rcx\n\t"
        "addq 64(%0), %%rcx\n\t"
        "addq 72(%0), %%rcx\n\t"
        "addq 80(%0), %%rcx\n\t"
        "addq 88(%0), %%rcx\n\t"
        "addq 96(%0), %%rcx\n\t"
        "addq 104(%0), %%rcx\n\t"
        "addq 112(%0), %%rcx\n\t"
        "addq 120(%0), %%rcx\n\t"
        "addq 128(%0), %%rcx\n\t"
        "addq 136(%0), %%rcx\n\t"
        "addq 144(%0), %%rcx\n\t"
        "addq 152(%0), %%rcx\n\t"
        "addq 160(%0), %%rcx\n\t"
        "addq 168(%0), %%rcx\n\t"
        "movq %%rcx, 48(%0)\n\t"
        "addq $1, %%rax\n\t"
        "jmp 1b\n\t"
        "2:\n\t"
        :
        : "r"(ctx), "r"(iters)
        : "rax", "rcx", "memory", "cc"
    );
    return now_ns() - start;
}

#else
#error "unsupported architecture for context-register microbench"
#endif

/// Runs one (mechanism, workload) measurement and prints ns/iteration.
static void report(const char *mech, const char *workload, uint64_t ns_total) {
    double ns_per_iter = (double)ns_total / (double)ITERS;
    printf("%s\t%s\t%.3f\n", mech, workload, ns_per_iter);
}

int main(void) {
    // Warm up: run each once to fill caches/tlb.
    (void)bench_a_str_repeat();
    (void)bench_a_array_push();
    (void)bench_a_json_encode();
    (void)bench_a_symfony_boot();
    (void)bench_b_str_repeat();
    (void)bench_b_array_push();
    (void)bench_b_json_encode();
    (void)bench_b_symfony_boot();
#if defined(__aarch64__) || defined(__x86_64__)
    (void)run_c_str_repeat();
    (void)run_c_array_push();
    (void)run_c_json_encode();
    (void)run_c_symfony_boot();
#endif

    // Mechanism (a): baseline ADRP+ADD / RIP-relative.
    report("baseline", "str_repeat_alloc", bench_a_str_repeat());
    report("baseline", "array_push_alloc", bench_a_array_push());
    report("baseline", "json_encode",      bench_a_json_encode());
    report("baseline", "symfony_boot",     bench_a_symfony_boot());

    // Mechanism (b): native TLS (__thread / TLV).
    report("native_tls", "str_repeat_alloc", bench_b_str_repeat());
    report("native_tls", "array_push_alloc", bench_b_array_push());
    report("native_tls", "json_encode",      bench_b_json_encode());
    report("native_tls", "symfony_boot",     bench_b_symfony_boot());

    // Mechanism (c): context register (x28 / r15).
#if defined(__aarch64__) || defined(__x86_64__)
    report("ctx_reg", "str_repeat_alloc", run_c_str_repeat());
    report("ctx_reg", "array_push_alloc", run_c_array_push());
    report("ctx_reg", "json_encode",      run_c_json_encode());
    report("ctx_reg", "symfony_boot",     run_c_symfony_boot());
#endif

    return 0;
}