.globl __rt_parse_url_common_x86
__rt_parse_url_common_x86:
    push rbp
    mov rbp, rsp
    sub rsp, 208
    mov QWORD PTR [rbp - 8], rax
    mov QWORD PTR [rbp - 16], rdx
    mov QWORD PTR [rbp - 24], rdi
    mov QWORD PTR [rbp - 200], r11
    mov QWORD PTR [rbp - 32], 0
    mov QWORD PTR [rbp - 40], 0
    mov QWORD PTR [rbp - 48], 0
    mov QWORD PTR [rbp - 56], 0
    mov QWORD PTR [rbp - 64], 0
    mov QWORD PTR [rbp - 72], 0
    mov QWORD PTR [rbp - 80], 0
    mov QWORD PTR [rbp - 88], 0
    mov QWORD PTR [rbp - 96], 0
    mov QWORD PTR [rbp - 104], 0
    mov QWORD PTR [rbp - 112], 0
    mov QWORD PTR [rbp - 120], 0
    mov QWORD PTR [rbp - 128], 0
    mov QWORD PTR [rbp - 136], 0
    mov QWORD PTR [rbp - 144], 0
    mov QWORD PTR [rbp - 152], 0
    mov QWORD PTR [rbp - 160], 0
    mov QWORD PTR [rbp - 168], 0
    mov QWORD PTR [rbp - 176], 0
    mov QWORD PTR [rbp - 184], 0
    mov QWORD PTR [rbp - 192], 0

    # Locate the first colon, if any.
    mov r8, rax
    xor r9d, r9d
Lparse_url_find_colon_x86:
    cmp r9, rdx
    je Lparse_url_no_colon_x86
    movzx r10d, BYTE PTR [r8 + r9]
    cmp r10b, 58
    je Lparse_url_colon_found_x86
    add r9, 1
    jmp Lparse_url_find_colon_x86

Lparse_url_colon_found_x86:
    mov QWORD PTR [rbp - 160], r9
    test r9, r9
    jz Lparse_url_parse_port_x86

    xor ecx, ecx
Lparse_url_validate_scheme_x86:
    cmp rcx, r9
    je Lparse_url_valid_scheme_x86
    movzx r10d, BYTE PTR [r8 + rcx]
    cmp r10b, 48
    jb Lparse_url_scheme_punct_x86
    cmp r10b, 57
    jbe Lparse_url_scheme_next_x86
    cmp r10b, 65
    jb Lparse_url_scheme_punct_x86
    cmp r10b, 90
    jbe Lparse_url_scheme_next_x86
    cmp r10b, 97
    jb Lparse_url_scheme_punct_x86
    cmp r10b, 122
    jbe Lparse_url_scheme_next_x86
Lparse_url_scheme_punct_x86:
    cmp r10b, 43
    je Lparse_url_scheme_next_x86
    cmp r10b, 45
    je Lparse_url_scheme_next_x86
    cmp r10b, 46
    jne Lparse_url_invalid_scheme_x86
Lparse_url_scheme_next_x86:
    add rcx, 1
    jmp Lparse_url_validate_scheme_x86

Lparse_url_invalid_scheme_x86:
    xor ecx, ecx
Lparse_url_invalid_delim_scan_x86:
    cmp rcx, rdx
    je Lparse_url_invalid_delim_done_x86
    movzx r10d, BYTE PTR [r8 + rcx]
    cmp r10b, 63
    je Lparse_url_invalid_delim_done_x86
    cmp r10b, 35
    je Lparse_url_invalid_delim_done_x86
    add rcx, 1
    jmp Lparse_url_invalid_delim_scan_x86
Lparse_url_invalid_delim_done_x86:
    lea r10, [r9 + 1]
    cmp r10, rdx
    jae Lparse_url_invalid_scheme_slashes_x86
    cmp r9, rcx
    jb Lparse_url_parse_port_x86
Lparse_url_invalid_scheme_slashes_x86:
    cmp rdx, 2
    jb Lparse_url_path_zero_x86
    cmp BYTE PTR [r8], 47
    jne Lparse_url_path_zero_x86
    cmp BYTE PTR [r8 + 1], 47
    jne Lparse_url_path_zero_x86
    mov rdi, 2
    jmp Lparse_url_host_x86

