use crate::codegen::emit::Emitter;

/// strcopy: copy a string to concat_buf (for in-place modification).
/// Input:  x1=ptr, x2=len
/// Output: x1=new_ptr (in concat_buf), x2=len (unchanged)
pub fn emit_strcopy(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcopy ---");
    emitter.label("__rt_strcopy");
    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp]");
    emitter.instruction("mov x29, sp");

    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8"); // dest

    emitter.instruction("mov x10, x9"); // save start
    emitter.instruction("mov x11, x2"); // save len
    emitter.label("__rt_strcopy_loop");
    emitter.instruction("cbz x11, __rt_strcopy_done");
    emitter.instruction("ldrb w12, [x1], #1");
    emitter.instruction("strb w12, [x9], #1");
    emitter.instruction("sub x11, x11, #1");
    emitter.instruction("b __rt_strcopy_loop");

    emitter.label("__rt_strcopy_done");
    emitter.instruction("add x8, x8, x2");
    emitter.instruction("str x8, [x6]");
    emitter.instruction("mov x1, x10"); // return new ptr
    // x2 unchanged

    emitter.instruction("ldp x29, x30, [sp]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

/// strtolower: copy string to concat_buf, lowercasing A-Z.
/// Input:  x1=ptr, x2=len
/// Output: x1=new_ptr, x2=len
pub fn emit_strtolower(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtolower ---");
    emitter.label("__rt_strtolower");
    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp]");
    emitter.instruction("mov x29, sp");

    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8");
    emitter.instruction("mov x10, x9");
    emitter.instruction("mov x11, x2");

    emitter.label("__rt_strtolower_loop");
    emitter.instruction("cbz x11, __rt_strtolower_done");
    emitter.instruction("ldrb w12, [x1], #1");
    emitter.instruction("cmp w12, #65"); // 'A'
    emitter.instruction("b.lt __rt_strtolower_store");
    emitter.instruction("cmp w12, #90"); // 'Z'
    emitter.instruction("b.gt __rt_strtolower_store");
    emitter.instruction("add w12, w12, #32");
    emitter.label("__rt_strtolower_store");
    emitter.instruction("strb w12, [x9], #1");
    emitter.instruction("sub x11, x11, #1");
    emitter.instruction("b __rt_strtolower_loop");

    emitter.label("__rt_strtolower_done");
    emitter.instruction("add x8, x8, x2");
    emitter.instruction("str x8, [x6]");
    emitter.instruction("mov x1, x10");
    emitter.instruction("ldp x29, x30, [sp]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

/// strtoupper: copy string to concat_buf, uppercasing a-z.
pub fn emit_strtoupper(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtoupper ---");
    emitter.label("__rt_strtoupper");
    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp]");
    emitter.instruction("mov x29, sp");

    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8");
    emitter.instruction("mov x10, x9");
    emitter.instruction("mov x11, x2");

    emitter.label("__rt_strtoupper_loop");
    emitter.instruction("cbz x11, __rt_strtoupper_done");
    emitter.instruction("ldrb w12, [x1], #1");
    emitter.instruction("cmp w12, #97"); // 'a'
    emitter.instruction("b.lt __rt_strtoupper_store");
    emitter.instruction("cmp w12, #122"); // 'z'
    emitter.instruction("b.gt __rt_strtoupper_store");
    emitter.instruction("sub w12, w12, #32");
    emitter.label("__rt_strtoupper_store");
    emitter.instruction("strb w12, [x9], #1");
    emitter.instruction("sub x11, x11, #1");
    emitter.instruction("b __rt_strtoupper_loop");

    emitter.label("__rt_strtoupper_done");
    emitter.instruction("add x8, x8, x2");
    emitter.instruction("str x8, [x6]");
    emitter.instruction("mov x1, x10");
    emitter.instruction("ldp x29, x30, [sp]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

/// trim: strip whitespace from both ends. Returns adjusted ptr+len (no copy needed).
pub fn emit_trim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim ---");
    // ltrim first, then rtrim
    emitter.label("__rt_trim");
    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp]");
    emitter.instruction("mov x29, sp");
    emitter.instruction("bl __rt_ltrim");
    emitter.instruction("bl __rt_rtrim");
    emitter.instruction("ldp x29, x30, [sp]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

/// ltrim: strip whitespace from left. Adjusts x1 and x2.
pub fn emit_ltrim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ltrim ---");
    emitter.label("__rt_ltrim");
    emitter.label("__rt_ltrim_loop");
    emitter.instruction("cbz x2, __rt_ltrim_done");
    emitter.instruction("ldrb w9, [x1]");
    emitter.instruction("cmp w9, #32"); // space
    emitter.instruction("b.eq __rt_ltrim_skip");
    emitter.instruction("cmp w9, #9"); // tab
    emitter.instruction("b.eq __rt_ltrim_skip");
    emitter.instruction("cmp w9, #10"); // newline
    emitter.instruction("b.eq __rt_ltrim_skip");
    emitter.instruction("cmp w9, #13"); // carriage return
    emitter.instruction("b.eq __rt_ltrim_skip");
    emitter.instruction("b __rt_ltrim_done");
    emitter.label("__rt_ltrim_skip");
    emitter.instruction("add x1, x1, #1");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("b __rt_ltrim_loop");
    emitter.label("__rt_ltrim_done");
    emitter.instruction("ret");
}

/// rtrim: strip whitespace from right. Adjusts x2.
pub fn emit_rtrim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rtrim ---");
    emitter.label("__rt_rtrim");
    emitter.label("__rt_rtrim_loop");
    emitter.instruction("cbz x2, __rt_rtrim_done");
    emitter.instruction("sub x9, x2, #1");
    emitter.instruction("ldrb w10, [x1, x9]");
    emitter.instruction("cmp w10, #32");
    emitter.instruction("b.eq __rt_rtrim_strip");
    emitter.instruction("cmp w10, #9");
    emitter.instruction("b.eq __rt_rtrim_strip");
    emitter.instruction("cmp w10, #10");
    emitter.instruction("b.eq __rt_rtrim_strip");
    emitter.instruction("cmp w10, #13");
    emitter.instruction("b.eq __rt_rtrim_strip");
    emitter.instruction("b __rt_rtrim_done");
    emitter.label("__rt_rtrim_strip");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("b __rt_rtrim_loop");
    emitter.label("__rt_rtrim_done");
    emitter.instruction("ret");
}

/// strpos: find needle in haystack. Returns position in x0, or -1 if not found.
/// Input: x1=haystack_ptr, x2=haystack_len, x3=needle_ptr, x4=needle_len
/// Output: x0 = position (or -1)
pub fn emit_strpos(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strpos ---");
    emitter.label("__rt_strpos");
    // Edge case: empty needle returns 0
    emitter.instruction("cbz x4, __rt_strpos_empty");
    // If needle longer than haystack, not found
    emitter.instruction("cmp x4, x2");
    emitter.instruction("b.gt __rt_strpos_notfound");
    emitter.instruction("mov x5, #0"); // current position

    emitter.label("__rt_strpos_outer");
    emitter.instruction("sub x9, x2, x4");
    emitter.instruction("cmp x5, x9");
    emitter.instruction("b.gt __rt_strpos_notfound");
    // Compare needle at current position
    emitter.instruction("mov x6, #0"); // needle index
    emitter.label("__rt_strpos_inner");
    emitter.instruction("cmp x6, x4");
    emitter.instruction("b.ge __rt_strpos_found");
    emitter.instruction("add x7, x5, x6");
    emitter.instruction("ldrb w8, [x1, x7]");
    emitter.instruction("ldrb w9, [x3, x6]");
    emitter.instruction("cmp w8, w9");
    emitter.instruction("b.ne __rt_strpos_next");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("b __rt_strpos_inner");

    emitter.label("__rt_strpos_next");
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("b __rt_strpos_outer");

    emitter.label("__rt_strpos_found");
    emitter.instruction("mov x0, x5");
    emitter.instruction("ret");
    emitter.label("__rt_strpos_empty");
    emitter.instruction("mov x0, #0");
    emitter.instruction("ret");
    emitter.label("__rt_strpos_notfound");
    emitter.instruction("mov x0, #-1");
    emitter.instruction("ret");
}

/// strrpos: find last occurrence of needle. Returns position or -1.
pub fn emit_strrpos(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrpos ---");
    emitter.label("__rt_strrpos");
    emitter.instruction("cbz x4, __rt_strrpos_empty");
    emitter.instruction("cmp x4, x2");
    emitter.instruction("b.gt __rt_strrpos_notfound");
    emitter.instruction("sub x5, x2, x4"); // start from end

    emitter.label("__rt_strrpos_outer");
    emitter.instruction("mov x6, #0");
    emitter.label("__rt_strrpos_inner");
    emitter.instruction("cmp x6, x4");
    emitter.instruction("b.ge __rt_strrpos_found");
    emitter.instruction("add x7, x5, x6");
    emitter.instruction("ldrb w8, [x1, x7]");
    emitter.instruction("ldrb w9, [x3, x6]");
    emitter.instruction("cmp w8, w9");
    emitter.instruction("b.ne __rt_strrpos_prev");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("b __rt_strrpos_inner");

    emitter.label("__rt_strrpos_prev");
    emitter.instruction("cbz x5, __rt_strrpos_notfound");
    emitter.instruction("sub x5, x5, #1");
    emitter.instruction("b __rt_strrpos_outer");

    emitter.label("__rt_strrpos_found");
    emitter.instruction("mov x0, x5");
    emitter.instruction("ret");
    emitter.label("__rt_strrpos_empty");
    emitter.instruction("sub x0, x2, #0"); // last position
    emitter.instruction("ret");
    emitter.label("__rt_strrpos_notfound");
    emitter.instruction("mov x0, #-1");
    emitter.instruction("ret");
}

/// str_repeat: repeat a string N times into concat_buf.
/// Input: x1=ptr, x2=len, x3=times
/// Output: x1=result_ptr, x2=result_len
pub fn emit_str_repeat(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_repeat ---");
    emitter.label("__rt_str_repeat");
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("stp x1, x2, [sp]");    // save src ptr/len
    emitter.instruction("str x3, [sp, #16]");    // save times

    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8");
    emitter.instruction("str x9, [sp, #24]"); // save result start

    emitter.instruction("mov x10, x3"); // counter
    emitter.label("__rt_str_repeat_loop");
    emitter.instruction("cbz x10, __rt_str_repeat_done");
    emitter.instruction("ldp x1, x2, [sp]");
    emitter.instruction("mov x11, x2");
    emitter.label("__rt_str_repeat_copy");
    emitter.instruction("cbz x11, __rt_str_repeat_next");
    emitter.instruction("ldrb w12, [x1], #1");
    emitter.instruction("strb w12, [x9], #1");
    emitter.instruction("sub x11, x11, #1");
    emitter.instruction("b __rt_str_repeat_copy");
    emitter.label("__rt_str_repeat_next");
    emitter.instruction("sub x10, x10, #1");
    emitter.instruction("b __rt_str_repeat_loop");

    emitter.label("__rt_str_repeat_done");
    emitter.instruction("ldr x1, [sp, #24]"); // result start
    emitter.instruction("sub x2, x9, x1");    // result len
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("add x8, x8, x2");
    emitter.instruction("str x8, [x6]");
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// strrev: reverse a string into concat_buf.
pub fn emit_strrev(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label("__rt_strrev");
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8");
    emitter.instruction("mov x10, x9"); // save start
    emitter.instruction("add x11, x1, x2"); // end of source
    emitter.instruction("mov x12, x2");

    emitter.label("__rt_strrev_loop");
    emitter.instruction("cbz x12, __rt_strrev_done");
    emitter.instruction("sub x11, x11, #1");
    emitter.instruction("ldrb w13, [x11]");
    emitter.instruction("strb w13, [x9], #1");
    emitter.instruction("sub x12, x12, #1");
    emitter.instruction("b __rt_strrev_loop");

    emitter.label("__rt_strrev_done");
    emitter.instruction("add x8, x8, x2");
    emitter.instruction("str x8, [x6]");
    emitter.instruction("mov x1, x10");
    // x2 unchanged
    emitter.instruction("ret");
}

/// chr: convert int to single-character string.
/// Input: x0 = char code
/// Output: x1 = ptr, x2 = 1
pub fn emit_chr(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: chr ---");
    emitter.label("__rt_chr");
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x1, x7, x8");
    emitter.instruction("strb w0, [x1]");
    emitter.instruction("add x8, x8, #1");
    emitter.instruction("str x8, [x6]");
    emitter.instruction("mov x2, #1");
    emitter.instruction("ret");
}

/// strcmp: compare two strings lexicographically.
/// Input: x1/x2 = str_a, x3/x4 = str_b
/// Output: x0 = <0, 0, or >0
pub fn emit_strcmp(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcmp ---");
    emitter.label("__rt_strcmp");
    // Compare min(len_a, len_b) bytes
    emitter.instruction("cmp x2, x4");
    emitter.instruction("csel x5, x2, x4, lt"); // min len
    emitter.instruction("mov x6, #0"); // index

    emitter.label("__rt_strcmp_loop");
    emitter.instruction("cmp x6, x5");
    emitter.instruction("b.ge __rt_strcmp_len");
    emitter.instruction("ldrb w7, [x1, x6]");
    emitter.instruction("ldrb w8, [x3, x6]");
    emitter.instruction("cmp w7, w8");
    emitter.instruction("b.ne __rt_strcmp_diff");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("b __rt_strcmp_loop");

    emitter.label("__rt_strcmp_diff");
    emitter.instruction("sub x0, x7, x8");
    emitter.instruction("ret");
    emitter.label("__rt_strcmp_len");
    emitter.instruction("sub x0, x2, x4"); // compare by length
    emitter.instruction("ret");
}

/// strcasecmp: case-insensitive string comparison.
pub fn emit_strcasecmp(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcasecmp ---");
    emitter.label("__rt_strcasecmp");
    emitter.instruction("cmp x2, x4");
    emitter.instruction("csel x5, x2, x4, lt");
    emitter.instruction("mov x6, #0");

    emitter.label("__rt_strcasecmp_loop");
    emitter.instruction("cmp x6, x5");
    emitter.instruction("b.ge __rt_strcasecmp_len");
    emitter.instruction("ldrb w7, [x1, x6]");
    emitter.instruction("ldrb w8, [x3, x6]");
    // tolower both
    emitter.instruction("cmp w7, #65");
    emitter.instruction("b.lt __rt_strcasecmp_b");
    emitter.instruction("cmp w7, #90");
    emitter.instruction("b.gt __rt_strcasecmp_b");
    emitter.instruction("add w7, w7, #32");
    emitter.label("__rt_strcasecmp_b");
    emitter.instruction("cmp w8, #65");
    emitter.instruction("b.lt __rt_strcasecmp_cmp");
    emitter.instruction("cmp w8, #90");
    emitter.instruction("b.gt __rt_strcasecmp_cmp");
    emitter.instruction("add w8, w8, #32");
    emitter.label("__rt_strcasecmp_cmp");
    emitter.instruction("cmp w7, w8");
    emitter.instruction("b.ne __rt_strcasecmp_diff");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("b __rt_strcasecmp_loop");

    emitter.label("__rt_strcasecmp_diff");
    emitter.instruction("sub x0, x7, x8");
    emitter.instruction("ret");
    emitter.label("__rt_strcasecmp_len");
    emitter.instruction("sub x0, x2, x4");
    emitter.instruction("ret");
}

/// str_starts_with: check if haystack starts with needle.
/// Input: x1/x2=haystack, x3/x4=needle
/// Output: x0 = 1 if starts with, 0 otherwise
pub fn emit_str_starts_with(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_starts_with ---");
    emitter.label("__rt_str_starts_with");
    emitter.instruction("cmp x4, x2");
    emitter.instruction("b.gt __rt_str_starts_with_no");
    emitter.instruction("mov x5, #0");
    emitter.label("__rt_str_starts_with_loop");
    emitter.instruction("cmp x5, x4");
    emitter.instruction("b.ge __rt_str_starts_with_yes");
    emitter.instruction("ldrb w6, [x1, x5]");
    emitter.instruction("ldrb w7, [x3, x5]");
    emitter.instruction("cmp w6, w7");
    emitter.instruction("b.ne __rt_str_starts_with_no");
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("b __rt_str_starts_with_loop");
    emitter.label("__rt_str_starts_with_yes");
    emitter.instruction("mov x0, #1");
    emitter.instruction("ret");
    emitter.label("__rt_str_starts_with_no");
    emitter.instruction("mov x0, #0");
    emitter.instruction("ret");
}

/// str_ends_with: check if haystack ends with needle.
pub fn emit_str_ends_with(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_ends_with ---");
    emitter.label("__rt_str_ends_with");
    emitter.instruction("cmp x4, x2");
    emitter.instruction("b.gt __rt_str_ends_with_no");
    emitter.instruction("sub x5, x2, x4"); // offset in haystack
    emitter.instruction("mov x6, #0");
    emitter.label("__rt_str_ends_with_loop");
    emitter.instruction("cmp x6, x4");
    emitter.instruction("b.ge __rt_str_ends_with_yes");
    emitter.instruction("add x7, x5, x6");
    emitter.instruction("ldrb w8, [x1, x7]");
    emitter.instruction("ldrb w9, [x3, x6]");
    emitter.instruction("cmp w8, w9");
    emitter.instruction("b.ne __rt_str_ends_with_no");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("b __rt_str_ends_with_loop");
    emitter.label("__rt_str_ends_with_yes");
    emitter.instruction("mov x0, #1");
    emitter.instruction("ret");
    emitter.label("__rt_str_ends_with_no");
    emitter.instruction("mov x0, #0");
    emitter.instruction("ret");
}

/// str_replace: replace all occurrences of search with replace in subject.
/// Input: x1/x2=search, x3/x4=replace, x5/x6=subject
/// Output: x1=result_ptr, x2=result_len (in concat_buf)
pub fn emit_str_replace(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_replace ---");
    emitter.label("__rt_str_replace");
    emitter.instruction("sub sp, sp, #80");
    emitter.instruction("stp x29, x30, [sp, #64]");
    emitter.instruction("add x29, sp, #64");
    // Save args
    emitter.instruction("stp x1, x2, [sp]");    // search ptr/len
    emitter.instruction("stp x3, x4, [sp, #16]"); // replace ptr/len
    emitter.instruction("stp x5, x6, [sp, #32]"); // subject ptr/len

    // Get concat_buf dest
    emitter.instruction("adrp x9, _concat_off@PAGE");
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("adrp x11, _concat_buf@PAGE");
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");
    emitter.instruction("add x12, x11, x10"); // dest
    emitter.instruction("str x12, [sp, #48]"); // save result start
    emitter.instruction("str x9, [sp, #56]");  // save offset ptr

    emitter.instruction("mov x13, #0"); // subject index

    emitter.label("__rt_str_replace_loop");
    emitter.instruction("ldp x5, x6, [sp, #32]"); // subject
    emitter.instruction("cmp x13, x6");
    emitter.instruction("b.ge __rt_str_replace_done");

    // Check if search matches at current position
    emitter.instruction("ldp x1, x2, [sp]"); // search
    emitter.instruction("cbz x2, __rt_str_replace_copy_byte"); // empty search = no match
    emitter.instruction("sub x14, x6, x13"); // remaining
    emitter.instruction("cmp x2, x14");
    emitter.instruction("b.gt __rt_str_replace_copy_byte");

    emitter.instruction("mov x15, #0"); // match index
    emitter.label("__rt_str_replace_match");
    emitter.instruction("cmp x15, x2");
    emitter.instruction("b.ge __rt_str_replace_found");
    emitter.instruction("add x16, x13, x15");
    emitter.instruction("ldrb w17, [x5, x16]");
    emitter.instruction("ldrb w18, [x1, x15]");
    emitter.instruction("cmp w17, w18");
    emitter.instruction("b.ne __rt_str_replace_copy_byte");
    emitter.instruction("add x15, x15, #1");
    emitter.instruction("b __rt_str_replace_match");

    emitter.label("__rt_str_replace_found");
    // Copy replacement
    emitter.instruction("ldp x3, x4, [sp, #16]");
    emitter.instruction("mov x15, #0");
    emitter.label("__rt_str_replace_rep_copy");
    emitter.instruction("cmp x15, x4");
    emitter.instruction("b.ge __rt_str_replace_rep_done");
    emitter.instruction("ldrb w17, [x3, x15]");
    emitter.instruction("strb w17, [x12], #1");
    emitter.instruction("add x15, x15, #1");
    emitter.instruction("b __rt_str_replace_rep_copy");
    emitter.label("__rt_str_replace_rep_done");
    emitter.instruction("ldp x1, x2, [sp]");
    emitter.instruction("add x13, x13, x2"); // skip search length
    emitter.instruction("b __rt_str_replace_loop");

    emitter.label("__rt_str_replace_copy_byte");
    emitter.instruction("ldp x5, x6, [sp, #32]");
    emitter.instruction("ldrb w17, [x5, x13]");
    emitter.instruction("strb w17, [x12], #1");
    emitter.instruction("add x13, x13, #1");
    emitter.instruction("b __rt_str_replace_loop");

    emitter.label("__rt_str_replace_done");
    emitter.instruction("ldr x1, [sp, #48]"); // result start
    emitter.instruction("sub x2, x12, x1");   // result len
    emitter.instruction("ldr x9, [sp, #56]");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("add x10, x10, x2");
    emitter.instruction("str x10, [x9]");
    emitter.instruction("ldp x29, x30, [sp, #64]");
    emitter.instruction("add sp, sp, #80");
    emitter.instruction("ret");
}

/// explode: split string by delimiter into array of strings.
/// Input: x1/x2=delimiter, x3/x4=string
/// Output: x0 = array pointer
pub fn emit_explode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: explode ---");
    emitter.label("__rt_explode");
    emitter.instruction("sub sp, sp, #80");
    emitter.instruction("stp x29, x30, [sp, #64]");
    emitter.instruction("add x29, sp, #64");
    emitter.instruction("stp x1, x2, [sp]");    // delimiter
    emitter.instruction("stp x3, x4, [sp, #16]"); // string

    // Create new string array
    emitter.instruction("mov x0, #16"); // initial capacity
    emitter.instruction("mov x1, #16"); // elem_size (ptr+len)
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("str x0, [sp, #32]"); // array ptr

    emitter.instruction("mov x13, #0"); // current position in string
    emitter.instruction("str x13, [sp, #40]"); // current scan pos
    emitter.instruction("str x13, [sp, #48]"); // segment start = 0

    emitter.label("__rt_explode_loop");
    emitter.instruction("ldp x3, x4, [sp, #16]"); // string
    emitter.instruction("ldr x13, [sp, #40]"); // seg start (actually current scan pos)
    emitter.instruction("cmp x13, x4");
    emitter.instruction("b.ge __rt_explode_last");

    // Search for delimiter at position x13
    emitter.instruction("ldp x1, x2, [sp]"); // delimiter
    emitter.instruction("sub x14, x4, x13"); // remaining
    emitter.instruction("cmp x2, x14");
    emitter.instruction("b.gt __rt_explode_last"); // delimiter longer than remaining

    // Compare delimiter
    emitter.instruction("mov x15, #0");
    emitter.label("__rt_explode_cmp");
    emitter.instruction("cmp x15, x2");
    emitter.instruction("b.ge __rt_explode_match");
    emitter.instruction("add x16, x13, x15");
    emitter.instruction("ldrb w17, [x3, x16]");
    emitter.instruction("ldrb w18, [x1, x15]");
    emitter.instruction("cmp w17, w18");
    emitter.instruction("b.ne __rt_explode_advance");
    emitter.instruction("add x15, x15, #1");
    emitter.instruction("b __rt_explode_cmp");

    emitter.label("__rt_explode_advance");
    // Not a match, advance by 1
    emitter.instruction("add x13, x13, #1");
    emitter.instruction("str x13, [sp, #40]");
    emitter.instruction("b __rt_explode_loop");

    emitter.label("__rt_explode_match");
    // Found delimiter at x13. Segment is from last_start to x13.
    // We need to track segment start separately
    // For simplicity, push current segment and update start
    emitter.instruction("ldr x0, [sp, #32]"); // array
    emitter.instruction("ldp x3, x4, [sp, #16]"); // string
    // We need segment start — walk back to find it
    // Actually let's rethink: use sp+48 as segment_start
    emitter.instruction("ldr x16, [sp, #48]"); // segment start (init to 0)
    emitter.instruction("add x1, x3, x16");    // segment ptr
    emitter.instruction("sub x2, x13, x16");   // segment len
    emitter.instruction("bl __rt_array_push_str");
    // Update segment start past delimiter
    emitter.instruction("ldp x1, x2, [sp]"); // delimiter len
    emitter.instruction("ldr x13, [sp, #40]");
    emitter.instruction("add x13, x13, x2");
    emitter.instruction("str x13, [sp, #40]");
    emitter.instruction("str x13, [sp, #48]"); // new segment start
    emitter.instruction("b __rt_explode_loop");

    emitter.label("__rt_explode_last");
    // Push remaining segment
    emitter.instruction("ldr x0, [sp, #32]");
    emitter.instruction("ldp x3, x4, [sp, #16]");
    emitter.instruction("ldr x16, [sp, #48]");
    emitter.instruction("add x1, x3, x16");
    emitter.instruction("sub x2, x4, x16");
    emitter.instruction("bl __rt_array_push_str");

    emitter.instruction("ldr x0, [sp, #32]"); // return array
    emitter.instruction("ldp x29, x30, [sp, #64]");
    emitter.instruction("add sp, sp, #80");
    emitter.instruction("ret");
}

/// implode: join array elements with glue string.
/// Input: x1/x2=glue, x3=array_ptr
/// Output: x1=result_ptr, x2=result_len
pub fn emit_implode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode ---");
    emitter.label("__rt_implode");
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");
    emitter.instruction("stp x1, x2, [sp]");    // glue
    emitter.instruction("str x3, [sp, #16]");    // array

    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8"); // dest
    emitter.instruction("str x9, [sp, #24]"); // result start
    emitter.instruction("str x6, [sp, #32]"); // offset ptr

    emitter.instruction("ldr x3, [sp, #16]");
    emitter.instruction("ldr x10, [x3]"); // array length
    emitter.instruction("mov x11, #0");   // index

    emitter.label("__rt_implode_loop");
    emitter.instruction("cmp x11, x10");
    emitter.instruction("b.ge __rt_implode_done");

    // Add glue before element (except first)
    emitter.instruction("cbz x11, __rt_implode_elem");
    emitter.instruction("ldp x1, x2, [sp]");
    emitter.instruction("mov x12, x2");
    emitter.label("__rt_implode_glue");
    emitter.instruction("cbz x12, __rt_implode_elem");
    emitter.instruction("ldrb w13, [x1], #1");
    emitter.instruction("strb w13, [x9], #1");
    emitter.instruction("sub x12, x12, #1");
    emitter.instruction("b __rt_implode_glue");

    emitter.label("__rt_implode_elem");
    // Load element [index] from array (string = 16 bytes: ptr + len)
    emitter.instruction("ldr x3, [sp, #16]");
    emitter.instruction("lsl x12, x11, #4"); // index * 16
    emitter.instruction("add x12, x3, x12");
    emitter.instruction("add x12, x12, #24"); // skip header
    emitter.instruction("ldr x1, [x12]");     // elem ptr
    emitter.instruction("ldr x2, [x12, #8]"); // elem len

    emitter.instruction("mov x12, x2");
    emitter.label("__rt_implode_copy");
    emitter.instruction("cbz x12, __rt_implode_next");
    emitter.instruction("ldrb w13, [x1], #1");
    emitter.instruction("strb w13, [x9], #1");
    emitter.instruction("sub x12, x12, #1");
    emitter.instruction("b __rt_implode_copy");

    emitter.label("__rt_implode_next");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_implode_loop");

    emitter.label("__rt_implode_done");
    emitter.instruction("ldr x1, [sp, #24]");
    emitter.instruction("sub x2, x9, x1");
    emitter.instruction("ldr x6, [sp, #32]");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("add x8, x8, x2");
    emitter.instruction("str x8, [x6]");
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}
