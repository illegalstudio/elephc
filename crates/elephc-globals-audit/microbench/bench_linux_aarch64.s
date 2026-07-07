	.file	"bench_xcompile.c"
	.text
	.globl	bench_a_str_repeat              // -- Begin function bench_a_str_repeat
	.p2align	2
	.type	bench_a_str_repeat,@function
bench_a_str_repeat:                     // @bench_a_str_repeat
	.cfi_startproc
// %bb.0:
	mov	w8, #38528                      // =0x9680
	adrp	x9, g_buf_ptr_a
	adrp	x10, g_buf_len_a
	movk	w8, #152, lsl #16
.LBB0_1:                                // =>This Inner Loop Header: Depth=1
	ldr	x11, [x9, :lo12:g_buf_ptr_a]
	ldr	x12, [x10, :lo12:g_buf_len_a]
	subs	x8, x8, #1
	add	x11, x11, #16
	add	x12, x12, #1
	str	x11, [x9, :lo12:g_buf_ptr_a]
	str	x12, [x10, :lo12:g_buf_len_a]
	b.ne	.LBB0_1
// %bb.2:
	ldr	x0, [x9, :lo12:g_buf_ptr_a]
	ret
.Lfunc_end0:
	.size	bench_a_str_repeat, .Lfunc_end0-bench_a_str_repeat
	.cfi_endproc
                                        // -- End function
	.globl	bench_b_str_repeat              // -- Begin function bench_b_str_repeat
	.p2align	2
	.type	bench_b_str_repeat,@function
bench_b_str_repeat:                     // @bench_b_str_repeat
	.cfi_startproc
// %bb.0:
	mrs	x8, TPIDR_EL0
	add	x9, x8, :tprel_hi12:g_buf_ptr_b
	add	x10, x8, :tprel_hi12:g_buf_len_b
	add	x8, x9, :tprel_lo12_nc:g_buf_ptr_b
	add	x9, x10, :tprel_lo12_nc:g_buf_len_b
	mov	w10, #38528                     // =0x9680
	movk	w10, #152, lsl #16
.LBB1_1:                                // =>This Inner Loop Header: Depth=1
	ldr	x11, [x8]
	ldr	x12, [x9]
	subs	x10, x10, #1
	add	x11, x11, #16
	add	x12, x12, #1
	str	x11, [x8]
	str	x12, [x9]
	b.ne	.LBB1_1
// %bb.2:
	ldr	x0, [x8]
	ret
.Lfunc_end1:
	.size	bench_b_str_repeat, .Lfunc_end1-bench_b_str_repeat
	.cfi_endproc
                                        // -- End function
	.globl	bench_c_str_repeat              // -- Begin function bench_c_str_repeat
	.p2align	2
	.type	bench_c_str_repeat,@function
bench_c_str_repeat:                     // @bench_c_str_repeat
	.cfi_startproc
// %bb.0:
	str	x28, [sp, #-16]!                // 8-byte Folded Spill
	.cfi_def_cfa_offset 16
	.cfi_offset w28, -16
	mov	w8, #38528                      // =0x9680
	adrp	x28, g_ctx
	add	x28, x28, :lo12:g_ctx
	movk	w8, #152, lsl #16
	//APP
	mov	x0, #0                          // =0x0
.Ltmp0:
	cmp	x0, x8
	b.hs	.Ltmp1
	ldr	x1, [x28]
	ldr	x3, [x28, #8]
	add	x1, x1, #16
	add	x3, x3, #1
	str	x1, [x28]
	str	x3, [x28, #8]
	add	x0, x0, #1
	b	.Ltmp0
.Ltmp1:

	//NO_APP
	ldr	x0, [x28]
	ldr	x28, [sp], #16                  // 8-byte Folded Reload
	.cfi_def_cfa_offset 0
	.cfi_restore w28
	ret
.Lfunc_end2:
	.size	bench_c_str_repeat, .Lfunc_end2-bench_c_str_repeat
	.cfi_endproc
                                        // -- End function
	.type	g_buf_ptr_a,@object             // @g_buf_ptr_a
	.local	g_buf_ptr_a
	.comm	g_buf_ptr_a,8,8
	.type	g_buf_len_a,@object             // @g_buf_len_a
	.local	g_buf_len_a
	.comm	g_buf_len_a,8,8
	.type	g_buf_ptr_b,@object             // @g_buf_ptr_b
	.section	.tbss,"awT",@nobits
	.p2align	3, 0x0
g_buf_ptr_b:
	.xword	0                               // 0x0
	.size	g_buf_ptr_b, 8

	.type	g_buf_len_b,@object             // @g_buf_len_b
	.p2align	3, 0x0
g_buf_len_b:
	.xword	0                               // 0x0
	.size	g_buf_len_b, 8

	.type	g_ctx,@object                   // @g_ctx
	.local	g_ctx
	.comm	g_ctx,176,8
	.ident	"Apple clang version 21.0.0 (clang-2100.0.123.102)"
	.section	".note.GNU-stack","",@progbits
	.addrsig
	.addrsig_sym g_buf_ptr_a
	.addrsig_sym g_buf_len_a
	.addrsig_sym g_ctx
