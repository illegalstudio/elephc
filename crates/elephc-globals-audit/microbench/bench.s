	.build_version macos, 26, 0	sdk_version 26, 4
	.section	__TEXT,__text,regular,pure_instructions
	.globl	_main                           ; -- Begin function main
	.p2align	2
_main:                                  ; @main
	.cfi_startproc
; %bb.0:
	sub	sp, sp, #160
	stp	d9, d8, [sp, #48]               ; 16-byte Folded Spill
	stp	x28, x27, [sp, #64]             ; 16-byte Folded Spill
	stp	x26, x25, [sp, #80]             ; 16-byte Folded Spill
	stp	x24, x23, [sp, #96]             ; 16-byte Folded Spill
	stp	x22, x21, [sp, #112]            ; 16-byte Folded Spill
	stp	x20, x19, [sp, #128]            ; 16-byte Folded Spill
	stp	x29, x30, [sp, #144]            ; 16-byte Folded Spill
	add	x29, sp, #144
	.cfi_def_cfa w29, 16
	.cfi_offset w30, -8
	.cfi_offset w29, -16
	.cfi_offset w19, -24
	.cfi_offset w20, -32
	.cfi_offset w21, -40
	.cfi_offset w22, -48
	.cfi_offset w23, -56
	.cfi_offset w24, -64
	.cfi_offset w25, -72
	.cfi_offset w26, -80
	.cfi_offset w27, -88
	.cfi_offset w28, -96
	.cfi_offset b8, -104
	.cfi_offset b9, -112
	mov	w26, #38528                     ; =0x9680
	movk	w26, #152, lsl #16
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	adrp	x25, _g_buf_ptr_a@PAGE
	adrp	x27, _g_buf_len_a@PAGE
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_1:                                 ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x25, _g_buf_ptr_a@PAGEOFF]
	ldr	x10, [x27, _g_buf_len_a@PAGEOFF]
	add	x9, x9, #16
	str	x9, [x25, _g_buf_ptr_a@PAGEOFF]
	add	x9, x10, #1
	str	x9, [x27, _g_buf_len_a@PAGEOFF]
	subs	x8, x8, #1
	b.ne	LBB0_1
; %bb.2:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	adrp	x11, _g_arr_ptr_a@PAGE
	adrp	x12, _g_arr_cap_a@PAGE
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_3:                                 ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x11, _g_arr_ptr_a@PAGEOFF]
	ldr	x10, [x12, _g_arr_cap_a@PAGEOFF]
	add	x9, x9, #8
	str	x9, [x11, _g_arr_ptr_a@PAGEOFF]
	add	x9, x10, #1
	str	x9, [x12, _g_arr_cap_a@PAGEOFF]
	subs	x8, x8, #1
	b.ne	LBB0_3
; %bb.4:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	adrp	x11, _g_depth_a@PAGE
	adrp	x12, _g_flags_a@PAGE
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_5:                                 ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x11, _g_depth_a@PAGEOFF]
	ldr	x10, [x12, _g_flags_a@PAGEOFF]
	add	x9, x9, #1
	str	x9, [x11, _g_depth_a@PAGEOFF]
	eor	x9, x10, #0x1
	str	x9, [x12, _g_flags_a@PAGEOFF]
	subs	x8, x8, #1
	b.ne	LBB0_5
; %bb.6:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	bl	_bench_a_symfony_boot
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
Lloh0:
	adrp	x0, _g_buf_ptr_b@TLVPPAGE
Lloh1:
	ldr	x0, [x0, _g_buf_ptr_b@TLVPPAGEOFF]
	ldr	x8, [x0]
	blr	x8
	mov	x23, x0
Lloh2:
	adrp	x0, _g_buf_len_b@TLVPPAGE
Lloh3:
	ldr	x0, [x0, _g_buf_len_b@TLVPPAGEOFF]
	ldr	x8, [x0]
	blr	x8
	mov	x24, x0
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_7:                                 ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x23]
	ldr	x10, [x24]
	add	x9, x9, #16
	str	x9, [x23]
	add	x9, x10, #1
	str	x9, [x24]
	subs	x8, x8, #1
	b.ne	LBB0_7
; %bb.8:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
Lloh4:
	adrp	x0, _g_arr_ptr_b@TLVPPAGE
Lloh5:
	ldr	x0, [x0, _g_arr_ptr_b@TLVPPAGEOFF]
	ldr	x8, [x0]
	blr	x8
	mov	x21, x0
Lloh6:
	adrp	x0, _g_arr_cap_b@TLVPPAGE
Lloh7:
	ldr	x0, [x0, _g_arr_cap_b@TLVPPAGEOFF]
	ldr	x8, [x0]
	blr	x8
	mov	x22, x0
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_9:                                 ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x21]
	ldr	x10, [x22]
	add	x9, x9, #8
	str	x9, [x21]
	add	x9, x10, #1
	str	x9, [x22]
	subs	x8, x8, #1
	b.ne	LBB0_9
; %bb.10:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
Lloh8:
	adrp	x0, _g_depth_b@TLVPPAGE
Lloh9:
	ldr	x0, [x0, _g_depth_b@TLVPPAGEOFF]
	ldr	x8, [x0]
	blr	x8
	mov	x19, x0
Lloh10:
	adrp	x0, _g_flags_b@TLVPPAGE
Lloh11:
	ldr	x0, [x0, _g_flags_b@TLVPPAGEOFF]
	ldr	x8, [x0]
	blr	x8
	mov	x20, x0
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_11:                                ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x19]
	ldr	x10, [x20]
	add	x9, x9, #1
	str	x9, [x19]
	eor	x9, x10, #0x1
	str	x9, [x20]
	subs	x8, x8, #1
	b.ne	LBB0_11
; %bb.12:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	bl	_bench_b_symfony_boot
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
Lloh12:
	adrp	x28, _g_ctx@PAGE
Lloh13:
	add	x28, x28, _g_ctx@PAGEOFF
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
	; InlineAsm Start
	mov	x0, #0                          ; =0x0
Ltmp0:
	cmp	x0, x8
	b.hs	Ltmp1
	ldr	x1, [x28]
	ldr	x3, [x28, #8]
	add	x1, x1, #16
	add	x3, x3, #1
	str	x1, [x28]
	str	x3, [x28, #8]
	add	x0, x0, #1
	b	Ltmp0
Ltmp1:

	; InlineAsm End
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
	; InlineAsm Start
	mov	x0, #0                          ; =0x0
Ltmp2:
	cmp	x0, x8
	b.hs	Ltmp3
	ldr	x1, [x28, #16]
	ldr	x3, [x28, #24]
	add	x1, x1, #8
	add	x3, x3, #1
	str	x1, [x28, #16]
	str	x3, [x28, #24]
	add	x0, x0, #1
	b	Ltmp2
Ltmp3:

	; InlineAsm End
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
	; InlineAsm Start
	mov	x0, #0                          ; =0x0
Ltmp4:
	cmp	x0, x8
	b.hs	Ltmp5
	ldr	x1, [x28, #32]
	ldr	x3, [x28, #40]
	add	x1, x1, #1
	eor	x3, x3, #0x1
	str	x1, [x28, #32]
	str	x3, [x28, #40]
	add	x0, x0, #1
	b	Ltmp4
Ltmp5:

	; InlineAsm End
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	mov	w2, #38528                      ; =0x9680
	movk	w2, #152, lsl #16
	; InlineAsm Start
	mov	x0, #0                          ; =0x0
Ltmp6:
	cmp	x0, x2
	b.hs	Ltmp7
	ldr	x1, [x28, #48]
	ldr	x3, [x28, #56]
	ldr	x4, [x28, #64]
	ldr	x5, [x28, #72]
	ldr	x6, [x28, #80]
	ldr	x7, [x28, #88]
	ldr	x8, [x28, #96]
	ldr	x9, [x28, #104]
	ldr	x10, [x28, #112]
	ldr	x11, [x28, #120]
	ldr	x12, [x28, #128]
	ldr	x13, [x28, #136]
	ldr	x14, [x28, #144]
	ldr	x15, [x28, #152]
	ldr	x16, [x28, #160]
	ldr	x17, [x28, #168]
	add	x1, x1, x3
	add	x1, x1, x4
	add	x1, x1, x5
	add	x1, x1, x6
	add	x1, x1, x7
	add	x1, x1, x8
	add	x1, x1, x9
	add	x1, x1, x10
	add	x1, x1, x11
	add	x1, x1, x12
	add	x1, x1, x13
	add	x1, x1, x14
	add	x1, x1, x15
	add	x1, x1, x16
	add	x1, x1, x17
	str	x1, [x28, #48]
	add	x0, x0, #1
	b	Ltmp6
Ltmp7:

	; InlineAsm End
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x28, x8, [sp, #32]
	str	x8, [sp, #24]                   ; 8-byte Folded Spill
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_13:                                ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x25, _g_buf_ptr_a@PAGEOFF]
	ldr	x10, [x27, _g_buf_len_a@PAGEOFF]
	add	x9, x9, #16
	str	x9, [x25, _g_buf_ptr_a@PAGEOFF]
	add	x9, x10, #1
	str	x9, [x27, _g_buf_len_a@PAGEOFF]
	subs	x8, x8, #1
	b.ne	LBB0_13
; %bb.14:
	mov	w25, #51712                     ; =0xca00
	movk	w25, #15258, lsl #16
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x28
	ldr	x10, [sp, #24]                  ; 8-byte Folded Reload
	sub	x9, x9, x10
	madd	x8, x8, x25, x9
	ucvtf	d0, x8
	mov	x8, #20684562497536             ; =0x12d000000000
	movk	x8, #16739, lsl #48
	fmov	d1, x8
	fdiv	d0, d0, d1
Lloh14:
	adrp	x8, l_.str.1@PAGE
Lloh15:
	add	x8, x8, l_.str.1@PAGEOFF
Lloh16:
	adrp	x28, l_.str@PAGE
Lloh17:
	add	x28, x28, l_.str@PAGEOFF
	stp	x28, x8, [sp]
	str	d0, [sp, #16]
Lloh18:
	adrp	x0, l_.str.7@PAGE
Lloh19:
	add	x0, x0, l_.str.7@PAGEOFF
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x25, x27, [sp, #32]
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
	adrp	x11, _g_arr_ptr_a@PAGE
	adrp	x12, _g_arr_cap_a@PAGE
LBB0_15:                                ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x11, _g_arr_ptr_a@PAGEOFF]
	ldr	x10, [x12, _g_arr_cap_a@PAGEOFF]
	add	x9, x9, #8
	str	x9, [x11, _g_arr_ptr_a@PAGEOFF]
	add	x9, x10, #1
	str	x9, [x12, _g_arr_cap_a@PAGEOFF]
	subs	x8, x8, #1
	b.ne	LBB0_15
; %bb.16:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x25
	sub	x9, x9, x27
	mov	w10, #51712                     ; =0xca00
	movk	w10, #15258, lsl #16
	madd	x8, x8, x10, x9
	ucvtf	d0, x8
	mov	x8, #20684562497536             ; =0x12d000000000
	movk	x8, #16739, lsl #48
	fmov	d1, x8
	fdiv	d0, d0, d1
Lloh20:
	adrp	x8, l_.str.2@PAGE
Lloh21:
	add	x8, x8, l_.str.2@PAGEOFF
	stp	x28, x8, [sp]
	str	d0, [sp, #16]
Lloh22:
	adrp	x0, l_.str.7@PAGE
Lloh23:
	add	x0, x0, l_.str.7@PAGEOFF
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x25, x27, [sp, #32]
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
	adrp	x11, _g_depth_a@PAGE
	adrp	x12, _g_flags_a@PAGE
LBB0_17:                                ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x11, _g_depth_a@PAGEOFF]
	ldr	x10, [x12, _g_flags_a@PAGEOFF]
	add	x9, x9, #1
	str	x9, [x11, _g_depth_a@PAGEOFF]
	eor	x9, x10, #0x1
	str	x9, [x12, _g_flags_a@PAGEOFF]
	subs	x8, x8, #1
	b.ne	LBB0_17
; %bb.18:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x25
	sub	x9, x9, x27
	mov	w10, #51712                     ; =0xca00
	movk	w10, #15258, lsl #16
	madd	x8, x8, x10, x9
	ucvtf	d0, x8
	mov	x8, #20684562497536             ; =0x12d000000000
	movk	x8, #16739, lsl #48
	fmov	d8, x8
	fdiv	d0, d0, d8
Lloh24:
	adrp	x27, l_.str.3@PAGE
Lloh25:
	add	x27, x27, l_.str.3@PAGEOFF
	stp	x28, x27, [sp]
	str	d0, [sp, #16]
Lloh26:
	adrp	x25, l_.str.7@PAGE
Lloh27:
	add	x25, x25, l_.str.7@PAGEOFF
	mov	x0, x25
	bl	_printf
	bl	_bench_a_symfony_boot
	ucvtf	d0, x0
	fdiv	d0, d0, d8
	str	d0, [sp, #16]
Lloh28:
	adrp	x8, l_.str.4@PAGE
Lloh29:
	add	x8, x8, l_.str.4@PAGEOFF
	stp	x28, x8, [sp]
	mov	x0, x25
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x25, x28, [sp, #32]
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_19:                                ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x23]
	ldr	x10, [x24]
	add	x9, x9, #16
	str	x9, [x23]
	add	x9, x10, #1
	str	x9, [x24]
	subs	x8, x8, #1
	b.ne	LBB0_19
; %bb.20:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x25
	sub	x9, x9, x28
	mov	w10, #51712                     ; =0xca00
	movk	w10, #15258, lsl #16
	madd	x8, x8, x10, x9
	ucvtf	d0, x8
	mov	x8, #20684562497536             ; =0x12d000000000
	movk	x8, #16739, lsl #48
	fmov	d1, x8
	fdiv	d0, d0, d1
Lloh30:
	adrp	x23, l_.str.5@PAGE
Lloh31:
	add	x23, x23, l_.str.5@PAGEOFF
Lloh32:
	adrp	x8, l_.str.1@PAGE
Lloh33:
	add	x8, x8, l_.str.1@PAGEOFF
	stp	x23, x8, [sp]
	str	d0, [sp, #16]
Lloh34:
	adrp	x0, l_.str.7@PAGE
Lloh35:
	add	x0, x0, l_.str.7@PAGEOFF
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x24, x25, [sp, #32]
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB0_21:                                ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x21]
	ldr	x10, [x22]
	add	x9, x9, #8
	str	x9, [x21]
	add	x9, x10, #1
	str	x9, [x22]
	subs	x8, x8, #1
	b.ne	LBB0_21
; %bb.22:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x24
	sub	x9, x9, x25
	mov	w10, #51712                     ; =0xca00
	movk	w10, #15258, lsl #16
	madd	x8, x8, x10, x9
	ucvtf	d0, x8
	mov	x8, #20684562497536             ; =0x12d000000000
	movk	x8, #16739, lsl #48
	fmov	d1, x8
	fdiv	d0, d0, d1
Lloh36:
	adrp	x25, l_.str.2@PAGE
Lloh37:
	add	x25, x25, l_.str.2@PAGEOFF
	stp	x23, x25, [sp]
	str	d0, [sp, #16]
Lloh38:
	adrp	x0, l_.str.7@PAGE
Lloh39:
	add	x0, x0, l_.str.7@PAGEOFF
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x21, x22, [sp, #32]
LBB0_23:                                ; =>This Inner Loop Header: Depth=1
	ldr	x8, [x19]
	ldr	x9, [x20]
	add	x8, x8, #1
	str	x8, [x19]
	eor	x8, x9, #0x1
	str	x8, [x20]
	subs	x26, x26, #1
	b.ne	LBB0_23
; %bb.24:
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x21
	sub	x9, x9, x22
	mov	w24, #51712                     ; =0xca00
	movk	w24, #15258, lsl #16
	madd	x8, x8, x24, x9
	ucvtf	d0, x8
	mov	x8, #20684562497536             ; =0x12d000000000
	movk	x8, #16739, lsl #48
	fmov	d8, x8
	fdiv	d0, d0, d8
	stp	x23, x27, [sp]
	str	d0, [sp, #16]
Lloh40:
	adrp	x19, l_.str.7@PAGE
Lloh41:
	add	x19, x19, l_.str.7@PAGEOFF
	mov	x0, x19
	bl	_printf
	bl	_bench_b_symfony_boot
	ucvtf	d0, x0
	fdiv	d0, d0, d8
	str	d0, [sp, #16]
Lloh42:
	adrp	x26, l_.str.4@PAGE
Lloh43:
	add	x26, x26, l_.str.4@PAGEOFF
	stp	x23, x26, [sp]
	mov	x0, x19
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x21, x22, [sp, #32]
Lloh44:
	adrp	x28, _g_ctx@PAGE
Lloh45:
	add	x28, x28, _g_ctx@PAGEOFF
	mov	w20, #38528                     ; =0x9680
	movk	w20, #152, lsl #16
	; InlineAsm Start
	mov	x0, #0                          ; =0x0
Ltmp8:
	cmp	x0, x20
	b.hs	Ltmp9
	ldr	x1, [x28]
	ldr	x3, [x28, #8]
	add	x1, x1, #16
	add	x3, x3, #1
	str	x1, [x28]
	str	x3, [x28, #8]
	add	x0, x0, #1
	b	Ltmp8
Ltmp9:

	; InlineAsm End
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x21
	sub	x9, x9, x22
	madd	x8, x8, x24, x9
	ucvtf	d0, x8
	fdiv	d0, d0, d8
Lloh46:
	adrp	x21, l_.str.6@PAGE
Lloh47:
	add	x21, x21, l_.str.6@PAGEOFF
Lloh48:
	adrp	x8, l_.str.1@PAGE
Lloh49:
	add	x8, x8, l_.str.1@PAGEOFF
	stp	x21, x8, [sp]
	str	d0, [sp, #16]
	mov	x0, x19
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x22, x23, [sp, #32]
	; InlineAsm Start
	mov	x0, #0                          ; =0x0
Ltmp10:
	cmp	x0, x20
	b.hs	Ltmp11
	ldr	x1, [x28, #16]
	ldr	x3, [x28, #24]
	add	x1, x1, #8
	add	x3, x3, #1
	str	x1, [x28, #16]
	str	x3, [x28, #24]
	add	x0, x0, #1
	b	Ltmp10
Ltmp11:

	; InlineAsm End
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x22
	sub	x9, x9, x23
	madd	x8, x8, x24, x9
	ucvtf	d0, x8
	fdiv	d0, d0, d8
	stp	x21, x25, [sp]
	str	d0, [sp, #16]
	mov	x0, x19
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x22, x23, [sp, #32]
	; InlineAsm Start
	mov	x0, #0                          ; =0x0
Ltmp12:
	cmp	x0, x20
	b.hs	Ltmp13
	ldr	x1, [x28, #32]
	ldr	x3, [x28, #40]
	add	x1, x1, #1
	eor	x3, x3, #0x1
	str	x1, [x28, #32]
	str	x3, [x28, #40]
	add	x0, x0, #1
	b	Ltmp12
Ltmp13:

	; InlineAsm End
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x22
	sub	x9, x9, x23
	madd	x8, x8, x24, x9
	ucvtf	d0, x8
	fdiv	d0, d0, d8
	stp	x21, x27, [sp]
	str	d0, [sp, #16]
	mov	x0, x19
	bl	_printf
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x22, x23, [sp, #32]
	; InlineAsm Start
	mov	x0, #0                          ; =0x0
Ltmp14:
	cmp	x0, x20
	b.hs	Ltmp15
	ldr	x1, [x28, #48]
	ldr	x3, [x28, #56]
	ldr	x4, [x28, #64]
	ldr	x5, [x28, #72]
	ldr	x6, [x28, #80]
	ldr	x7, [x28, #88]
	ldr	x8, [x28, #96]
	ldr	x9, [x28, #104]
	ldr	x10, [x28, #112]
	ldr	x11, [x28, #120]
	ldr	x12, [x28, #128]
	ldr	x13, [x28, #136]
	ldr	x14, [x28, #144]
	ldr	x15, [x28, #152]
	ldr	x16, [x28, #160]
	ldr	x17, [x28, #168]
	add	x1, x1, x3
	add	x1, x1, x4
	add	x1, x1, x5
	add	x1, x1, x6
	add	x1, x1, x7
	add	x1, x1, x8
	add	x1, x1, x9
	add	x1, x1, x10
	add	x1, x1, x11
	add	x1, x1, x12
	add	x1, x1, x13
	add	x1, x1, x14
	add	x1, x1, x15
	add	x1, x1, x16
	add	x1, x1, x17
	str	x1, [x28, #48]
	add	x0, x0, #1
	b	Ltmp14
Ltmp15:

	; InlineAsm End
	add	x1, sp, #32
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp, #32]
	sub	x8, x8, x22
	sub	x9, x9, x23
	madd	x8, x8, x24, x9
	ucvtf	d0, x8
	fdiv	d0, d0, d8
	stp	x21, x26, [sp]
	str	d0, [sp, #16]
	mov	x0, x19
	bl	_printf
	mov	w0, #0                          ; =0x0
	ldp	x29, x30, [sp, #144]            ; 16-byte Folded Reload
	ldp	x20, x19, [sp, #128]            ; 16-byte Folded Reload
	ldp	x22, x21, [sp, #112]            ; 16-byte Folded Reload
	ldp	x24, x23, [sp, #96]             ; 16-byte Folded Reload
	ldp	x26, x25, [sp, #80]             ; 16-byte Folded Reload
	ldp	x28, x27, [sp, #64]             ; 16-byte Folded Reload
	ldp	d9, d8, [sp, #48]               ; 16-byte Folded Reload
	add	sp, sp, #160
	ret
	.loh AdrpLdr	Lloh2, Lloh3
	.loh AdrpLdr	Lloh0, Lloh1
	.loh AdrpLdr	Lloh6, Lloh7
	.loh AdrpLdr	Lloh4, Lloh5
	.loh AdrpLdr	Lloh10, Lloh11
	.loh AdrpLdr	Lloh8, Lloh9
	.loh AdrpAdd	Lloh12, Lloh13
	.loh AdrpAdd	Lloh18, Lloh19
	.loh AdrpAdd	Lloh16, Lloh17
	.loh AdrpAdd	Lloh14, Lloh15
	.loh AdrpAdd	Lloh22, Lloh23
	.loh AdrpAdd	Lloh20, Lloh21
	.loh AdrpAdd	Lloh28, Lloh29
	.loh AdrpAdd	Lloh26, Lloh27
	.loh AdrpAdd	Lloh24, Lloh25
	.loh AdrpAdd	Lloh34, Lloh35
	.loh AdrpAdd	Lloh32, Lloh33
	.loh AdrpAdd	Lloh30, Lloh31
	.loh AdrpAdd	Lloh38, Lloh39
	.loh AdrpAdd	Lloh36, Lloh37
	.loh AdrpAdd	Lloh48, Lloh49
	.loh AdrpAdd	Lloh46, Lloh47
	.loh AdrpAdd	Lloh44, Lloh45
	.loh AdrpAdd	Lloh42, Lloh43
	.loh AdrpAdd	Lloh40, Lloh41
	.cfi_endproc
                                        ; -- End function
	.p2align	2                               ; -- Begin function bench_a_symfony_boot
_bench_a_symfony_boot:                  ; @bench_a_symfony_boot
	.cfi_startproc
; %bb.0:
	sub	sp, sp, #48
	stp	x20, x19, [sp, #16]             ; 16-byte Folded Spill
	stp	x29, x30, [sp, #32]             ; 16-byte Folded Spill
	add	x29, sp, #32
	.cfi_def_cfa w29, 16
	.cfi_offset w30, -8
	.cfi_offset w29, -16
	.cfi_offset w19, -24
	.cfi_offset w20, -32
	mov	x1, sp
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x20, x19, [sp]
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
Lloh50:
	adrp	x9, _g_boot@PAGE
Lloh51:
	add	x9, x9, _g_boot@PAGEOFF
LBB1_1:                                 ; =>This Inner Loop Header: Depth=1
	ldr	x10, [x9]
	ldr	x11, [x9, #8]
	add	x10, x11, x10
	ldr	x11, [x9, #16]
	ldr	x12, [x9, #24]
	add	x11, x11, x12
	add	x10, x10, x11
	ldr	x11, [x9, #32]
	ldr	x12, [x9, #40]
	ldr	x13, [x9, #48]
	add	x11, x11, x12
	add	x11, x11, x13
	add	x10, x10, x11
	ldr	x11, [x9, #56]
	ldr	x12, [x9, #64]
	ldr	x13, [x9, #72]
	add	x11, x11, x12
	add	x11, x11, x13
	ldr	x12, [x9, #80]
	add	x11, x11, x12
	add	x10, x10, x11
	ldr	x11, [x9, #88]
	ldr	x12, [x9, #96]
	ldr	x13, [x9, #104]
	add	x11, x11, x12
	add	x11, x11, x13
	ldr	x12, [x9, #112]
	ldr	x13, [x9, #120]
	add	x11, x11, x12
	add	x11, x11, x13
	add	x10, x10, x11
	str	x10, [x9]
	subs	x8, x8, #1
	b.ne	LBB1_1
; %bb.2:
	mov	x1, sp
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp]
	sub	x8, x8, x20
	mov	w10, #51712                     ; =0xca00
	movk	w10, #15258, lsl #16
	sub	x9, x9, x19
	madd	x0, x8, x10, x9
	ldp	x29, x30, [sp, #32]             ; 16-byte Folded Reload
	ldp	x20, x19, [sp, #16]             ; 16-byte Folded Reload
	add	sp, sp, #48
	ret
	.loh AdrpAdd	Lloh50, Lloh51
	.cfi_endproc
                                        ; -- End function
	.p2align	2                               ; -- Begin function bench_b_symfony_boot
_bench_b_symfony_boot:                  ; @bench_b_symfony_boot
	.cfi_startproc
; %bb.0:
	sub	sp, sp, #48
	stp	x20, x19, [sp, #16]             ; 16-byte Folded Spill
	stp	x29, x30, [sp, #32]             ; 16-byte Folded Spill
	add	x29, sp, #32
	.cfi_def_cfa w29, 16
	.cfi_offset w30, -8
	.cfi_offset w29, -16
	.cfi_offset w19, -24
	.cfi_offset w20, -32
	mov	x1, sp
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x20, x19, [sp]
Lloh52:
	adrp	x0, _g_boot_b@TLVPPAGE
Lloh53:
	ldr	x0, [x0, _g_boot_b@TLVPPAGEOFF]
	ldr	x8, [x0]
	blr	x8
	mov	w8, #38528                      ; =0x9680
	movk	w8, #152, lsl #16
LBB2_1:                                 ; =>This Inner Loop Header: Depth=1
	ldr	x9, [x0]
	ldr	x10, [x0, #8]
	add	x9, x10, x9
	ldr	x10, [x0, #16]
	ldr	x11, [x0, #24]
	add	x10, x10, x11
	add	x9, x9, x10
	ldr	x10, [x0, #32]
	ldr	x11, [x0, #40]
	ldr	x12, [x0, #48]
	add	x10, x10, x11
	add	x10, x10, x12
	add	x9, x9, x10
	ldr	x10, [x0, #56]
	ldr	x11, [x0, #64]
	ldr	x12, [x0, #72]
	add	x10, x10, x11
	add	x10, x10, x12
	ldr	x11, [x0, #80]
	add	x10, x10, x11
	add	x9, x9, x10
	ldr	x10, [x0, #88]
	ldr	x11, [x0, #96]
	ldr	x12, [x0, #104]
	add	x10, x10, x11
	add	x10, x10, x12
	ldr	x11, [x0, #112]
	ldr	x12, [x0, #120]
	add	x10, x10, x11
	add	x10, x10, x12
	add	x9, x9, x10
	str	x9, [x0]
	subs	x8, x8, #1
	b.ne	LBB2_1
; %bb.2:
	mov	x1, sp
	mov	w0, #6                          ; =0x6
	bl	_clock_gettime
	ldp	x8, x9, [sp]
	sub	x8, x8, x20
	mov	w10, #51712                     ; =0xca00
	movk	w10, #15258, lsl #16
	sub	x9, x9, x19
	madd	x0, x8, x10, x9
	ldp	x29, x30, [sp, #32]             ; 16-byte Folded Reload
	ldp	x20, x19, [sp, #16]             ; 16-byte Folded Reload
	add	sp, sp, #48
	ret
	.loh AdrpLdr	Lloh52, Lloh53
	.cfi_endproc
                                        ; -- End function
	.section	__TEXT,__cstring,cstring_literals
l_.str:                                 ; @.str
	.asciz	"baseline"

l_.str.1:                               ; @.str.1
	.asciz	"str_repeat_alloc"

l_.str.2:                               ; @.str.2
	.asciz	"array_push_alloc"

l_.str.3:                               ; @.str.3
	.asciz	"json_encode"

l_.str.4:                               ; @.str.4
	.asciz	"symfony_boot"

l_.str.5:                               ; @.str.5
	.asciz	"native_tls"

l_.str.6:                               ; @.str.6
	.asciz	"ctx_reg"

.zerofill __DATA,__bss,_g_buf_ptr_a,8,3 ; @g_buf_ptr_a
.zerofill __DATA,__bss,_g_buf_len_a,8,3 ; @g_buf_len_a
.zerofill __DATA,__bss,_g_arr_ptr_a,8,3 ; @g_arr_ptr_a
.zerofill __DATA,__bss,_g_arr_cap_a,8,3 ; @g_arr_cap_a
.zerofill __DATA,__bss,_g_depth_a,8,3   ; @g_depth_a
.zerofill __DATA,__bss,_g_flags_a,8,3   ; @g_flags_a
.zerofill __DATA,__bss,_g_boot,128,3    ; @g_boot
.tbss _g_buf_ptr_b$tlv$init, 8, 3       ; @g_buf_ptr_b

	.section	__DATA,__thread_vars,thread_local_variables
_g_buf_ptr_b:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_g_buf_ptr_b$tlv$init

.tbss _g_buf_len_b$tlv$init, 8, 3       ; @g_buf_len_b

_g_buf_len_b:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_g_buf_len_b$tlv$init

.tbss _g_arr_ptr_b$tlv$init, 8, 3       ; @g_arr_ptr_b

_g_arr_ptr_b:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_g_arr_ptr_b$tlv$init

.tbss _g_arr_cap_b$tlv$init, 8, 3       ; @g_arr_cap_b

_g_arr_cap_b:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_g_arr_cap_b$tlv$init

.tbss _g_depth_b$tlv$init, 8, 3         ; @g_depth_b

_g_depth_b:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_g_depth_b$tlv$init

.tbss _g_flags_b$tlv$init, 8, 3         ; @g_flags_b

_g_flags_b:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_g_flags_b$tlv$init

.tbss _g_boot_b$tlv$init, 128, 3        ; @g_boot_b

_g_boot_b:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_g_boot_b$tlv$init

.zerofill __DATA,__bss,_g_ctx,176,3     ; @g_ctx
	.section	__TEXT,__cstring,cstring_literals
l_.str.7:                               ; @.str.7
	.asciz	"%s\t%s\t%.3f\n"

.subsections_via_symbols