Lparse_url_valid_scheme_x86:
    or QWORD PTR [rbp - 32], 1
    mov QWORD PTR [rbp - 40], r8
    mov QWORD PTR [rbp - 48], r9
    lea rcx, [r9 + 1]
    cmp rcx, rdx
    je Lparse_url_finish_x86
    cmp BYTE PTR [r8 + rcx], 47
    je Lparse_url_scheme_slash_x86

    mov r10, rcx
Lparse_url_scheme_digit_scan_x86:
    cmp r10, rdx
    je Lparse_url_scheme_digit_done_x86
    movzx r11d, BYTE PTR [r8 + r10]
    cmp r11b, 48
    jb Lparse_url_scheme_digit_done_x86
    cmp r11b, 57
    ja Lparse_url_scheme_digit_done_x86
    add r10, 1
    jmp Lparse_url_scheme_digit_scan_x86
Lparse_url_scheme_digit_done_x86:
    cmp r10, rdx
    je Lparse_url_scheme_port_length_x86
    cmp BYTE PTR [r8 + r10], 47
    jne Lparse_url_scheme_as_path_x86
Lparse_url_scheme_port_length_x86:
    mov r11, r10
    sub r11, r9
    cmp r11, 7
    jb Lparse_url_parse_port_x86
Lparse_url_scheme_as_path_x86:
    mov rdi, rcx
    jmp Lparse_url_path_x86

Lparse_url_scheme_slash_x86:
    lea r10, [r9 + 2]
    cmp r10, rdx
    jae Lparse_url_scheme_single_slash_x86
    cmp BYTE PTR [r8 + r10], 47
    jne Lparse_url_scheme_single_slash_x86
    lea rdi, [r9 + 3]

    cmp r9, 4
    jne Lparse_url_host_x86
    movzx ecx, BYTE PTR [r8]
    or ecx, 32
    cmp cl, 102
    jne Lparse_url_host_x86
    movzx ecx, BYTE PTR [r8 + 1]
    or ecx, 32
    cmp cl, 105
    jne Lparse_url_host_x86
    movzx ecx, BYTE PTR [r8 + 2]
    or ecx, 32
    cmp cl, 108
    jne Lparse_url_host_x86
    movzx ecx, BYTE PTR [r8 + 3]
    or ecx, 32
    cmp cl, 101
    jne Lparse_url_host_x86
    cmp rdi, rdx
    jae Lparse_url_host_x86
    cmp BYTE PTR [r8 + rdi], 47
    jne Lparse_url_host_x86
    lea rcx, [r9 + 5]
    cmp rcx, rdx
    jae Lparse_url_path_x86
    cmp BYTE PTR [r8 + rcx], 58
    jne Lparse_url_path_x86
    lea rdi, [r9 + 4]
    jmp Lparse_url_path_x86

Lparse_url_scheme_single_slash_x86:
    lea rdi, [r9 + 1]
    jmp Lparse_url_path_x86

Lparse_url_no_colon_x86:
    cmp rdx, 2
    jb Lparse_url_path_zero_x86
    cmp BYTE PTR [r8], 47
    jne Lparse_url_path_zero_x86
    cmp BYTE PTR [r8 + 1], 47
    jne Lparse_url_path_zero_x86
    mov rdi, 2
    jmp Lparse_url_host_x86

Lparse_url_path_zero_x86:
    xor edi, edi
    jmp Lparse_url_path_x86

Lparse_url_parse_port_x86:
    and QWORD PTR [rbp - 32], -2
    mov r9, QWORD PTR [rbp - 160]
    lea rcx, [r9 + 1]
    mov r10, rcx
Lparse_url_initial_port_scan_x86:
    cmp r10, rdx
    je Lparse_url_initial_port_scanned_x86
    mov r11, r10
    sub r11, rcx
    cmp r11, 6
    jae Lparse_url_initial_port_scanned_x86
    movzx r11d, BYTE PTR [r8 + r10]
    cmp r11b, 48
    jb Lparse_url_initial_port_scanned_x86
    cmp r11b, 57
    ja Lparse_url_initial_port_scanned_x86
    add r10, 1
    jmp Lparse_url_initial_port_scan_x86
