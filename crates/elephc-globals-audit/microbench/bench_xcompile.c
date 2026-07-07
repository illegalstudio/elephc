// Minimal cross-compile variant for asm inspection only — no libc headers.
// Used to verify the addressing-mode patterns that the compiler emits for
// each target. Timing numbers are CI-deferred for linux targets.

typedef unsigned long uint64_t;
typedef unsigned char uint8_t;

#define ITERS 10000000

static volatile uint64_t g_buf_ptr_a;
static volatile uint64_t g_buf_len_a;
static __thread volatile uint64_t g_buf_ptr_b;
static __thread volatile uint64_t g_buf_len_b;

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

/// Baseline str_repeat hot loop — compiler emits ADRP+ADD (AArch64) or
/// RIP-relative LEA (x86_64), hoisted outside the loop.
uint64_t bench_a_str_repeat(void) {
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t p = g_buf_ptr_a;
        uint64_t l = g_buf_len_a;
        g_buf_ptr_a = p + 16;
        g_buf_len_a = l + 1;
    }
    return g_buf_ptr_a;
}

/// Native TLS str_repeat hot loop — compiler emits TLV access (macOS) or
/// TLS local-exec/general-dynamic (Linux).
uint64_t bench_b_str_repeat(void) {
    for (uint64_t i = 0; i < ITERS; i++) {
        uint64_t p = g_buf_ptr_b;
        uint64_t l = g_buf_len_b;
        g_buf_ptr_b = p + 16;
        g_buf_len_b = l + 1;
    }
    return g_buf_ptr_b;
}

#if defined(__aarch64__)
/// Context-register str_repeat hot loop (AArch64, x28 base).
uint64_t bench_c_str_repeat(void) {
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
    return g_ctx.buf_ptr;
}
#elif defined(__x86_64__)
/// Context-register str_repeat hot loop (x86_64, r15 base).
uint64_t bench_c_str_repeat(void) {
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
    return g_ctx.buf_ptr;
}
#endif