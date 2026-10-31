.globl __rt_parse_url_common
__rt_parse_url_common:
    sub sp, sp, #224
    stp x29, x30, [sp, #208]
    add x29, sp, #208
    str x1, [sp, #0]
    str x2, [sp, #8]
    str x3, [sp, #16]
    str x5, [sp, #192]
    stp xzr, xzr, [sp, #24]
    stp xzr, xzr, [sp, #40]
    stp xzr, xzr, [sp, #56]
    stp xzr, xzr, [sp, #72]
    stp xzr, xzr, [sp, #88]
    stp xzr, xzr, [sp, #104]
    stp xzr, xzr, [sp, #120]
    stp xzr, xzr, [sp, #136]
    stp xzr, xzr, [sp, #152]
    stp xzr, xzr, [sp, #168]
    str xzr, [sp, #184]

    // Locate the first colon, if any.
    mov x9, x1
    mov x10, #0
Lparse_url_find_colon:
    cmp x10, x2
    b.eq Lparse_url_no_colon
    ldrb w11, [x9, x10]
    cmp w11, #58
    b.eq Lparse_url_colon_found
    add x10, x10, #1
    b Lparse_url_find_colon

Lparse_url_colon_found:
    str x10, [sp, #160]
    cbz x10, Lparse_url_parse_port

    // A scheme is ASCII alphanumeric plus '+', '-', and '.'.
    mov x11, #0
Lparse_url_validate_scheme:
    cmp x11, x10
    b.eq Lparse_url_valid_scheme
    ldrb w12, [x9, x11]
    cmp w12, #48
    b.lo Lparse_url_scheme_punct
    cmp w12, #57
    b.ls Lparse_url_scheme_next
    cmp w12, #65
    b.lo Lparse_url_scheme_punct
    cmp w12, #90
    b.ls Lparse_url_scheme_next
    cmp w12, #97
    b.lo Lparse_url_scheme_punct
    cmp w12, #122
    b.ls Lparse_url_scheme_next
Lparse_url_scheme_punct:
    cmp w12, #43
    b.eq Lparse_url_scheme_next
    cmp w12, #45
    b.eq Lparse_url_scheme_next
    cmp w12, #46
    b.ne Lparse_url_invalid_scheme
Lparse_url_scheme_next:
    add x11, x11, #1
    b Lparse_url_validate_scheme

Lparse_url_invalid_scheme:
    // A colon before query/fragment may still be PHP's host:port form.
    mov x11, #0
Lparse_url_invalid_delim_scan:
    cmp x11, x2
    b.eq Lparse_url_invalid_delim_done
    ldrb w12, [x9, x11]
    cmp w12, #63
    b.eq Lparse_url_invalid_delim_done
    cmp w12, #35
    b.eq Lparse_url_invalid_delim_done
    add x11, x11, #1
    b Lparse_url_invalid_delim_scan
Lparse_url_invalid_delim_done:
    add x12, x10, #1
    cmp x12, x2
    b.hs Lparse_url_invalid_scheme_slashes
    cmp x10, x11
    b.lo Lparse_url_parse_port
Lparse_url_invalid_scheme_slashes:
    cmp x2, #2
    b.lo Lparse_url_path_zero
    ldrb w11, [x9]
    cmp w11, #47
    b.ne Lparse_url_path_zero
    ldrb w11, [x9, #1]
    cmp w11, #47
    b.ne Lparse_url_path_zero
    mov x15, #2
    b Lparse_url_host

Lparse_url_valid_scheme:
    // Record the scheme slice before deciding which parser state follows it.
    mov x11, #1
    str x11, [sp, #24]
    str x9, [sp, #32]
    str x10, [sp, #40]
    add x11, x10, #1
    cmp x11, x2
    b.eq Lparse_url_finish
    ldrb w12, [x9, x11]
    cmp w12, #47
    b.eq Lparse_url_scheme_slash

    // A short numeric suffix is reinterpreted as a port, as in example.com:80/path.
    mov x12, x11
Lparse_url_scheme_digit_scan:
    cmp x12, x2
    b.eq Lparse_url_scheme_digit_done
    ldrb w13, [x9, x12]
    cmp w13, #48
    b.lo Lparse_url_scheme_digit_done
    cmp w13, #57
    b.hi Lparse_url_scheme_digit_done
    add x12, x12, #1
    b Lparse_url_scheme_digit_scan
Lparse_url_scheme_digit_done:
    cmp x12, x2
    b.eq Lparse_url_scheme_port_length
    ldrb w13, [x9, x12]
    cmp w13, #47
    b.ne Lparse_url_scheme_as_path
Lparse_url_scheme_port_length:
    sub x13, x12, x10
    cmp x13, #7
    b.lo Lparse_url_parse_port
Lparse_url_scheme_as_path:
    mov x15, x11
    b Lparse_url_path

Lparse_url_scheme_slash:
    add x12, x10, #2
    cmp x12, x2
    b.hs Lparse_url_scheme_single_slash
    ldrb w13, [x9, x12]
    cmp w13, #47
    b.ne Lparse_url_scheme_single_slash
    add x15, x10, #3

    // file:///path is a path, not an empty authority.
    cmp x10, #4
    b.ne Lparse_url_host
    ldrb w11, [x9]
    orr w11, w11, #32
    cmp w11, #102
    b.ne Lparse_url_host
    ldrb w11, [x9, #1]
    orr w11, w11, #32
    cmp w11, #105
    b.ne Lparse_url_host
    ldrb w11, [x9, #2]
    orr w11, w11, #32
    cmp w11, #108
    b.ne Lparse_url_host
    ldrb w11, [x9, #3]
    orr w11, w11, #32
    cmp w11, #101
    b.ne Lparse_url_host
    cmp x15, x2
    b.hs Lparse_url_host
    ldrb w11, [x9, x15]
    cmp w11, #47
    b.ne Lparse_url_host
    add x11, x10, #5
    cmp x11, x2
    b.hs Lparse_url_path
    add x12, x10, #5
    ldrb w13, [x9, x12]
    cmp w13, #58
    b.ne Lparse_url_path
    add x15, x10, #4
    b Lparse_url_path

Lparse_url_scheme_single_slash:
    add x15, x10, #1
    b Lparse_url_path

Lparse_url_no_colon:
    cmp x2, #2
    b.lo Lparse_url_path_zero
    ldrb w11, [x9]
    cmp w11, #47
    b.ne Lparse_url_path_zero
    ldrb w11, [x9, #1]
    cmp w11, #47
    b.ne Lparse_url_path_zero
    mov x15, #2
    b Lparse_url_host

Lparse_url_path_zero:
    mov x15, #0
    b Lparse_url_path

Lparse_url_parse_port:
    ldr x11, [sp, #24]
    bic x11, x11, #1
    str x11, [sp, #24]
    ldr x10, [sp, #160]
    add x11, x10, #1
    mov x12, x11
Lparse_url_initial_port_scan:
    cmp x12, x2
    b.eq Lparse_url_initial_port_scanned
    sub x13, x12, x11
    cmp x13, #6
    b.hs Lparse_url_initial_port_scanned
    ldrb w13, [x9, x12]
    cmp w13, #48
    b.lo Lparse_url_initial_port_scanned
    cmp w13, #57
    b.hi Lparse_url_initial_port_scanned
    add x12, x12, #1
    b Lparse_url_initial_port_scan
Lparse_url_initial_port_scanned:
    sub x13, x12, x11
    cbz x13, Lparse_url_initial_port_empty
    cmp x13, #6
    b.hs Lparse_url_initial_port_fallback
    cmp x12, x2
    b.eq Lparse_url_initial_port_value
    ldrb w14, [x9, x12]
    cmp w14, #47
    b.ne Lparse_url_initial_port_fallback
Lparse_url_initial_port_value:
    mov x14, #0
Lparse_url_initial_port_accumulate:
    cmp x11, x12
    b.eq Lparse_url_initial_port_range
    ldrb w15, [x9, x11]
    sub w15, w15, #48
    mov x16, #10
    madd x14, x14, x16, x15
    add x11, x11, #1
    b Lparse_url_initial_port_accumulate
Lparse_url_initial_port_range:
    mov x15, #65535
    cmp x14, x15
    b.hi Lparse_url_invalid
    ldr x15, [sp, #24]
    orr x15, x15, #4
    str x15, [sp, #24]
    str x14, [sp, #64]
    cmp x2, #2
    b.lo Lparse_url_host_zero
    ldrb w11, [x9]
    cmp w11, #47
    b.ne Lparse_url_host_zero
    ldrb w11, [x9, #1]
    cmp w11, #47
    b.ne Lparse_url_host_zero
    mov x15, #2
    b Lparse_url_host
Lparse_url_host_zero:
    mov x15, #0
    b Lparse_url_host

Lparse_url_initial_port_empty:
    cmp x12, x2
    b.eq Lparse_url_invalid
Lparse_url_initial_port_fallback:
    cmp x2, #2
    b.lo Lparse_url_path_zero
    ldrb w11, [x9]
    cmp w11, #47
    b.ne Lparse_url_path_zero
    ldrb w11, [x9, #1]
    cmp w11, #47
    b.ne Lparse_url_path_zero
    mov x15, #2

Lparse_url_host:
    // Find the end of authority at the first '/', '?', or '#'.
    mov x16, x15
Lparse_url_authority_end_scan:
    cmp x16, x2
    b.eq Lparse_url_authority_end_found
    ldrb w11, [x9, x16]
    cmp w11, #47
    b.eq Lparse_url_authority_end_found
    cmp w11, #63
    b.eq Lparse_url_authority_end_found
    cmp w11, #35
    b.eq Lparse_url_authority_end_found
    add x16, x16, #1
    b Lparse_url_authority_end_scan
Lparse_url_authority_end_found:
    str x16, [sp, #168]

    // The last '@' terminates userinfo.
    mov x11, x15
    mov x12, #-1
Lparse_url_at_scan:
    cmp x11, x16
    b.eq Lparse_url_at_done
    ldrb w13, [x9, x11]
    cmp w13, #64
    csel x12, x11, x12, eq
    add x11, x11, #1
    b Lparse_url_at_scan
Lparse_url_at_done:
    cmp x12, #0
    b.lt Lparse_url_after_userinfo
    mov x11, x15
Lparse_url_user_colon_scan:
    cmp x11, x12
    b.eq Lparse_url_user_only
    ldrb w13, [x9, x11]
    cmp w13, #58
    b.eq Lparse_url_user_password
    add x11, x11, #1
    b Lparse_url_user_colon_scan
Lparse_url_user_password:
    ldr x13, [sp, #24]
    orr x13, x13, #24
    str x13, [sp, #24]
    add x14, x9, x15
    str x14, [sp, #72]
    sub x14, x11, x15
    str x14, [sp, #80]
    add x14, x11, #1
    add x14, x9, x14
    str x14, [sp, #88]
    sub x14, x12, x11
    sub x14, x14, #1
    str x14, [sp, #96]
    add x15, x12, #1
    b Lparse_url_after_userinfo
Lparse_url_user_only:
    ldr x13, [sp, #24]
    orr x13, x13, #8
    str x13, [sp, #24]
    add x14, x9, x15
    str x14, [sp, #72]
    sub x14, x12, x15
    str x14, [sp, #80]
    add x15, x12, #1

Lparse_url_after_userinfo:
    // A bracketed IPv6 literal consumes its internal colons as host bytes.
    cmp x15, x16
    b.hs Lparse_url_find_host_colon
    ldrb w11, [x9, x15]
    cmp w11, #91
    b.ne Lparse_url_find_host_colon
    sub x11, x16, #1
    ldrb w12, [x9, x11]
    cmp w12, #93
    b.eq Lparse_url_no_host_colon

Lparse_url_find_host_colon:
    mov x11, x15
    mov x12, #-1
Lparse_url_host_colon_scan:
    cmp x11, x16
    b.eq Lparse_url_host_colon_done
    ldrb w13, [x9, x11]
    cmp w13, #58
    csel x12, x11, x12, eq
    add x11, x11, #1
    b Lparse_url_host_colon_scan
Lparse_url_host_colon_done:
    cmp x12, #0
    b.lt Lparse_url_no_host_colon
    ldr x13, [sp, #24]
    tst x13, #4
    b.ne Lparse_url_host_end_colon
    add x11, x12, #1
    sub x13, x16, x11
    cmp x13, #5
    b.hi Lparse_url_invalid
    cbz x13, Lparse_url_host_end_colon

    // PHP's strtol-style authority parser skips ASCII whitespace before the optional sign.
    mov x14, #0
    mov x17, #0
Lparse_url_authority_port_whitespace:
    cmp x11, x16
    b.eq Lparse_url_authority_port_digits
    ldrb w13, [x9, x11]
    cmp w13, #32
    b.eq Lparse_url_authority_port_whitespace_next
    cmp w13, #9
    b.lo Lparse_url_authority_port_sign
    cmp w13, #13
    b.ls Lparse_url_authority_port_whitespace_next
    b Lparse_url_authority_port_sign
Lparse_url_authority_port_whitespace_next:
    add x11, x11, #1
    b Lparse_url_authority_port_whitespace
Lparse_url_authority_port_sign:
    cmp x11, x16
    b.eq Lparse_url_authority_port_digits
    ldrb w13, [x9, x11]
    cmp w13, #45
    b.ne Lparse_url_authority_port_plus
    mov x17, #1
    add x11, x11, #1
    b Lparse_url_authority_port_digits
Lparse_url_authority_port_plus:
    cmp w13, #43
    b.ne Lparse_url_authority_port_digits
    add x11, x11, #1
Lparse_url_authority_port_digits:
    mov x13, x11
Lparse_url_authority_port_loop:
    cmp x11, x16
    b.eq Lparse_url_authority_port_done
    ldrb w6, [x9, x11]
    cmp w6, #48
    b.lo Lparse_url_authority_port_done
    cmp w6, #57
    b.hi Lparse_url_authority_port_done
    sub w6, w6, #48
    mov x7, #10
    madd x14, x14, x7, x6
    add x11, x11, #1
    b Lparse_url_authority_port_loop
Lparse_url_authority_port_done:
    cmp x11, x13
    b.eq Lparse_url_invalid
    cbz x17, Lparse_url_authority_port_range
    neg x14, x14
Lparse_url_authority_port_range:
    cmp x14, #0
    b.lt Lparse_url_invalid
    mov x13, #65535
    cmp x14, x13
    b.hi Lparse_url_invalid
    ldr x13, [sp, #24]
    orr x13, x13, #4
    str x13, [sp, #24]
    str x14, [sp, #64]
Lparse_url_host_end_colon:
    mov x17, x12
    b Lparse_url_store_host
Lparse_url_no_host_colon:
    mov x17, x16
Lparse_url_store_host:
    cmp x17, x15
    b.ls Lparse_url_invalid
    ldr x11, [sp, #24]
    orr x11, x11, #2
    str x11, [sp, #24]
    add x11, x9, x15
    str x11, [sp, #48]
    sub x11, x17, x15
    str x11, [sp, #56]
    cmp x16, x2
    b.eq Lparse_url_finish
    mov x15, x16

Lparse_url_path:
    mov x16, x2
    mov x11, x15
Lparse_url_fragment_scan:
    cmp x11, x16
    b.eq Lparse_url_fragment_done
    ldrb w12, [x9, x11]
    cmp w12, #35
    b.eq Lparse_url_fragment_found
    add x11, x11, #1
    b Lparse_url_fragment_scan
Lparse_url_fragment_found:
    ldr x12, [sp, #24]
    orr x12, x12, #128
    str x12, [sp, #24]
    add x12, x11, #1
    add x13, x9, x12
    str x13, [sp, #136]
    sub x13, x16, x12
    str x13, [sp, #144]
    mov x16, x11
Lparse_url_fragment_done:
    mov x11, x15
Lparse_url_query_scan:
    cmp x11, x16
    b.eq Lparse_url_query_done
    ldrb w12, [x9, x11]
    cmp w12, #63
    b.eq Lparse_url_query_found
    add x11, x11, #1
    b Lparse_url_query_scan
Lparse_url_query_found:
    ldr x12, [sp, #24]
    orr x12, x12, #64
    str x12, [sp, #24]
    add x12, x11, #1
    add x13, x9, x12
    str x13, [sp, #120]
    sub x13, x16, x12
    str x13, [sp, #128]
    mov x16, x11
Lparse_url_query_done:
    cmp x15, x16
    b.lo Lparse_url_store_path
    cmp x15, x2
    b.ne Lparse_url_finish
Lparse_url_store_path:
    ldr x11, [sp, #24]
    orr x11, x11, #32
    str x11, [sp, #24]
    add x11, x9, x15
    str x11, [sp, #104]
    sub x11, x16, x15
    str x11, [sp, #112]

Lparse_url_finish:
    ldr x11, [sp, #16]
    cmp x11, #7
    b.gt Lparse_url_component_error
    cmp x11, #0
    b.lt Lparse_url_array
    mov x12, #1
    lsl x12, x12, x11
    ldr x13, [sp, #24]
    tst x13, x12
    b.eq Lparse_url_null
    cmp x11, #2
    b.eq Lparse_url_component_port
    cmp x11, #0
    b.eq Lparse_url_component_scheme
    cmp x11, #1
    b.eq Lparse_url_component_host
    cmp x11, #3
    b.eq Lparse_url_component_user
    cmp x11, #4
    b.eq Lparse_url_component_pass
    cmp x11, #5
    b.eq Lparse_url_component_path
    cmp x11, #6
    b.eq Lparse_url_component_query
    b Lparse_url_component_fragment

Lparse_url_component_scheme:
    ldp x1, x2, [sp, #32]
    b Lparse_url_component_string
Lparse_url_component_host:
    ldp x1, x2, [sp, #48]
    b Lparse_url_component_string
Lparse_url_component_user:
    ldp x1, x2, [sp, #72]
    b Lparse_url_component_string
Lparse_url_component_pass:
    ldp x1, x2, [sp, #88]
    b Lparse_url_component_string
Lparse_url_component_path:
    ldp x1, x2, [sp, #104]
    b Lparse_url_component_string
Lparse_url_component_query:
    ldp x1, x2, [sp, #120]
    b Lparse_url_component_string
Lparse_url_component_fragment:
    ldp x1, x2, [sp, #136]
Lparse_url_component_string:
    bl Lparse_url_copy_component
    stp x1, x2, [sp, #176]
    mov x0, #24
    bl __rt_heap_alloc
    mov x9, #5
    str x9, [x0, #-8]
    mov x9, #1
    str x9, [x0]
    ldp x10, x11, [sp, #176]
    stp x10, x11, [x0, #8]
    b Lparse_url_return

Lparse_url_component_port:
    mov x0, #0
    ldr x1, [sp, #64]
    mov x2, #0
    bl __rt_mixed_from_value
    b Lparse_url_return

Lparse_url_null:
    mov x0, #8
    mov x1, #0
    mov x2, #0
    bl __rt_mixed_from_value
    b Lparse_url_return

Lparse_url_invalid:
    ldr x9, [sp, #192]
    cbz x9, Lparse_url_invalid_build
    mov x0, #0
    mov x1, #0
    b Lparse_url_return
Lparse_url_invalid_build:
    mov x0, #3
    mov x1, #0
    mov x2, #0
    bl __rt_mixed_from_value
    b Lparse_url_return

Lparse_url_component_error:
    mov x0, x11
    ldp x29, x30, [sp, #208]
    add sp, sp, #224
    b __rt_parse_url_throw_component

Lparse_url_array:
    ldr x9, [sp, #192]
    cbnz x9, Lparse_url_array_count
    mov x0, #16
    mov x1, #7
    bl __rt_hash_new
    str x0, [sp, #184]
    ldr x9, [sp, #24]
    tst x9, #1
    b.eq Lparse_url_array_host
    ldr x0, [sp, #184]
    mov x1, #0
    ldp x2, x3, [sp, #32]
    bl Lparse_url_insert_string
    str x0, [sp, #184]
Lparse_url_array_host:
    ldr x9, [sp, #24]
    tst x9, #2
    b.eq Lparse_url_array_port
    ldr x0, [sp, #184]
    mov x1, #1
    ldp x2, x3, [sp, #48]
    bl Lparse_url_insert_string
    str x0, [sp, #184]
Lparse_url_array_port:
    ldr x9, [sp, #24]
    tst x9, #4
    b.eq Lparse_url_array_user
    ldr x0, [sp, #184]
    mov x1, #2
    ldr x2, [sp, #64]
    bl Lparse_url_insert_port
    str x0, [sp, #184]
Lparse_url_array_user:
    ldr x9, [sp, #24]
    tst x9, #8
    b.eq Lparse_url_array_pass
    ldr x0, [sp, #184]
    mov x1, #3
    ldp x2, x3, [sp, #72]
    bl Lparse_url_insert_string
    str x0, [sp, #184]
Lparse_url_array_pass:
    ldr x9, [sp, #24]
    tst x9, #16
    b.eq Lparse_url_array_path
    ldr x0, [sp, #184]
    mov x1, #4
    ldp x2, x3, [sp, #88]
    bl Lparse_url_insert_string
    str x0, [sp, #184]
Lparse_url_array_path:
    ldr x9, [sp, #24]
    tst x9, #32
    b.eq Lparse_url_array_query
    ldr x0, [sp, #184]
    mov x1, #5
    ldp x2, x3, [sp, #104]
    bl Lparse_url_insert_string
    str x0, [sp, #184]
Lparse_url_array_query:
    ldr x9, [sp, #24]
    tst x9, #64
    b.eq Lparse_url_array_fragment
    ldr x0, [sp, #184]
    mov x1, #6
    ldp x2, x3, [sp, #120]
    bl Lparse_url_insert_string
    str x0, [sp, #184]
Lparse_url_array_fragment:
    ldr x9, [sp, #24]
    tst x9, #128
    b.eq Lparse_url_box_array
    ldr x0, [sp, #184]
    mov x1, #7
    ldp x2, x3, [sp, #136]
    bl Lparse_url_insert_string
    str x0, [sp, #184]

Lparse_url_box_array:
    mov x0, #24
    bl __rt_heap_alloc
    mov x9, #5
    str x9, [x0, #-8]
    mov x9, #5
    str x9, [x0]
    ldr x9, [sp, #184]
    str x9, [x0, #8]
    str xzr, [x0, #16]

Lparse_url_return:
    ldp x29, x30, [sp, #208]
    add sp, sp, #224
    ret

Lparse_url_array_count:
    ldr x9, [sp, #24]
    and x9, x9, #0xff
    fmov d0, x9
    cnt v0.8b, v0.8b
    addv b0, v0.8b
    umov w0, v0.b[0]
    mov x1, #1
    b Lparse_url_return

// Copy a component into owned storage and replace PHP-disallowed control bytes.
Lparse_url_copy_component:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    add x29, sp, #16
    bl __rt_str_persist
    mov x9, #0
Lparse_url_copy_scan:
    cmp x9, x2
    b.eq Lparse_url_copy_done
    ldrb w10, [x1, x9]
    cmp w10, #32
    b.lo Lparse_url_copy_replace
    cmp w10, #127
    b.ne Lparse_url_copy_next
Lparse_url_copy_replace:
    mov w10, #95
    strb w10, [x1, x9]
Lparse_url_copy_next:
    add x9, x9, #1
    b Lparse_url_copy_scan
Lparse_url_copy_done:
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret

// Insert one owned string component into the Mixed-valued result hash.
Lparse_url_insert_string:
    sub sp, sp, #64
    stp x29, x30, [sp, #48]
    add x29, sp, #48
    stp x0, x1, [sp, #0]
    mov x1, x2
    mov x2, x3
    bl Lparse_url_copy_component
    stp x1, x2, [sp, #16]
    ldr x0, [sp, #8]
    bl __rt_parse_url_key_address
    ldp x3, x4, [sp, #16]
    mov x5, #1
    ldr x0, [sp, #0]
    bl __rt_hash_set
    ldp x29, x30, [sp, #48]
    add sp, sp, #64
    ret

// Insert the integer port component into the Mixed-valued result hash.
Lparse_url_insert_port:
    sub sp, sp, #48
    stp x29, x30, [sp, #32]
    add x29, sp, #32
    stp x0, x2, [sp, #0]
    mov x0, x1
    bl __rt_parse_url_key_address
    ldr x3, [sp, #8]
    mov x4, #0
    mov x5, #0
    ldr x0, [sp, #0]
    bl __rt_hash_set
    ldp x29, x30, [sp, #32]
    add sp, sp, #48
    ret