Lparse_url_initial_port_scanned_x86:
    mov r11, r10
    sub r11, rcx
    test r11, r11
    jz Lparse_url_initial_port_empty_x86
    cmp r11, 6
    jae Lparse_url_initial_port_fallback_x86
    cmp r10, rdx
    je Lparse_url_initial_port_value_x86
    cmp BYTE PTR [r8 + r10], 47
    jne Lparse_url_initial_port_fallback_x86
Lparse_url_initial_port_value_x86:
    xor r11d, r11d
Lparse_url_initial_port_accumulate_x86:
    cmp rcx, r10
    je Lparse_url_initial_port_range_x86
    movzx eax, BYTE PTR [r8 + rcx]
    sub eax, 48
    imul r11, r11, 10
    add r11, rax
    add rcx, 1
    jmp Lparse_url_initial_port_accumulate_x86
Lparse_url_initial_port_range_x86:
    cmp r11, 65535
    ja Lparse_url_invalid_x86
    or QWORD PTR [rbp - 32], 4
    mov QWORD PTR [rbp - 72], r11
    cmp rdx, 2
    jb Lparse_url_host_zero_x86
    cmp BYTE PTR [r8], 47
    jne Lparse_url_host_zero_x86
    cmp BYTE PTR [r8 + 1], 47
    jne Lparse_url_host_zero_x86
    mov rdi, 2
    jmp Lparse_url_host_x86
Lparse_url_host_zero_x86:
    xor edi, edi
    jmp Lparse_url_host_x86

Lparse_url_initial_port_empty_x86:
    cmp r10, rdx
    je Lparse_url_invalid_x86
Lparse_url_initial_port_fallback_x86:
    cmp rdx, 2
    jb Lparse_url_path_zero_x86
    cmp BYTE PTR [r8], 47
    jne Lparse_url_path_zero_x86
    cmp BYTE PTR [r8 + 1], 47
    jne Lparse_url_path_zero_x86
    mov rdi, 2

Lparse_url_host_x86:
    mov rcx, rdi
Lparse_url_authority_end_scan_x86:
    cmp rcx, rdx
    je Lparse_url_authority_end_found_x86
    movzx r10d, BYTE PTR [r8 + rcx]
    cmp r10b, 47
    je Lparse_url_authority_end_found_x86
    cmp r10b, 63
    je Lparse_url_authority_end_found_x86
    cmp r10b, 35
    je Lparse_url_authority_end_found_x86
    add rcx, 1
    jmp Lparse_url_authority_end_scan_x86
Lparse_url_authority_end_found_x86:
    mov QWORD PTR [rbp - 168], rcx

    mov r9, rdi
    mov r10, -1
Lparse_url_at_scan_x86:
    cmp r9, rcx
    je Lparse_url_at_done_x86
    cmp BYTE PTR [r8 + r9], 64
    cmove r10, r9
    add r9, 1
    jmp Lparse_url_at_scan_x86
Lparse_url_at_done_x86:
    test r10, r10
    js Lparse_url_after_userinfo_x86
    mov r9, rdi
Lparse_url_user_colon_scan_x86:
    cmp r9, r10
    je Lparse_url_user_only_x86
    cmp BYTE PTR [r8 + r9], 58
    je Lparse_url_user_password_x86
    add r9, 1
    jmp Lparse_url_user_colon_scan_x86
Lparse_url_user_password_x86:
    or QWORD PTR [rbp - 32], 24
    lea r11, [r8 + rdi]
    mov QWORD PTR [rbp - 80], r11
    mov r11, r9
    sub r11, rdi
    mov QWORD PTR [rbp - 88], r11
    lea r11, [r9 + 1]
    add r11, r8
    mov QWORD PTR [rbp - 96], r11
    mov r11, r10
    sub r11, r9
    sub r11, 1
    mov QWORD PTR [rbp - 104], r11
    lea rdi, [r10 + 1]
    jmp Lparse_url_after_userinfo_x86
Lparse_url_user_only_x86:
    or QWORD PTR [rbp - 32], 8
    lea r11, [r8 + rdi]
    mov QWORD PTR [rbp - 80], r11
    mov r11, r10
    sub r11, rdi
    mov QWORD PTR [rbp - 88], r11
    lea rdi, [r10 + 1]

