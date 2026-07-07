	.file	"bench_xcompile.c"
	.text
	.globl	bench_a_str_repeat              # -- Begin function bench_a_str_repeat
	.p2align	4
	.type	bench_a_str_repeat,@function
bench_a_str_repeat:                     # @bench_a_str_repeat
	.cfi_startproc
# %bb.0:
	movl	$10000000, %eax                 # imm = 0x989680
	.p2align	4
.LBB0_1:                                # =>This Inner Loop Header: Depth=1
	movq	g_buf_ptr_a(%rip), %rcx
	movq	g_buf_len_a(%rip), %rdx
	addq	$16, %rcx
	movq	%rcx, g_buf_ptr_a(%rip)
	incq	%rdx
	movq	%rdx, g_buf_len_a(%rip)
	movq	g_buf_ptr_a(%rip), %rcx
	movq	g_buf_len_a(%rip), %rdx
	addq	$16, %rcx
	movq	%rcx, g_buf_ptr_a(%rip)
	incq	%rdx
	movq	%rdx, g_buf_len_a(%rip)
	addq	$-2, %rax
	jne	.LBB0_1
# %bb.2:
	movq	g_buf_ptr_a(%rip), %rax
	retq
.Lfunc_end0:
	.size	bench_a_str_repeat, .Lfunc_end0-bench_a_str_repeat
	.cfi_endproc
                                        # -- End function
	.globl	bench_b_str_repeat              # -- Begin function bench_b_str_repeat
	.p2align	4
	.type	bench_b_str_repeat,@function
bench_b_str_repeat:                     # @bench_b_str_repeat
	.cfi_startproc
# %bb.0:
	movl	$10000000, %eax                 # imm = 0x989680
	.p2align	4
.LBB1_1:                                # =>This Inner Loop Header: Depth=1
	movq	%fs:g_buf_ptr_b@TPOFF, %rcx
	movq	%fs:g_buf_len_b@TPOFF, %rdx
	addq	$16, %rcx
	movq	%rcx, %fs:g_buf_ptr_b@TPOFF
	incq	%rdx
	movq	%rdx, %fs:g_buf_len_b@TPOFF
	movq	%fs:g_buf_ptr_b@TPOFF, %rcx
	movq	%fs:g_buf_len_b@TPOFF, %rdx
	addq	$16, %rcx
	movq	%rcx, %fs:g_buf_ptr_b@TPOFF
	incq	%rdx
	movq	%rdx, %fs:g_buf_len_b@TPOFF
	addq	$-2, %rax
	jne	.LBB1_1
# %bb.2:
	movq	%fs:g_buf_ptr_b@TPOFF, %rax
	retq
.Lfunc_end1:
	.size	bench_b_str_repeat, .Lfunc_end1-bench_b_str_repeat
	.cfi_endproc
                                        # -- End function
	.globl	bench_c_str_repeat              # -- Begin function bench_c_str_repeat
	.p2align	4
	.type	bench_c_str_repeat,@function
bench_c_str_repeat:                     # @bench_c_str_repeat
	.cfi_startproc
# %bb.0:
	pushq	%r15
	.cfi_def_cfa_offset 16
	.cfi_offset %r15, -16
	leaq	g_ctx(%rip), %r15
	movl	$10000000, %esi                 # imm = 0x989680
	#APP
	xorq	%rax, %rax
.Ltmp0:
	cmpq	%rsi, %rax
	jae	.Ltmp1
	movq	(%r15), %rcx
	movq	8(%r15), %rdx
	addq	$16, %rcx
	addq	$1, %rdx
	movq	%rcx, (%r15)
	movq	%rdx, 8(%r15)
	addq	$1, %rax
	jmp	.Ltmp0
.Ltmp1:

	#NO_APP
	movq	g_ctx(%rip), %rax
	popq	%r15
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end2:
	.size	bench_c_str_repeat, .Lfunc_end2-bench_c_str_repeat
	.cfi_endproc
                                        # -- End function
	.type	g_buf_ptr_a,@object             # @g_buf_ptr_a
	.local	g_buf_ptr_a
	.comm	g_buf_ptr_a,8,8
	.type	g_buf_len_a,@object             # @g_buf_len_a
	.local	g_buf_len_a
	.comm	g_buf_len_a,8,8
	.type	g_buf_ptr_b,@object             # @g_buf_ptr_b
	.section	.tbss,"awT",@nobits
	.p2align	3, 0x0
g_buf_ptr_b:
	.quad	0                               # 0x0
	.size	g_buf_ptr_b, 8

	.type	g_buf_len_b,@object             # @g_buf_len_b
	.p2align	3, 0x0
g_buf_len_b:
	.quad	0                               # 0x0
	.size	g_buf_len_b, 8

	.type	g_ctx,@object                   # @g_ctx
	.local	g_ctx
	.comm	g_ctx,176,8
	.ident	"Apple clang version 21.0.0 (clang-2100.0.123.102)"
	.section	".note.GNU-stack","",@progbits
	.addrsig
	.addrsig_sym g_buf_ptr_a
	.addrsig_sym g_buf_len_a
	.addrsig_sym g_ctx