Lparse_url_after_userinfo_x86:
    cmp rdi, rcx
    jae Lparse_url_find_host_colon_x86
    cmp BYTE PTR [r8 + rdi], 91
    jne Lparse_url_find_host_colon_x86
    lea r9, [rcx - 1]
    cmp BYTE PTR [r8 + r9], 93
    je Lparse_url_no_host_colon_x86

Lparse_url_find_host_colon_x86:
    mov r9, rdi
    mov r10, -1
Lparse_url_host_colon_scan_x86:
    cmp r9, rcx
    je Lparse_url_host_colon_done_x86
    cmp BYTE PTR [r8 + r9], 58
    cmove r10, r9
    add r9, 1
    jmp Lparse_url_host_colon_scan_x86
Lparse_url_host_colon_done_x86:
    test r10, r10
    js Lparse_url_no_host_colon_x86
    test QWORD PTR [rbp - 32], 4
    jnz Lparse_url_host_end_colon_x86
    lea r9, [r10 + 1]
    mov r11, rcx
    sub r11, r9
    cmp r11, 5
    ja Lparse_url_invalid_x86
    test r11, r11
    jz Lparse_url_host_end_colon_x86

    xor esi, esi
    xor r11d, r11d
Lparse_url_authority_port_whitespace_x86:
    cmp r9, rcx
    je Lparse_url_authority_port_digits_x86
    movzx eax, BYTE PTR [r8 + r9]
    cmp al, 32
    je Lparse_url_authority_port_whitespace_next_x86
    cmp al, 9
    jb Lparse_url_authority_port_sign_x86
    cmp al, 13
    jbe Lparse_url_authority_port_whitespace_next_x86
    jmp Lparse_url_authority_port_sign_x86
Lparse_url_authority_port_whitespace_next_x86:
    add r9, 1
    jmp Lparse_url_authority_port_whitespace_x86
Lparse_url_authority_port_sign_x86:
    cmp r9, rcx
    je Lparse_url_authority_port_digits_x86
    movzx eax, BYTE PTR [r8 + r9]
    cmp al, 45
    jne Lparse_url_authority_port_plus_x86
    mov r11, 1
    add r9, 1
    jmp Lparse_url_authority_port_digits_x86
Lparse_url_authority_port_plus_x86:
    cmp al, 43
    jne Lparse_url_authority_port_digits_x86
    add r9, 1
Lparse_url_authority_port_digits_x86:
    mov rax, r9
Lparse_url_authority_port_loop_x86:
    cmp r9, rcx
    je Lparse_url_authority_port_done_x86
    movzx edx, BYTE PTR [r8 + r9]
    cmp dl, 48
    jb Lparse_url_authority_port_done_x86
    cmp dl, 57
    ja Lparse_url_authority_port_done_x86
    sub edx, 48
    imul rsi, rsi, 10
    add rsi, rdx
    add r9, 1
    jmp Lparse_url_authority_port_loop_x86
Lparse_url_authority_port_done_x86:
    cmp r9, rax
    je Lparse_url_invalid_x86
    test r11, r11
    jz Lparse_url_authority_port_range_x86
    neg rsi
Lparse_url_authority_port_range_x86:
    test rsi, rsi
    js Lparse_url_invalid_x86
    cmp rsi, 65535
    ja Lparse_url_invalid_x86
    or QWORD PTR [rbp - 32], 4
    mov QWORD PTR [rbp - 72], rsi
Lparse_url_host_end_colon_x86:
    mov r11, r10
    jmp Lparse_url_store_host_x86
Lparse_url_no_host_colon_x86:
    mov r11, rcx
Lparse_url_store_host_x86:
    cmp r11, rdi
    jbe Lparse_url_invalid_x86
    or QWORD PTR [rbp - 32], 2
    lea r9, [r8 + rdi]
    mov QWORD PTR [rbp - 56], r9
    sub r11, rdi
    mov QWORD PTR [rbp - 64], r11
    cmp rcx, QWORD PTR [rbp - 16]
    je Lparse_url_finish_x86
    mov rdi, rcx

Lparse_url_path_x86:
    mov rdx, QWORD PTR [rbp - 16]
    mov rcx, rdx
    mov r9, rdi
Lparse_url_fragment_scan_x86:
    cmp r9, rcx
    je Lparse_url_fragment_done_x86
    cmp BYTE PTR [r8 + r9], 35
    je Lparse_url_fragment_found_x86
    add r9, 1
    jmp Lparse_url_fragment_scan_x86
Lparse_url_fragment_found_x86:
    or QWORD PTR [rbp - 32], 128
    lea r10, [r9 + 1]
    lea r11, [r8 + r10]
    mov QWORD PTR [rbp - 144], r11
    mov r11, rcx
    sub r11, r10
    mov QWORD PTR [rbp - 152], r11
    mov rcx, r9
Lparse_url_fragment_done_x86:
    mov r9, rdi
Lparse_url_query_scan_x86:
    cmp r9, rcx
    je Lparse_url_query_done_x86
    cmp BYTE PTR [r8 + r9], 63
    je Lparse_url_query_found_x86
    add r9, 1
    jmp Lparse_url_query_scan_x86
Lparse_url_query_found_x86:
    or QWORD PTR [rbp - 32], 64
    lea r10, [r9 + 1]
    lea r11, [r8 + r10]
    mov QWORD PTR [rbp - 128], r11
    mov r11, rcx
    sub r11, r10
    mov QWORD PTR [rbp - 136], r11
    mov rcx, r9
Lparse_url_query_done_x86:
    cmp rdi, rcx
    jb Lparse_url_store_path_x86
    cmp rdi, rdx
    jne Lparse_url_finish_x86
Lparse_url_store_path_x86:
    or QWORD PTR [rbp - 32], 32
    lea r9, [r8 + rdi]
    mov QWORD PTR [rbp - 112], r9
    sub rcx, rdi
    mov QWORD PTR [rbp - 120], rcx

Lparse_url_finish_x86:
    mov rdi, QWORD PTR [rbp - 24]
    cmp rdi, 7
    jg Lparse_url_component_error_x86
    test rdi, rdi
    js Lparse_url_array_x86
    mov rcx, rdi
    mov r9, 1
    shl r9, cl
    test QWORD PTR [rbp - 32], r9
    jz Lparse_url_null_x86
    cmp rdi, 2
    je Lparse_url_component_port_x86
    cmp rdi, 0
    je Lparse_url_component_scheme_x86
    cmp rdi, 1
    je Lparse_url_component_host_x86
    cmp rdi, 3
    je Lparse_url_component_user_x86
    cmp rdi, 4
    je Lparse_url_component_pass_x86
    cmp rdi, 5
    je Lparse_url_component_path_x86
    cmp rdi, 6
    je Lparse_url_component_query_x86
    jmp Lparse_url_component_fragment_x86

Lparse_url_component_scheme_x86:
    mov rax, QWORD PTR [rbp - 40]
    mov rdx, QWORD PTR [rbp - 48]
    jmp Lparse_url_component_string_x86
Lparse_url_component_host_x86:
    mov rax, QWORD PTR [rbp - 56]
    mov rdx, QWORD PTR [rbp - 64]
    jmp Lparse_url_component_string_x86
Lparse_url_component_user_x86:
    mov rax, QWORD PTR [rbp - 80]
    mov rdx, QWORD PTR [rbp - 88]
    jmp Lparse_url_component_string_x86
Lparse_url_component_pass_x86:
    mov rax, QWORD PTR [rbp - 96]
    mov rdx, QWORD PTR [rbp - 104]
    jmp Lparse_url_component_string_x86
Lparse_url_component_path_x86:
    mov rax, QWORD PTR [rbp - 112]
    mov rdx, QWORD PTR [rbp - 120]
    jmp Lparse_url_component_string_x86
Lparse_url_component_query_x86:
    mov rax, QWORD PTR [rbp - 128]
    mov rdx, QWORD PTR [rbp - 136]
    jmp Lparse_url_component_string_x86
Lparse_url_component_fragment_x86:
    mov rax, QWORD PTR [rbp - 144]
    mov rdx, QWORD PTR [rbp - 152]
Lparse_url_component_string_x86:
    call Lparse_url_copy_component_x86
    mov QWORD PTR [rbp - 176], rax
    mov QWORD PTR [rbp - 184], rdx
    mov rax, 24
    call __rt_heap_alloc
    mov r10, {{MIXED_HEAP_KIND}}
    mov QWORD PTR [rax - 8], r10
    mov QWORD PTR [rax], 1
    mov r10, QWORD PTR [rbp - 176]
    mov QWORD PTR [rax + 8], r10
    mov r10, QWORD PTR [rbp - 184]
    mov QWORD PTR [rax + 16], r10
    jmp Lparse_url_return_x86

Lparse_url_component_port_x86:
    xor eax, eax
    mov rdi, QWORD PTR [rbp - 72]
    xor esi, esi
    call __rt_mixed_from_value
    jmp Lparse_url_return_x86

Lparse_url_null_x86:
    mov rax, 8
    xor edi, edi
    xor esi, esi
    call __rt_mixed_from_value
    jmp Lparse_url_return_x86

Lparse_url_invalid_x86:
    cmp QWORD PTR [rbp - 200], 0
    je Lparse_url_invalid_build_x86
    xor eax, eax
    xor edx, edx
    jmp Lparse_url_return_x86
Lparse_url_invalid_build_x86:
    mov rax, 3
    xor edi, edi
    xor esi, esi
    call __rt_mixed_from_value
    jmp Lparse_url_return_x86

Lparse_url_component_error_x86:
    mov rax, rdi
    mov rsp, rbp
    pop rbp
    jmp __rt_parse_url_throw_component

Lparse_url_array_x86:
    cmp QWORD PTR [rbp - 200], 0
    jne Lparse_url_count_x86
    mov rdi, 16
    mov rsi, 7
    call __rt_hash_new
    mov QWORD PTR [rbp - 192], rax

    test QWORD PTR [rbp - 32], 1
    jz Lparse_url_array_host_x86
    mov rax, QWORD PTR [rbp - 192]
    xor edi, edi
    mov rsi, QWORD PTR [rbp - 40]
    mov rdx, QWORD PTR [rbp - 48]
    call Lparse_url_insert_string_x86
    mov QWORD PTR [rbp - 192], rax
Lparse_url_array_host_x86:
    test QWORD PTR [rbp - 32], 2
    jz Lparse_url_array_port_x86
    mov rax, QWORD PTR [rbp - 192]
    mov rdi, 1
    mov rsi, QWORD PTR [rbp - 56]
    mov rdx, QWORD PTR [rbp - 64]
    call Lparse_url_insert_string_x86
    mov QWORD PTR [rbp - 192], rax
Lparse_url_array_port_x86:
    test QWORD PTR [rbp - 32], 4
    jz Lparse_url_array_user_x86
    mov rax, QWORD PTR [rbp - 192]
    mov rdi, 2
    mov rsi, QWORD PTR [rbp - 72]
    call Lparse_url_insert_port_x86
    mov QWORD PTR [rbp - 192], rax
Lparse_url_array_user_x86:
    test QWORD PTR [rbp - 32], 8
    jz Lparse_url_array_pass_x86
    mov rax, QWORD PTR [rbp - 192]
    mov rdi, 3
    mov rsi, QWORD PTR [rbp - 80]
    mov rdx, QWORD PTR [rbp - 88]
    call Lparse_url_insert_string_x86
    mov QWORD PTR [rbp - 192], rax
Lparse_url_array_pass_x86:
    test QWORD PTR [rbp - 32], 16
    jz Lparse_url_array_path_x86
    mov rax, QWORD PTR [rbp - 192]
    mov rdi, 4
    mov rsi, QWORD PTR [rbp - 96]
    mov rdx, QWORD PTR [rbp - 104]
    call Lparse_url_insert_string_x86
    mov QWORD PTR [rbp - 192], rax
Lparse_url_array_path_x86:
    test QWORD PTR [rbp - 32], 32
    jz Lparse_url_array_query_x86
    mov rax, QWORD PTR [rbp - 192]
    mov rdi, 5
    mov rsi, QWORD PTR [rbp - 112]
    mov rdx, QWORD PTR [rbp - 120]
    call Lparse_url_insert_string_x86
    mov QWORD PTR [rbp - 192], rax
Lparse_url_array_query_x86:
    test QWORD PTR [rbp - 32], 64
    jz Lparse_url_array_fragment_x86
    mov rax, QWORD PTR [rbp - 192]
    mov rdi, 6
    mov rsi, QWORD PTR [rbp - 128]
    mov rdx, QWORD PTR [rbp - 136]
    call Lparse_url_insert_string_x86
    mov QWORD PTR [rbp - 192], rax
Lparse_url_array_fragment_x86:
    test QWORD PTR [rbp - 32], 128
    jz Lparse_url_box_array_x86
    mov rax, QWORD PTR [rbp - 192]
    mov rdi, 7
    mov rsi, QWORD PTR [rbp - 144]
    mov rdx, QWORD PTR [rbp - 152]
    call Lparse_url_insert_string_x86
    mov QWORD PTR [rbp - 192], rax

Lparse_url_box_array_x86:
    mov rax, 24
    call __rt_heap_alloc
    mov r10, {{MIXED_HEAP_KIND}}
    mov QWORD PTR [rax - 8], r10
    mov QWORD PTR [rax], 5
    mov r10, QWORD PTR [rbp - 192]
    mov QWORD PTR [rax + 8], r10
    mov QWORD PTR [rax + 16], 0

Lparse_url_return_x86:
    mov rsp, rbp
    pop rbp
    ret

Lparse_url_count_x86:
    mov rax, QWORD PTR [rbp - 32]
    and eax, 0xff
    xor ecx, ecx
Lparse_url_count_loop_x86:
    test rax, rax
    jz Lparse_url_count_done_x86
    mov rdx, rax
    and edx, 1
    add rcx, rdx
    shr rax, 1
    jmp Lparse_url_count_loop_x86
Lparse_url_count_done_x86:
    mov rax, rcx
    mov rdx, 1
    jmp Lparse_url_return_x86

# Copy a component into owned storage and replace PHP-disallowed control bytes.
Lparse_url_copy_component_x86:
    push rbp
    mov rbp, rsp
    call __rt_str_persist
    xor r8d, r8d
Lparse_url_copy_scan_x86:
    cmp r8, rdx
    je Lparse_url_copy_done_x86
    movzx r9d, BYTE PTR [rax + r8]
    cmp r9b, 32
    jb Lparse_url_copy_replace_x86
    cmp r9b, 127
    jne Lparse_url_copy_next_x86
Lparse_url_copy_replace_x86:
    mov BYTE PTR [rax + r8], 95
Lparse_url_copy_next_x86:
    add r8, 1
    jmp Lparse_url_copy_scan_x86
Lparse_url_copy_done_x86:
    pop rbp
    ret

# Insert one owned string component into the Mixed-valued result hash.
Lparse_url_insert_string_x86:
    push rbp
    mov rbp, rsp
    sub rsp, 48
    mov QWORD PTR [rbp - 8], rax
    mov QWORD PTR [rbp - 16], rdi
    mov rax, rsi
    call Lparse_url_copy_component_x86
    mov QWORD PTR [rbp - 24], rax
    mov QWORD PTR [rbp - 32], rdx
    mov rax, QWORD PTR [rbp - 16]
    call __rt_parse_url_key_address
    mov rcx, QWORD PTR [rbp - 24]
    mov r8, QWORD PTR [rbp - 32]
    mov r9, 1
    mov rdi, QWORD PTR [rbp - 8]
    call __rt_hash_set
    mov rsp, rbp
    pop rbp
    ret

# Insert the integer port component into the Mixed-valued result hash.
Lparse_url_insert_port_x86:
    push rbp
    mov rbp, rsp
    sub rsp, 32
    mov QWORD PTR [rbp - 8], rax
    mov QWORD PTR [rbp - 16], rsi
    mov rax, rdi
    call __rt_parse_url_key_address
    mov rcx, QWORD PTR [rbp - 16]
    xor r8d, r8d
    xor r9d, r9d
    mov rdi, QWORD PTR [rbp - 8]
    call __rt_hash_set
    mov rsp, rbp
    pop rbp
    ret
