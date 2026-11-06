
    // --- shared mixed string context: _eir_shared_mixed_to_string ---
_eir_shared_mixed_to_string:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    str x19, [sp, #-16]!
    str x0, [sp, #-16]!
    bl __rt_mixed_unbox
    cmp x0, #6
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_object_1
    ldr x0, [sp], #16
    bl __rt_mixed_cast_string
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_object_1:
    ldr x10, [sp], #16
    mov x19, x1
    ldr x9, [x19]
    mov x10, #1
    cmp x9, x10
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_Error_4
    mov x10, #2
    cmp x9, x10
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_TypeError_5
    mov x10, #3
    cmp x9, x10
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_ArgumentCountError_6
    mov x10, #4
    cmp x9, x10
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_ArithmeticError_7
    mov x10, #8
    cmp x9, x10
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_DivisionByZeroError_8
    mov x10, #9
    cmp x9, x10
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_Exception_9
    mov x10, #11
    cmp x9, x10
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_JsonException_10
    mov x10, #17
    cmp x9, x10
    b.eq .L_eir__eir_shared_mixed_to_string_mixed_string_ValueError_11
    b .L_eir__eir_shared_mixed_to_string_mixed_string_no_match_2
.L_eir__eir_shared_mixed_to_string_mixed_string_Error_4:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_TypeError_5:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_ArgumentCountError_6:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_ArithmeticError_7:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_DivisionByZeroError_8:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_Exception_9:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_JsonException_10:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_ValueError_11:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    b .L_eir__eir_shared_mixed_to_string_mixed_string_done_3
.L_eir__eir_shared_mixed_to_string_mixed_string_no_match_2:
    ldr x9, [x19]
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    add x10, x10, x9, lsl #4
    ldr x1, [x10]
    ldr x2, [x10, #8]
    mov x3, x1
    mov x4, x2
    adrp x1, _str_0
    add x1, x1, :lo12:_str_0
    mov x2, #16
    bl __rt_concat
    adrp x3, _str_1
    add x3, x3, :lo12:_str_1
    mov x4, #33
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__eir_shared_mixed_to_string_dynamic_error_throw_12
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_2
    add x1, x1, :lo12:_str_2
    mov x2, #30
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__eir_shared_mixed_to_string_dynamic_error_throw_12:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_error_class_id
    add x9, x9, :lo12:_spl_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__eir_shared_mixed_to_string_mixed_string_done_3:
    ldr x19, [sp], #16
    ldp x29, x30, [sp], #16
    ret
    // @fn name=_class_propinit_1 symbol=_class_propinit_1 synthetic=1
.align 2

.section .text._class_propinit_1,"ax",@progbits
.globl _class_propinit_1
.type _class_propinit_1, %function
_class_propinit_1:
    // prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_1_stack_ok_14
    b __rt_stack_overflow
.L_eir__class_propinit_1_stack_ok_14:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-56]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-48]
    // param $this from x0
    stur x0, [x29, #-40]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-104]
    adrp x10, _class_propinit_1__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_1__cdylib_exception_cleanup
    stur x10, [x29, #-96]
    stur x29, [x29, #-88]
    mov x10, #0
    stur x10, [x29, #-80]
    stur x10, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    sub x10, x29, #104
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_1_entry_13:
    ldur x0, [x29, #-40]
    stur x0, [x29, #-8]
    adrp x1, _str_4
    add x1, x1, :lo12:_str_4
    mov x2, #0
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    bl __rt_str_persist
    ldr x9, [sp], #16
    stp x1, x2, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    mov x0, #0
    ldr x9, [sp], #16
    str x0, [x9, #24]
    str xzr, [x9, #32]
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, x0
    mov x2, xzr
    mov x0, #8
    bl __rt_mixed_from_value
    stur x0, [x29, #-32]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_mixed_unbox
    cmp x0, #0
    b.eq .L_eir__class_propinit_1_typed_prop_previous_0_int_18
    cmp x0, #1
    b.eq .L_eir__class_propinit_1_typed_prop_previous_1_string_19
    cmp x0, #2
    b.eq .L_eir__class_propinit_1_typed_prop_previous_2_float_20
    cmp x0, #3
    b.eq .L_eir__class_propinit_1_typed_prop_previous_3_bool_21
    cmp x0, #4
    b.eq .L_eir__class_propinit_1_typed_prop_previous_4_array_22
    cmp x0, #5
    b.eq .L_eir__class_propinit_1_typed_prop_previous_5_array_23
    cmp x0, #6
    b.eq .L_eir__class_propinit_1_typed_prop_previous_6_object_24
    cmp x0, #8
    b.eq .L_eir__class_propinit_1_typed_prop_previous_8_null_25
    cmp x0, #9
    b.eq .L_eir__class_propinit_1_typed_prop_previous_9_resource_26
    cmp x0, #10
    b.eq .L_eir__class_propinit_1_typed_prop_previous_10_Closure_27
    b .L_eir__class_propinit_1_typed_prop_reject_previous_17
.L_eir__class_propinit_1_typed_prop_previous_0_int_18:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_28
    bl __rt_ob_flush_all
    adrp x1, _str_5
    add x1, x1, :lo12:_str_5
    mov x2, #100
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_28:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_6
    add x9, x9, :lo12:_str_6
    str x9, [x0, #8]
    mov x9, #65
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_previous_1_string_19:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_29
    bl __rt_ob_flush_all
    adrp x1, _str_7
    add x1, x1, :lo12:_str_7
    mov x2, #103
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_29:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_8
    add x9, x9, :lo12:_str_8
    str x9, [x0, #8]
    mov x9, #68
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_previous_2_float_20:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_30
    bl __rt_ob_flush_all
    adrp x1, _str_9
    add x1, x1, :lo12:_str_9
    mov x2, #102
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_30:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_10
    add x9, x9, :lo12:_str_10
    str x9, [x0, #8]
    mov x9, #67
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_previous_3_bool_21:
    mov x0, x1
    cbz x0, 1f
    b .L_eir__class_propinit_1_typed_prop_bool_true_31
1:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_32
    bl __rt_ob_flush_all
    adrp x1, _str_11
    add x1, x1, :lo12:_str_11
    mov x2, #102
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_32:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_12
    add x9, x9, :lo12:_str_12
    str x9, [x0, #8]
    mov x9, #67
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_bool_true_31:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_33
    bl __rt_ob_flush_all
    adrp x1, _str_13
    add x1, x1, :lo12:_str_13
    mov x2, #101
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_33:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_14
    add x9, x9, :lo12:_str_14
    str x9, [x0, #8]
    mov x9, #66
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_previous_4_array_22:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_34
    bl __rt_ob_flush_all
    adrp x1, _str_15
    add x1, x1, :lo12:_str_15
    mov x2, #102
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_34:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_16
    add x9, x9, :lo12:_str_16
    str x9, [x0, #8]
    mov x9, #67
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_previous_5_array_23:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_35
    bl __rt_ob_flush_all
    adrp x1, _str_15
    add x1, x1, :lo12:_str_15
    mov x2, #102
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_35:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_16
    add x9, x9, :lo12:_str_16
    str x9, [x0, #8]
    mov x9, #67
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_previous_6_object_24:
    ldr x10, [x1]
    mov x11, #1
    cmp x10, x11
    b.eq .L_eir__class_propinit_1_typed_prop_accept_previous_15
    mov x11, #2
    cmp x10, x11
    b.eq .L_eir__class_propinit_1_typed_prop_accept_previous_15
    mov x11, #3
    cmp x10, x11
    b.eq .L_eir__class_propinit_1_typed_prop_accept_previous_15
    mov x11, #4
    cmp x10, x11
    b.eq .L_eir__class_propinit_1_typed_prop_accept_previous_15
    mov x11, #8
    cmp x10, x11
    b.eq .L_eir__class_propinit_1_typed_prop_accept_previous_15
    mov x11, #9
    cmp x10, x11
    b.eq .L_eir__class_propinit_1_typed_prop_accept_previous_15
    mov x11, #11
    cmp x10, x11
    b.eq .L_eir__class_propinit_1_typed_prop_accept_previous_15
    mov x11, #17
    cmp x10, x11
    b.eq .L_eir__class_propinit_1_typed_prop_accept_previous_15
    mov x0, x1
    ldr x9, [x0]
    adrp x10, _class_name_count
    add x10, x10, :lo12:_class_name_count
    ldr x10, [x10]
    cmp x9, x10
    b.hs .L_eir__class_propinit_1_runtime_class_name_fallback_37
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    lsl x11, x9, #4
    add x10, x10, x11
    ldr x1, [x10]
    ldr x2, [x10, #8]
    b .L_eir__class_propinit_1_runtime_class_name_done_38
.L_eir__class_propinit_1_runtime_class_name_fallback_37:
    adrp x1, _str_17
    add x1, x1, :lo12:_str_17
    mov x2, #6
.L_eir__class_propinit_1_runtime_class_name_done_38:
    mov x3, x1
    mov x4, x2
    adrp x1, _str_18
    add x1, x1, :lo12:_str_18
    mov x2, #14
    bl __rt_concat
    adrp x3, _str_19
    add x3, x3, :lo12:_str_19
    mov x4, #48
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_dynamic_error_throw_39
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_20
    add x1, x1, :lo12:_str_20
    mov x2, #34
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_dynamic_error_throw_39:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_previous_8_null_25:
    b .L_eir__class_propinit_1_typed_prop_accept_previous_15
.L_eir__class_propinit_1_typed_prop_previous_9_resource_26:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_40
    bl __rt_ob_flush_all
    adrp x1, _str_21
    add x1, x1, :lo12:_str_21
    mov x2, #105
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_40:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_22
    add x9, x9, :lo12:_str_22
    str x9, [x0, #8]
    mov x9, #70
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_previous_10_Closure_27:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_41
    bl __rt_ob_flush_all
    adrp x1, _str_23
    add x1, x1, :lo12:_str_23
    mov x2, #104
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_41:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_24
    add x9, x9, :lo12:_str_24
    str x9, [x0, #8]
    mov x9, #69
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_reject_previous_17:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_1_static_exception_throw_42
    bl __rt_ob_flush_all
    adrp x1, _str_25
    add x1, x1, :lo12:_str_25
    mov x2, #103
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_1_static_exception_throw_42:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_26
    add x9, x9, :lo12:_str_26
    str x9, [x0, #8]
    mov x9, #68
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_1_typed_prop_accept_previous_15:
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    ldr x9, [sp], #16
    str x0, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #40]
    bl __rt_decref_mixed
    ldr x9, [sp], #16
    ldr x0, [sp], #16
    str x0, [x9, #40]
    str xzr, [x9, #48]
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_1_epilogue:
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_1

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_1__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_1__cdylib_exception_cleanup
.type _class_propinit_1__cdylib_exception_cleanup, %function
_class_propinit_1__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_2 symbol=_class_propinit_2 synthetic=1
.align 2

.section .text._class_propinit_2,"ax",@progbits
.globl _class_propinit_2
.type _class_propinit_2, %function
_class_propinit_2:
    // prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_2_stack_ok_44
    b __rt_stack_overflow
.L_eir__class_propinit_2_stack_ok_44:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-56]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-48]
    // param $this from x0
    stur x0, [x29, #-40]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-104]
    adrp x10, _class_propinit_2__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_2__cdylib_exception_cleanup
    stur x10, [x29, #-96]
    stur x29, [x29, #-88]
    mov x10, #0
    stur x10, [x29, #-80]
    stur x10, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    sub x10, x29, #104
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_2_entry_43:
    ldur x0, [x29, #-40]
    stur x0, [x29, #-8]
    adrp x1, _str_4
    add x1, x1, :lo12:_str_4
    mov x2, #0
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    bl __rt_str_persist
    ldr x9, [sp], #16
    stp x1, x2, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    mov x0, #0
    ldr x9, [sp], #16
    str x0, [x9, #24]
    str xzr, [x9, #32]
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, x0
    mov x2, xzr
    mov x0, #8
    bl __rt_mixed_from_value
    stur x0, [x29, #-32]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_mixed_unbox
    cmp x0, #0
    b.eq .L_eir__class_propinit_2_typed_prop_previous_0_int_48
    cmp x0, #1
    b.eq .L_eir__class_propinit_2_typed_prop_previous_1_string_49
    cmp x0, #2
    b.eq .L_eir__class_propinit_2_typed_prop_previous_2_float_50
    cmp x0, #3
    b.eq .L_eir__class_propinit_2_typed_prop_previous_3_bool_51
    cmp x0, #4
    b.eq .L_eir__class_propinit_2_typed_prop_previous_4_array_52
    cmp x0, #5
    b.eq .L_eir__class_propinit_2_typed_prop_previous_5_array_53
    cmp x0, #6
    b.eq .L_eir__class_propinit_2_typed_prop_previous_6_object_54
    cmp x0, #8
    b.eq .L_eir__class_propinit_2_typed_prop_previous_8_null_55
    cmp x0, #9
    b.eq .L_eir__class_propinit_2_typed_prop_previous_9_resource_56
    cmp x0, #10
    b.eq .L_eir__class_propinit_2_typed_prop_previous_10_Closure_57
    b .L_eir__class_propinit_2_typed_prop_reject_previous_47
.L_eir__class_propinit_2_typed_prop_previous_0_int_48:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_58
    bl __rt_ob_flush_all
    adrp x1, _str_27
    add x1, x1, :lo12:_str_27
    mov x2, #104
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_58:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_28
    add x9, x9, :lo12:_str_28
    str x9, [x0, #8]
    mov x9, #69
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_previous_1_string_49:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_59
    bl __rt_ob_flush_all
    adrp x1, _str_29
    add x1, x1, :lo12:_str_29
    mov x2, #107
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_59:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_30
    add x9, x9, :lo12:_str_30
    str x9, [x0, #8]
    mov x9, #72
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_previous_2_float_50:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_60
    bl __rt_ob_flush_all
    adrp x1, _str_31
    add x1, x1, :lo12:_str_31
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_60:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_32
    add x9, x9, :lo12:_str_32
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_previous_3_bool_51:
    mov x0, x1
    cbz x0, 1f
    b .L_eir__class_propinit_2_typed_prop_bool_true_61
1:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_62
    bl __rt_ob_flush_all
    adrp x1, _str_33
    add x1, x1, :lo12:_str_33
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_62:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_34
    add x9, x9, :lo12:_str_34
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_bool_true_61:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_63
    bl __rt_ob_flush_all
    adrp x1, _str_35
    add x1, x1, :lo12:_str_35
    mov x2, #105
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_63:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_36
    add x9, x9, :lo12:_str_36
    str x9, [x0, #8]
    mov x9, #70
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_previous_4_array_52:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_64
    bl __rt_ob_flush_all
    adrp x1, _str_37
    add x1, x1, :lo12:_str_37
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_64:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_38
    add x9, x9, :lo12:_str_38
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_previous_5_array_53:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_65
    bl __rt_ob_flush_all
    adrp x1, _str_37
    add x1, x1, :lo12:_str_37
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_65:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_38
    add x9, x9, :lo12:_str_38
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_previous_6_object_54:
    ldr x10, [x1]
    mov x11, #1
    cmp x10, x11
    b.eq .L_eir__class_propinit_2_typed_prop_accept_previous_45
    mov x11, #2
    cmp x10, x11
    b.eq .L_eir__class_propinit_2_typed_prop_accept_previous_45
    mov x11, #3
    cmp x10, x11
    b.eq .L_eir__class_propinit_2_typed_prop_accept_previous_45
    mov x11, #4
    cmp x10, x11
    b.eq .L_eir__class_propinit_2_typed_prop_accept_previous_45
    mov x11, #8
    cmp x10, x11
    b.eq .L_eir__class_propinit_2_typed_prop_accept_previous_45
    mov x11, #9
    cmp x10, x11
    b.eq .L_eir__class_propinit_2_typed_prop_accept_previous_45
    mov x11, #11
    cmp x10, x11
    b.eq .L_eir__class_propinit_2_typed_prop_accept_previous_45
    mov x11, #17
    cmp x10, x11
    b.eq .L_eir__class_propinit_2_typed_prop_accept_previous_45
    mov x0, x1
    ldr x9, [x0]
    adrp x10, _class_name_count
    add x10, x10, :lo12:_class_name_count
    ldr x10, [x10]
    cmp x9, x10
    b.hs .L_eir__class_propinit_2_runtime_class_name_fallback_67
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    lsl x11, x9, #4
    add x10, x10, x11
    ldr x1, [x10]
    ldr x2, [x10, #8]
    b .L_eir__class_propinit_2_runtime_class_name_done_68
.L_eir__class_propinit_2_runtime_class_name_fallback_67:
    adrp x1, _str_17
    add x1, x1, :lo12:_str_17
    mov x2, #6
.L_eir__class_propinit_2_runtime_class_name_done_68:
    mov x3, x1
    mov x4, x2
    adrp x1, _str_18
    add x1, x1, :lo12:_str_18
    mov x2, #14
    bl __rt_concat
    adrp x3, _str_39
    add x3, x3, :lo12:_str_39
    mov x4, #52
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_dynamic_error_throw_69
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_20
    add x1, x1, :lo12:_str_20
    mov x2, #34
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_dynamic_error_throw_69:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_previous_8_null_55:
    b .L_eir__class_propinit_2_typed_prop_accept_previous_45
.L_eir__class_propinit_2_typed_prop_previous_9_resource_56:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_70
    bl __rt_ob_flush_all
    adrp x1, _str_40
    add x1, x1, :lo12:_str_40
    mov x2, #109
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_70:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_41
    add x9, x9, :lo12:_str_41
    str x9, [x0, #8]
    mov x9, #74
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_previous_10_Closure_57:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_71
    bl __rt_ob_flush_all
    adrp x1, _str_42
    add x1, x1, :lo12:_str_42
    mov x2, #108
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_71:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_43
    add x9, x9, :lo12:_str_43
    str x9, [x0, #8]
    mov x9, #73
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_reject_previous_47:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_2_static_exception_throw_72
    bl __rt_ob_flush_all
    adrp x1, _str_44
    add x1, x1, :lo12:_str_44
    mov x2, #107
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_2_static_exception_throw_72:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_45
    add x9, x9, :lo12:_str_45
    str x9, [x0, #8]
    mov x9, #72
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_2_typed_prop_accept_previous_45:
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    ldr x9, [sp], #16
    str x0, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #40]
    bl __rt_decref_mixed
    ldr x9, [sp], #16
    ldr x0, [sp], #16
    str x0, [x9, #40]
    str xzr, [x9, #48]
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_2_epilogue:
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_2

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_2__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_2__cdylib_exception_cleanup
.type _class_propinit_2__cdylib_exception_cleanup, %function
_class_propinit_2__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_3 symbol=_class_propinit_3 synthetic=1
.align 2

.section .text._class_propinit_3,"ax",@progbits
.globl _class_propinit_3
.type _class_propinit_3, %function
_class_propinit_3:
    // prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_3_stack_ok_74
    b __rt_stack_overflow
.L_eir__class_propinit_3_stack_ok_74:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-56]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-48]
    // param $this from x0
    stur x0, [x29, #-40]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-104]
    adrp x10, _class_propinit_3__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_3__cdylib_exception_cleanup
    stur x10, [x29, #-96]
    stur x29, [x29, #-88]
    mov x10, #0
    stur x10, [x29, #-80]
    stur x10, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    sub x10, x29, #104
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_3_entry_73:
    ldur x0, [x29, #-40]
    stur x0, [x29, #-8]
    adrp x1, _str_4
    add x1, x1, :lo12:_str_4
    mov x2, #0
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    bl __rt_str_persist
    ldr x9, [sp], #16
    stp x1, x2, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    mov x0, #0
    ldr x9, [sp], #16
    str x0, [x9, #24]
    str xzr, [x9, #32]
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, x0
    mov x2, xzr
    mov x0, #8
    bl __rt_mixed_from_value
    stur x0, [x29, #-32]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_mixed_unbox
    cmp x0, #0
    b.eq .L_eir__class_propinit_3_typed_prop_previous_0_int_78
    cmp x0, #1
    b.eq .L_eir__class_propinit_3_typed_prop_previous_1_string_79
    cmp x0, #2
    b.eq .L_eir__class_propinit_3_typed_prop_previous_2_float_80
    cmp x0, #3
    b.eq .L_eir__class_propinit_3_typed_prop_previous_3_bool_81
    cmp x0, #4
    b.eq .L_eir__class_propinit_3_typed_prop_previous_4_array_82
    cmp x0, #5
    b.eq .L_eir__class_propinit_3_typed_prop_previous_5_array_83
    cmp x0, #6
    b.eq .L_eir__class_propinit_3_typed_prop_previous_6_object_84
    cmp x0, #8
    b.eq .L_eir__class_propinit_3_typed_prop_previous_8_null_85
    cmp x0, #9
    b.eq .L_eir__class_propinit_3_typed_prop_previous_9_resource_86
    cmp x0, #10
    b.eq .L_eir__class_propinit_3_typed_prop_previous_10_Closure_87
    b .L_eir__class_propinit_3_typed_prop_reject_previous_77
.L_eir__class_propinit_3_typed_prop_previous_0_int_78:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_88
    bl __rt_ob_flush_all
    adrp x1, _str_46
    add x1, x1, :lo12:_str_46
    mov x2, #113
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_88:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_47
    add x9, x9, :lo12:_str_47
    str x9, [x0, #8]
    mov x9, #78
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_previous_1_string_79:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_89
    bl __rt_ob_flush_all
    adrp x1, _str_48
    add x1, x1, :lo12:_str_48
    mov x2, #116
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_89:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_49
    add x9, x9, :lo12:_str_49
    str x9, [x0, #8]
    mov x9, #81
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_previous_2_float_80:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_90
    bl __rt_ob_flush_all
    adrp x1, _str_50
    add x1, x1, :lo12:_str_50
    mov x2, #115
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_90:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_51
    add x9, x9, :lo12:_str_51
    str x9, [x0, #8]
    mov x9, #80
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_previous_3_bool_81:
    mov x0, x1
    cbz x0, 1f
    b .L_eir__class_propinit_3_typed_prop_bool_true_91
1:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_92
    bl __rt_ob_flush_all
    adrp x1, _str_52
    add x1, x1, :lo12:_str_52
    mov x2, #115
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_92:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_53
    add x9, x9, :lo12:_str_53
    str x9, [x0, #8]
    mov x9, #80
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_bool_true_91:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_93
    bl __rt_ob_flush_all
    adrp x1, _str_54
    add x1, x1, :lo12:_str_54
    mov x2, #114
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_93:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_55
    add x9, x9, :lo12:_str_55
    str x9, [x0, #8]
    mov x9, #79
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_previous_4_array_82:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_94
    bl __rt_ob_flush_all
    adrp x1, _str_56
    add x1, x1, :lo12:_str_56
    mov x2, #115
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_94:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_57
    add x9, x9, :lo12:_str_57
    str x9, [x0, #8]
    mov x9, #80
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_previous_5_array_83:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_95
    bl __rt_ob_flush_all
    adrp x1, _str_56
    add x1, x1, :lo12:_str_56
    mov x2, #115
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_95:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_57
    add x9, x9, :lo12:_str_57
    str x9, [x0, #8]
    mov x9, #80
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_previous_6_object_84:
    ldr x10, [x1]
    mov x11, #1
    cmp x10, x11
    b.eq .L_eir__class_propinit_3_typed_prop_accept_previous_75
    mov x11, #2
    cmp x10, x11
    b.eq .L_eir__class_propinit_3_typed_prop_accept_previous_75
    mov x11, #3
    cmp x10, x11
    b.eq .L_eir__class_propinit_3_typed_prop_accept_previous_75
    mov x11, #4
    cmp x10, x11
    b.eq .L_eir__class_propinit_3_typed_prop_accept_previous_75
    mov x11, #8
    cmp x10, x11
    b.eq .L_eir__class_propinit_3_typed_prop_accept_previous_75
    mov x11, #9
    cmp x10, x11
    b.eq .L_eir__class_propinit_3_typed_prop_accept_previous_75
    mov x11, #11
    cmp x10, x11
    b.eq .L_eir__class_propinit_3_typed_prop_accept_previous_75
    mov x11, #17
    cmp x10, x11
    b.eq .L_eir__class_propinit_3_typed_prop_accept_previous_75
    mov x0, x1
    ldr x9, [x0]
    adrp x10, _class_name_count
    add x10, x10, :lo12:_class_name_count
    ldr x10, [x10]
    cmp x9, x10
    b.hs .L_eir__class_propinit_3_runtime_class_name_fallback_97
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    lsl x11, x9, #4
    add x10, x10, x11
    ldr x1, [x10]
    ldr x2, [x10, #8]
    b .L_eir__class_propinit_3_runtime_class_name_done_98
.L_eir__class_propinit_3_runtime_class_name_fallback_97:
    adrp x1, _str_17
    add x1, x1, :lo12:_str_17
    mov x2, #6
.L_eir__class_propinit_3_runtime_class_name_done_98:
    mov x3, x1
    mov x4, x2
    adrp x1, _str_18
    add x1, x1, :lo12:_str_18
    mov x2, #14
    bl __rt_concat
    adrp x3, _str_58
    add x3, x3, :lo12:_str_58
    mov x4, #61
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_dynamic_error_throw_99
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_20
    add x1, x1, :lo12:_str_20
    mov x2, #34
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_dynamic_error_throw_99:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_previous_8_null_85:
    b .L_eir__class_propinit_3_typed_prop_accept_previous_75
.L_eir__class_propinit_3_typed_prop_previous_9_resource_86:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_100
    bl __rt_ob_flush_all
    adrp x1, _str_59
    add x1, x1, :lo12:_str_59
    mov x2, #118
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_100:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_60
    add x9, x9, :lo12:_str_60
    str x9, [x0, #8]
    mov x9, #83
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_previous_10_Closure_87:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_101
    bl __rt_ob_flush_all
    adrp x1, _str_61
    add x1, x1, :lo12:_str_61
    mov x2, #117
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_101:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_62
    add x9, x9, :lo12:_str_62
    str x9, [x0, #8]
    mov x9, #82
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_reject_previous_77:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_3_static_exception_throw_102
    bl __rt_ob_flush_all
    adrp x1, _str_63
    add x1, x1, :lo12:_str_63
    mov x2, #116
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_3_static_exception_throw_102:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_64
    add x9, x9, :lo12:_str_64
    str x9, [x0, #8]
    mov x9, #81
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_3_typed_prop_accept_previous_75:
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    ldr x9, [sp], #16
    str x0, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #40]
    bl __rt_decref_mixed
    ldr x9, [sp], #16
    ldr x0, [sp], #16
    str x0, [x9, #40]
    str xzr, [x9, #48]
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_3_epilogue:
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_3

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_3__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_3__cdylib_exception_cleanup
.type _class_propinit_3__cdylib_exception_cleanup, %function
_class_propinit_3__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_4 symbol=_class_propinit_4 synthetic=1
.align 2

.section .text._class_propinit_4,"ax",@progbits
.globl _class_propinit_4
.type _class_propinit_4, %function
_class_propinit_4:
    // prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_4_stack_ok_104
    b __rt_stack_overflow
.L_eir__class_propinit_4_stack_ok_104:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-56]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-48]
    // param $this from x0
    stur x0, [x29, #-40]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-104]
    adrp x10, _class_propinit_4__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_4__cdylib_exception_cleanup
    stur x10, [x29, #-96]
    stur x29, [x29, #-88]
    mov x10, #0
    stur x10, [x29, #-80]
    stur x10, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    sub x10, x29, #104
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_4_entry_103:
    ldur x0, [x29, #-40]
    stur x0, [x29, #-8]
    adrp x1, _str_4
    add x1, x1, :lo12:_str_4
    mov x2, #0
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    bl __rt_str_persist
    ldr x9, [sp], #16
    stp x1, x2, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    mov x0, #0
    ldr x9, [sp], #16
    str x0, [x9, #24]
    str xzr, [x9, #32]
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, x0
    mov x2, xzr
    mov x0, #8
    bl __rt_mixed_from_value
    stur x0, [x29, #-32]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_mixed_unbox
    cmp x0, #0
    b.eq .L_eir__class_propinit_4_typed_prop_previous_0_int_108
    cmp x0, #1
    b.eq .L_eir__class_propinit_4_typed_prop_previous_1_string_109
    cmp x0, #2
    b.eq .L_eir__class_propinit_4_typed_prop_previous_2_float_110
    cmp x0, #3
    b.eq .L_eir__class_propinit_4_typed_prop_previous_3_bool_111
    cmp x0, #4
    b.eq .L_eir__class_propinit_4_typed_prop_previous_4_array_112
    cmp x0, #5
    b.eq .L_eir__class_propinit_4_typed_prop_previous_5_array_113
    cmp x0, #6
    b.eq .L_eir__class_propinit_4_typed_prop_previous_6_object_114
    cmp x0, #8
    b.eq .L_eir__class_propinit_4_typed_prop_previous_8_null_115
    cmp x0, #9
    b.eq .L_eir__class_propinit_4_typed_prop_previous_9_resource_116
    cmp x0, #10
    b.eq .L_eir__class_propinit_4_typed_prop_previous_10_Closure_117
    b .L_eir__class_propinit_4_typed_prop_reject_previous_107
.L_eir__class_propinit_4_typed_prop_previous_0_int_108:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_118
    bl __rt_ob_flush_all
    adrp x1, _str_65
    add x1, x1, :lo12:_str_65
    mov x2, #110
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_118:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_66
    add x9, x9, :lo12:_str_66
    str x9, [x0, #8]
    mov x9, #75
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_previous_1_string_109:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_119
    bl __rt_ob_flush_all
    adrp x1, _str_67
    add x1, x1, :lo12:_str_67
    mov x2, #113
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_119:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_68
    add x9, x9, :lo12:_str_68
    str x9, [x0, #8]
    mov x9, #78
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_previous_2_float_110:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_120
    bl __rt_ob_flush_all
    adrp x1, _str_69
    add x1, x1, :lo12:_str_69
    mov x2, #112
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_120:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_70
    add x9, x9, :lo12:_str_70
    str x9, [x0, #8]
    mov x9, #77
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_previous_3_bool_111:
    mov x0, x1
    cbz x0, 1f
    b .L_eir__class_propinit_4_typed_prop_bool_true_121
1:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_122
    bl __rt_ob_flush_all
    adrp x1, _str_71
    add x1, x1, :lo12:_str_71
    mov x2, #112
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_122:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_72
    add x9, x9, :lo12:_str_72
    str x9, [x0, #8]
    mov x9, #77
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_bool_true_121:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_123
    bl __rt_ob_flush_all
    adrp x1, _str_73
    add x1, x1, :lo12:_str_73
    mov x2, #111
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_123:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_74
    add x9, x9, :lo12:_str_74
    str x9, [x0, #8]
    mov x9, #76
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_previous_4_array_112:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_124
    bl __rt_ob_flush_all
    adrp x1, _str_75
    add x1, x1, :lo12:_str_75
    mov x2, #112
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_124:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_76
    add x9, x9, :lo12:_str_76
    str x9, [x0, #8]
    mov x9, #77
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_previous_5_array_113:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_125
    bl __rt_ob_flush_all
    adrp x1, _str_75
    add x1, x1, :lo12:_str_75
    mov x2, #112
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_125:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_76
    add x9, x9, :lo12:_str_76
    str x9, [x0, #8]
    mov x9, #77
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_previous_6_object_114:
    ldr x10, [x1]
    mov x11, #1
    cmp x10, x11
    b.eq .L_eir__class_propinit_4_typed_prop_accept_previous_105
    mov x11, #2
    cmp x10, x11
    b.eq .L_eir__class_propinit_4_typed_prop_accept_previous_105
    mov x11, #3
    cmp x10, x11
    b.eq .L_eir__class_propinit_4_typed_prop_accept_previous_105
    mov x11, #4
    cmp x10, x11
    b.eq .L_eir__class_propinit_4_typed_prop_accept_previous_105
    mov x11, #8
    cmp x10, x11
    b.eq .L_eir__class_propinit_4_typed_prop_accept_previous_105
    mov x11, #9
    cmp x10, x11
    b.eq .L_eir__class_propinit_4_typed_prop_accept_previous_105
    mov x11, #11
    cmp x10, x11
    b.eq .L_eir__class_propinit_4_typed_prop_accept_previous_105
    mov x11, #17
    cmp x10, x11
    b.eq .L_eir__class_propinit_4_typed_prop_accept_previous_105
    mov x0, x1
    ldr x9, [x0]
    adrp x10, _class_name_count
    add x10, x10, :lo12:_class_name_count
    ldr x10, [x10]
    cmp x9, x10
    b.hs .L_eir__class_propinit_4_runtime_class_name_fallback_127
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    lsl x11, x9, #4
    add x10, x10, x11
    ldr x1, [x10]
    ldr x2, [x10, #8]
    b .L_eir__class_propinit_4_runtime_class_name_done_128
.L_eir__class_propinit_4_runtime_class_name_fallback_127:
    adrp x1, _str_17
    add x1, x1, :lo12:_str_17
    mov x2, #6
.L_eir__class_propinit_4_runtime_class_name_done_128:
    mov x3, x1
    mov x4, x2
    adrp x1, _str_18
    add x1, x1, :lo12:_str_18
    mov x2, #14
    bl __rt_concat
    adrp x3, _str_77
    add x3, x3, :lo12:_str_77
    mov x4, #58
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_dynamic_error_throw_129
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_20
    add x1, x1, :lo12:_str_20
    mov x2, #34
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_dynamic_error_throw_129:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_previous_8_null_115:
    b .L_eir__class_propinit_4_typed_prop_accept_previous_105
.L_eir__class_propinit_4_typed_prop_previous_9_resource_116:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_130
    bl __rt_ob_flush_all
    adrp x1, _str_78
    add x1, x1, :lo12:_str_78
    mov x2, #115
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_130:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_79
    add x9, x9, :lo12:_str_79
    str x9, [x0, #8]
    mov x9, #80
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_previous_10_Closure_117:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_131
    bl __rt_ob_flush_all
    adrp x1, _str_80
    add x1, x1, :lo12:_str_80
    mov x2, #114
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_131:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_81
    add x9, x9, :lo12:_str_81
    str x9, [x0, #8]
    mov x9, #79
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_reject_previous_107:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_4_static_exception_throw_132
    bl __rt_ob_flush_all
    adrp x1, _str_82
    add x1, x1, :lo12:_str_82
    mov x2, #113
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_4_static_exception_throw_132:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_83
    add x9, x9, :lo12:_str_83
    str x9, [x0, #8]
    mov x9, #78
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_4_typed_prop_accept_previous_105:
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    ldr x9, [sp], #16
    str x0, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #40]
    bl __rt_decref_mixed
    ldr x9, [sp], #16
    ldr x0, [sp], #16
    str x0, [x9, #40]
    str xzr, [x9, #48]
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_4_epilogue:
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_4

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_4__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_4__cdylib_exception_cleanup
.type _class_propinit_4__cdylib_exception_cleanup, %function
_class_propinit_4__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_8 symbol=_class_propinit_8 synthetic=1
.align 2

.section .text._class_propinit_8,"ax",@progbits
.globl _class_propinit_8
.type _class_propinit_8, %function
_class_propinit_8:
    // prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_8_stack_ok_134
    b __rt_stack_overflow
.L_eir__class_propinit_8_stack_ok_134:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-56]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-48]
    // param $this from x0
    stur x0, [x29, #-40]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-104]
    adrp x10, _class_propinit_8__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_8__cdylib_exception_cleanup
    stur x10, [x29, #-96]
    stur x29, [x29, #-88]
    mov x10, #0
    stur x10, [x29, #-80]
    stur x10, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    sub x10, x29, #104
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_8_entry_133:
    ldur x0, [x29, #-40]
    stur x0, [x29, #-8]
    adrp x1, _str_4
    add x1, x1, :lo12:_str_4
    mov x2, #0
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    bl __rt_str_persist
    ldr x9, [sp], #16
    stp x1, x2, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    mov x0, #0
    ldr x9, [sp], #16
    str x0, [x9, #24]
    str xzr, [x9, #32]
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, x0
    mov x2, xzr
    mov x0, #8
    bl __rt_mixed_from_value
    stur x0, [x29, #-32]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_mixed_unbox
    cmp x0, #0
    b.eq .L_eir__class_propinit_8_typed_prop_previous_0_int_138
    cmp x0, #1
    b.eq .L_eir__class_propinit_8_typed_prop_previous_1_string_139
    cmp x0, #2
    b.eq .L_eir__class_propinit_8_typed_prop_previous_2_float_140
    cmp x0, #3
    b.eq .L_eir__class_propinit_8_typed_prop_previous_3_bool_141
    cmp x0, #4
    b.eq .L_eir__class_propinit_8_typed_prop_previous_4_array_142
    cmp x0, #5
    b.eq .L_eir__class_propinit_8_typed_prop_previous_5_array_143
    cmp x0, #6
    b.eq .L_eir__class_propinit_8_typed_prop_previous_6_object_144
    cmp x0, #8
    b.eq .L_eir__class_propinit_8_typed_prop_previous_8_null_145
    cmp x0, #9
    b.eq .L_eir__class_propinit_8_typed_prop_previous_9_resource_146
    cmp x0, #10
    b.eq .L_eir__class_propinit_8_typed_prop_previous_10_Closure_147
    b .L_eir__class_propinit_8_typed_prop_reject_previous_137
.L_eir__class_propinit_8_typed_prop_previous_0_int_138:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_148
    bl __rt_ob_flush_all
    adrp x1, _str_84
    add x1, x1, :lo12:_str_84
    mov x2, #114
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_148:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_85
    add x9, x9, :lo12:_str_85
    str x9, [x0, #8]
    mov x9, #79
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_previous_1_string_139:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_149
    bl __rt_ob_flush_all
    adrp x1, _str_86
    add x1, x1, :lo12:_str_86
    mov x2, #117
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_149:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_87
    add x9, x9, :lo12:_str_87
    str x9, [x0, #8]
    mov x9, #82
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_previous_2_float_140:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_150
    bl __rt_ob_flush_all
    adrp x1, _str_88
    add x1, x1, :lo12:_str_88
    mov x2, #116
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_150:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_89
    add x9, x9, :lo12:_str_89
    str x9, [x0, #8]
    mov x9, #81
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_previous_3_bool_141:
    mov x0, x1
    cbz x0, 1f
    b .L_eir__class_propinit_8_typed_prop_bool_true_151
1:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_152
    bl __rt_ob_flush_all
    adrp x1, _str_90
    add x1, x1, :lo12:_str_90
    mov x2, #116
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_152:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_91
    add x9, x9, :lo12:_str_91
    str x9, [x0, #8]
    mov x9, #81
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_bool_true_151:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_153
    bl __rt_ob_flush_all
    adrp x1, _str_92
    add x1, x1, :lo12:_str_92
    mov x2, #115
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_153:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_93
    add x9, x9, :lo12:_str_93
    str x9, [x0, #8]
    mov x9, #80
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_previous_4_array_142:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_154
    bl __rt_ob_flush_all
    adrp x1, _str_94
    add x1, x1, :lo12:_str_94
    mov x2, #116
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_154:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_95
    add x9, x9, :lo12:_str_95
    str x9, [x0, #8]
    mov x9, #81
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_previous_5_array_143:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_155
    bl __rt_ob_flush_all
    adrp x1, _str_94
    add x1, x1, :lo12:_str_94
    mov x2, #116
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_155:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_95
    add x9, x9, :lo12:_str_95
    str x9, [x0, #8]
    mov x9, #81
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_previous_6_object_144:
    ldr x10, [x1]
    mov x11, #1
    cmp x10, x11
    b.eq .L_eir__class_propinit_8_typed_prop_accept_previous_135
    mov x11, #2
    cmp x10, x11
    b.eq .L_eir__class_propinit_8_typed_prop_accept_previous_135
    mov x11, #3
    cmp x10, x11
    b.eq .L_eir__class_propinit_8_typed_prop_accept_previous_135
    mov x11, #4
    cmp x10, x11
    b.eq .L_eir__class_propinit_8_typed_prop_accept_previous_135
    mov x11, #8
    cmp x10, x11
    b.eq .L_eir__class_propinit_8_typed_prop_accept_previous_135
    mov x11, #9
    cmp x10, x11
    b.eq .L_eir__class_propinit_8_typed_prop_accept_previous_135
    mov x11, #11
    cmp x10, x11
    b.eq .L_eir__class_propinit_8_typed_prop_accept_previous_135
    mov x11, #17
    cmp x10, x11
    b.eq .L_eir__class_propinit_8_typed_prop_accept_previous_135
    mov x0, x1
    ldr x9, [x0]
    adrp x10, _class_name_count
    add x10, x10, :lo12:_class_name_count
    ldr x10, [x10]
    cmp x9, x10
    b.hs .L_eir__class_propinit_8_runtime_class_name_fallback_157
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    lsl x11, x9, #4
    add x10, x10, x11
    ldr x1, [x10]
    ldr x2, [x10, #8]
    b .L_eir__class_propinit_8_runtime_class_name_done_158
.L_eir__class_propinit_8_runtime_class_name_fallback_157:
    adrp x1, _str_17
    add x1, x1, :lo12:_str_17
    mov x2, #6
.L_eir__class_propinit_8_runtime_class_name_done_158:
    mov x3, x1
    mov x4, x2
    adrp x1, _str_18
    add x1, x1, :lo12:_str_18
    mov x2, #14
    bl __rt_concat
    adrp x3, _str_96
    add x3, x3, :lo12:_str_96
    mov x4, #62
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_dynamic_error_throw_159
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_20
    add x1, x1, :lo12:_str_20
    mov x2, #34
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_dynamic_error_throw_159:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_previous_8_null_145:
    b .L_eir__class_propinit_8_typed_prop_accept_previous_135
.L_eir__class_propinit_8_typed_prop_previous_9_resource_146:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_160
    bl __rt_ob_flush_all
    adrp x1, _str_97
    add x1, x1, :lo12:_str_97
    mov x2, #119
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_160:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_98
    add x9, x9, :lo12:_str_98
    str x9, [x0, #8]
    mov x9, #84
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_previous_10_Closure_147:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_161
    bl __rt_ob_flush_all
    adrp x1, _str_99
    add x1, x1, :lo12:_str_99
    mov x2, #118
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_161:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_100
    add x9, x9, :lo12:_str_100
    str x9, [x0, #8]
    mov x9, #83
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_reject_previous_137:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_8_static_exception_throw_162
    bl __rt_ob_flush_all
    adrp x1, _str_101
    add x1, x1, :lo12:_str_101
    mov x2, #117
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_8_static_exception_throw_162:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_102
    add x9, x9, :lo12:_str_102
    str x9, [x0, #8]
    mov x9, #82
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_8_typed_prop_accept_previous_135:
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    ldr x9, [sp], #16
    str x0, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #40]
    bl __rt_decref_mixed
    ldr x9, [sp], #16
    ldr x0, [sp], #16
    str x0, [x9, #40]
    str xzr, [x9, #48]
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_8_epilogue:
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_8

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_8__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_8__cdylib_exception_cleanup
.type _class_propinit_8__cdylib_exception_cleanup, %function
_class_propinit_8__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_9 symbol=_class_propinit_9 synthetic=1
.align 2

.section .text._class_propinit_9,"ax",@progbits
.globl _class_propinit_9
.type _class_propinit_9, %function
_class_propinit_9:
    // prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_9_stack_ok_164
    b __rt_stack_overflow
.L_eir__class_propinit_9_stack_ok_164:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-56]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-48]
    // param $this from x0
    stur x0, [x29, #-40]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-104]
    adrp x10, _class_propinit_9__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_9__cdylib_exception_cleanup
    stur x10, [x29, #-96]
    stur x29, [x29, #-88]
    mov x10, #0
    stur x10, [x29, #-80]
    stur x10, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    sub x10, x29, #104
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_9_entry_163:
    ldur x0, [x29, #-40]
    stur x0, [x29, #-8]
    adrp x1, _str_4
    add x1, x1, :lo12:_str_4
    mov x2, #0
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    bl __rt_str_persist
    ldr x9, [sp], #16
    stp x1, x2, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    mov x0, #0
    ldr x9, [sp], #16
    str x0, [x9, #24]
    str xzr, [x9, #32]
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, x0
    mov x2, xzr
    mov x0, #8
    bl __rt_mixed_from_value
    stur x0, [x29, #-32]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_mixed_unbox
    cmp x0, #0
    b.eq .L_eir__class_propinit_9_typed_prop_previous_0_int_168
    cmp x0, #1
    b.eq .L_eir__class_propinit_9_typed_prop_previous_1_string_169
    cmp x0, #2
    b.eq .L_eir__class_propinit_9_typed_prop_previous_2_float_170
    cmp x0, #3
    b.eq .L_eir__class_propinit_9_typed_prop_previous_3_bool_171
    cmp x0, #4
    b.eq .L_eir__class_propinit_9_typed_prop_previous_4_array_172
    cmp x0, #5
    b.eq .L_eir__class_propinit_9_typed_prop_previous_5_array_173
    cmp x0, #6
    b.eq .L_eir__class_propinit_9_typed_prop_previous_6_object_174
    cmp x0, #8
    b.eq .L_eir__class_propinit_9_typed_prop_previous_8_null_175
    cmp x0, #9
    b.eq .L_eir__class_propinit_9_typed_prop_previous_9_resource_176
    cmp x0, #10
    b.eq .L_eir__class_propinit_9_typed_prop_previous_10_Closure_177
    b .L_eir__class_propinit_9_typed_prop_reject_previous_167
.L_eir__class_propinit_9_typed_prop_previous_0_int_168:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_178
    bl __rt_ob_flush_all
    adrp x1, _str_103
    add x1, x1, :lo12:_str_103
    mov x2, #104
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_178:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_104
    add x9, x9, :lo12:_str_104
    str x9, [x0, #8]
    mov x9, #69
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_previous_1_string_169:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_179
    bl __rt_ob_flush_all
    adrp x1, _str_105
    add x1, x1, :lo12:_str_105
    mov x2, #107
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_179:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_106
    add x9, x9, :lo12:_str_106
    str x9, [x0, #8]
    mov x9, #72
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_previous_2_float_170:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_180
    bl __rt_ob_flush_all
    adrp x1, _str_107
    add x1, x1, :lo12:_str_107
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_180:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_108
    add x9, x9, :lo12:_str_108
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_previous_3_bool_171:
    mov x0, x1
    cbz x0, 1f
    b .L_eir__class_propinit_9_typed_prop_bool_true_181
1:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_182
    bl __rt_ob_flush_all
    adrp x1, _str_109
    add x1, x1, :lo12:_str_109
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_182:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_110
    add x9, x9, :lo12:_str_110
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_bool_true_181:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_183
    bl __rt_ob_flush_all
    adrp x1, _str_111
    add x1, x1, :lo12:_str_111
    mov x2, #105
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_183:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_112
    add x9, x9, :lo12:_str_112
    str x9, [x0, #8]
    mov x9, #70
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_previous_4_array_172:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_184
    bl __rt_ob_flush_all
    adrp x1, _str_113
    add x1, x1, :lo12:_str_113
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_184:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_114
    add x9, x9, :lo12:_str_114
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_previous_5_array_173:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_185
    bl __rt_ob_flush_all
    adrp x1, _str_113
    add x1, x1, :lo12:_str_113
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_185:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_114
    add x9, x9, :lo12:_str_114
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_previous_6_object_174:
    ldr x10, [x1]
    mov x11, #1
    cmp x10, x11
    b.eq .L_eir__class_propinit_9_typed_prop_accept_previous_165
    mov x11, #2
    cmp x10, x11
    b.eq .L_eir__class_propinit_9_typed_prop_accept_previous_165
    mov x11, #3
    cmp x10, x11
    b.eq .L_eir__class_propinit_9_typed_prop_accept_previous_165
    mov x11, #4
    cmp x10, x11
    b.eq .L_eir__class_propinit_9_typed_prop_accept_previous_165
    mov x11, #8
    cmp x10, x11
    b.eq .L_eir__class_propinit_9_typed_prop_accept_previous_165
    mov x11, #9
    cmp x10, x11
    b.eq .L_eir__class_propinit_9_typed_prop_accept_previous_165
    mov x11, #11
    cmp x10, x11
    b.eq .L_eir__class_propinit_9_typed_prop_accept_previous_165
    mov x11, #17
    cmp x10, x11
    b.eq .L_eir__class_propinit_9_typed_prop_accept_previous_165
    mov x0, x1
    ldr x9, [x0]
    adrp x10, _class_name_count
    add x10, x10, :lo12:_class_name_count
    ldr x10, [x10]
    cmp x9, x10
    b.hs .L_eir__class_propinit_9_runtime_class_name_fallback_187
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    lsl x11, x9, #4
    add x10, x10, x11
    ldr x1, [x10]
    ldr x2, [x10, #8]
    b .L_eir__class_propinit_9_runtime_class_name_done_188
.L_eir__class_propinit_9_runtime_class_name_fallback_187:
    adrp x1, _str_17
    add x1, x1, :lo12:_str_17
    mov x2, #6
.L_eir__class_propinit_9_runtime_class_name_done_188:
    mov x3, x1
    mov x4, x2
    adrp x1, _str_18
    add x1, x1, :lo12:_str_18
    mov x2, #14
    bl __rt_concat
    adrp x3, _str_115
    add x3, x3, :lo12:_str_115
    mov x4, #52
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_dynamic_error_throw_189
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_20
    add x1, x1, :lo12:_str_20
    mov x2, #34
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_dynamic_error_throw_189:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_previous_8_null_175:
    b .L_eir__class_propinit_9_typed_prop_accept_previous_165
.L_eir__class_propinit_9_typed_prop_previous_9_resource_176:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_190
    bl __rt_ob_flush_all
    adrp x1, _str_116
    add x1, x1, :lo12:_str_116
    mov x2, #109
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_190:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_117
    add x9, x9, :lo12:_str_117
    str x9, [x0, #8]
    mov x9, #74
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_previous_10_Closure_177:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_191
    bl __rt_ob_flush_all
    adrp x1, _str_118
    add x1, x1, :lo12:_str_118
    mov x2, #108
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_191:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_119
    add x9, x9, :lo12:_str_119
    str x9, [x0, #8]
    mov x9, #73
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_reject_previous_167:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_9_static_exception_throw_192
    bl __rt_ob_flush_all
    adrp x1, _str_120
    add x1, x1, :lo12:_str_120
    mov x2, #107
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_9_static_exception_throw_192:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_121
    add x9, x9, :lo12:_str_121
    str x9, [x0, #8]
    mov x9, #72
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_9_typed_prop_accept_previous_165:
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    ldr x9, [sp], #16
    str x0, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #40]
    bl __rt_decref_mixed
    ldr x9, [sp], #16
    ldr x0, [sp], #16
    str x0, [x9, #40]
    str xzr, [x9, #48]
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_9_epilogue:
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_9

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_9__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_9__cdylib_exception_cleanup
.type _class_propinit_9__cdylib_exception_cleanup, %function
_class_propinit_9__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_11 symbol=_class_propinit_11 synthetic=1
.align 2

.section .text._class_propinit_11,"ax",@progbits
.globl _class_propinit_11
.type _class_propinit_11, %function
_class_propinit_11:
    // prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_11_stack_ok_194
    b __rt_stack_overflow
.L_eir__class_propinit_11_stack_ok_194:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-56]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-48]
    // param $this from x0
    stur x0, [x29, #-40]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-104]
    adrp x10, _class_propinit_11__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_11__cdylib_exception_cleanup
    stur x10, [x29, #-96]
    stur x29, [x29, #-88]
    mov x10, #0
    stur x10, [x29, #-80]
    stur x10, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    sub x10, x29, #104
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_11_entry_193:
    ldur x0, [x29, #-40]
    stur x0, [x29, #-8]
    adrp x1, _str_4
    add x1, x1, :lo12:_str_4
    mov x2, #0
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    bl __rt_str_persist
    ldr x9, [sp], #16
    stp x1, x2, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    mov x0, #0
    ldr x9, [sp], #16
    str x0, [x9, #24]
    str xzr, [x9, #32]
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, x0
    mov x2, xzr
    mov x0, #8
    bl __rt_mixed_from_value
    stur x0, [x29, #-32]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_mixed_unbox
    cmp x0, #0
    b.eq .L_eir__class_propinit_11_typed_prop_previous_0_int_198
    cmp x0, #1
    b.eq .L_eir__class_propinit_11_typed_prop_previous_1_string_199
    cmp x0, #2
    b.eq .L_eir__class_propinit_11_typed_prop_previous_2_float_200
    cmp x0, #3
    b.eq .L_eir__class_propinit_11_typed_prop_previous_3_bool_201
    cmp x0, #4
    b.eq .L_eir__class_propinit_11_typed_prop_previous_4_array_202
    cmp x0, #5
    b.eq .L_eir__class_propinit_11_typed_prop_previous_5_array_203
    cmp x0, #6
    b.eq .L_eir__class_propinit_11_typed_prop_previous_6_object_204
    cmp x0, #8
    b.eq .L_eir__class_propinit_11_typed_prop_previous_8_null_205
    cmp x0, #9
    b.eq .L_eir__class_propinit_11_typed_prop_previous_9_resource_206
    cmp x0, #10
    b.eq .L_eir__class_propinit_11_typed_prop_previous_10_Closure_207
    b .L_eir__class_propinit_11_typed_prop_reject_previous_197
.L_eir__class_propinit_11_typed_prop_previous_0_int_198:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_208
    bl __rt_ob_flush_all
    adrp x1, _str_122
    add x1, x1, :lo12:_str_122
    mov x2, #108
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_208:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_123
    add x9, x9, :lo12:_str_123
    str x9, [x0, #8]
    mov x9, #73
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_previous_1_string_199:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_209
    bl __rt_ob_flush_all
    adrp x1, _str_124
    add x1, x1, :lo12:_str_124
    mov x2, #111
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_209:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_125
    add x9, x9, :lo12:_str_125
    str x9, [x0, #8]
    mov x9, #76
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_previous_2_float_200:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_210
    bl __rt_ob_flush_all
    adrp x1, _str_126
    add x1, x1, :lo12:_str_126
    mov x2, #110
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_210:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_127
    add x9, x9, :lo12:_str_127
    str x9, [x0, #8]
    mov x9, #75
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_previous_3_bool_201:
    mov x0, x1
    cbz x0, 1f
    b .L_eir__class_propinit_11_typed_prop_bool_true_211
1:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_212
    bl __rt_ob_flush_all
    adrp x1, _str_128
    add x1, x1, :lo12:_str_128
    mov x2, #110
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_212:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_129
    add x9, x9, :lo12:_str_129
    str x9, [x0, #8]
    mov x9, #75
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_bool_true_211:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_213
    bl __rt_ob_flush_all
    adrp x1, _str_130
    add x1, x1, :lo12:_str_130
    mov x2, #109
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_213:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_131
    add x9, x9, :lo12:_str_131
    str x9, [x0, #8]
    mov x9, #74
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_previous_4_array_202:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_214
    bl __rt_ob_flush_all
    adrp x1, _str_132
    add x1, x1, :lo12:_str_132
    mov x2, #110
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_214:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_133
    add x9, x9, :lo12:_str_133
    str x9, [x0, #8]
    mov x9, #75
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_previous_5_array_203:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_215
    bl __rt_ob_flush_all
    adrp x1, _str_132
    add x1, x1, :lo12:_str_132
    mov x2, #110
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_215:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_133
    add x9, x9, :lo12:_str_133
    str x9, [x0, #8]
    mov x9, #75
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_previous_6_object_204:
    ldr x10, [x1]
    mov x11, #1
    cmp x10, x11
    b.eq .L_eir__class_propinit_11_typed_prop_accept_previous_195
    mov x11, #2
    cmp x10, x11
    b.eq .L_eir__class_propinit_11_typed_prop_accept_previous_195
    mov x11, #3
    cmp x10, x11
    b.eq .L_eir__class_propinit_11_typed_prop_accept_previous_195
    mov x11, #4
    cmp x10, x11
    b.eq .L_eir__class_propinit_11_typed_prop_accept_previous_195
    mov x11, #8
    cmp x10, x11
    b.eq .L_eir__class_propinit_11_typed_prop_accept_previous_195
    mov x11, #9
    cmp x10, x11
    b.eq .L_eir__class_propinit_11_typed_prop_accept_previous_195
    mov x11, #11
    cmp x10, x11
    b.eq .L_eir__class_propinit_11_typed_prop_accept_previous_195
    mov x11, #17
    cmp x10, x11
    b.eq .L_eir__class_propinit_11_typed_prop_accept_previous_195
    mov x0, x1
    ldr x9, [x0]
    adrp x10, _class_name_count
    add x10, x10, :lo12:_class_name_count
    ldr x10, [x10]
    cmp x9, x10
    b.hs .L_eir__class_propinit_11_runtime_class_name_fallback_217
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    lsl x11, x9, #4
    add x10, x10, x11
    ldr x1, [x10]
    ldr x2, [x10, #8]
    b .L_eir__class_propinit_11_runtime_class_name_done_218
.L_eir__class_propinit_11_runtime_class_name_fallback_217:
    adrp x1, _str_17
    add x1, x1, :lo12:_str_17
    mov x2, #6
.L_eir__class_propinit_11_runtime_class_name_done_218:
    mov x3, x1
    mov x4, x2
    adrp x1, _str_18
    add x1, x1, :lo12:_str_18
    mov x2, #14
    bl __rt_concat
    adrp x3, _str_134
    add x3, x3, :lo12:_str_134
    mov x4, #56
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_dynamic_error_throw_219
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_20
    add x1, x1, :lo12:_str_20
    mov x2, #34
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_dynamic_error_throw_219:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_previous_8_null_205:
    b .L_eir__class_propinit_11_typed_prop_accept_previous_195
.L_eir__class_propinit_11_typed_prop_previous_9_resource_206:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_220
    bl __rt_ob_flush_all
    adrp x1, _str_135
    add x1, x1, :lo12:_str_135
    mov x2, #113
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_220:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_136
    add x9, x9, :lo12:_str_136
    str x9, [x0, #8]
    mov x9, #78
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_previous_10_Closure_207:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_221
    bl __rt_ob_flush_all
    adrp x1, _str_137
    add x1, x1, :lo12:_str_137
    mov x2, #112
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_221:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_138
    add x9, x9, :lo12:_str_138
    str x9, [x0, #8]
    mov x9, #77
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_reject_previous_197:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_11_static_exception_throw_222
    bl __rt_ob_flush_all
    adrp x1, _str_139
    add x1, x1, :lo12:_str_139
    mov x2, #111
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_11_static_exception_throw_222:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_140
    add x9, x9, :lo12:_str_140
    str x9, [x0, #8]
    mov x9, #76
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_11_typed_prop_accept_previous_195:
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    ldr x9, [sp], #16
    str x0, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #40]
    bl __rt_decref_mixed
    ldr x9, [sp], #16
    ldr x0, [sp], #16
    str x0, [x9, #40]
    str xzr, [x9, #48]
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_11_epilogue:
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_11

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_11__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_11__cdylib_exception_cleanup
.type _class_propinit_11__cdylib_exception_cleanup, %function
_class_propinit_11__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_17 symbol=_class_propinit_17 synthetic=1
.align 2

.section .text._class_propinit_17,"ax",@progbits
.globl _class_propinit_17
.type _class_propinit_17, %function
_class_propinit_17:
    // prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_17_stack_ok_224
    b __rt_stack_overflow
.L_eir__class_propinit_17_stack_ok_224:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-56]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-48]
    // param $this from x0
    stur x0, [x29, #-40]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-104]
    adrp x10, _class_propinit_17__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_17__cdylib_exception_cleanup
    stur x10, [x29, #-96]
    stur x29, [x29, #-88]
    mov x10, #0
    stur x10, [x29, #-80]
    stur x10, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    sub x10, x29, #104
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_17_entry_223:
    ldur x0, [x29, #-40]
    stur x0, [x29, #-8]
    adrp x1, _str_4
    add x1, x1, :lo12:_str_4
    mov x2, #0
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    bl __rt_str_persist
    ldr x9, [sp], #16
    stp x1, x2, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    mov x0, #0
    ldr x9, [sp], #16
    str x0, [x9, #24]
    str xzr, [x9, #32]
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, x0
    mov x2, xzr
    mov x0, #8
    bl __rt_mixed_from_value
    stur x0, [x29, #-32]
    ldur x9, [x29, #-8]
    str x9, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_mixed_unbox
    cmp x0, #0
    b.eq .L_eir__class_propinit_17_typed_prop_previous_0_int_228
    cmp x0, #1
    b.eq .L_eir__class_propinit_17_typed_prop_previous_1_string_229
    cmp x0, #2
    b.eq .L_eir__class_propinit_17_typed_prop_previous_2_float_230
    cmp x0, #3
    b.eq .L_eir__class_propinit_17_typed_prop_previous_3_bool_231
    cmp x0, #4
    b.eq .L_eir__class_propinit_17_typed_prop_previous_4_array_232
    cmp x0, #5
    b.eq .L_eir__class_propinit_17_typed_prop_previous_5_array_233
    cmp x0, #6
    b.eq .L_eir__class_propinit_17_typed_prop_previous_6_object_234
    cmp x0, #8
    b.eq .L_eir__class_propinit_17_typed_prop_previous_8_null_235
    cmp x0, #9
    b.eq .L_eir__class_propinit_17_typed_prop_previous_9_resource_236
    cmp x0, #10
    b.eq .L_eir__class_propinit_17_typed_prop_previous_10_Closure_237
    b .L_eir__class_propinit_17_typed_prop_reject_previous_227
.L_eir__class_propinit_17_typed_prop_previous_0_int_228:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_238
    bl __rt_ob_flush_all
    adrp x1, _str_141
    add x1, x1, :lo12:_str_141
    mov x2, #105
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_238:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_142
    add x9, x9, :lo12:_str_142
    str x9, [x0, #8]
    mov x9, #70
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_previous_1_string_229:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_239
    bl __rt_ob_flush_all
    adrp x1, _str_143
    add x1, x1, :lo12:_str_143
    mov x2, #108
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_239:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_144
    add x9, x9, :lo12:_str_144
    str x9, [x0, #8]
    mov x9, #73
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_previous_2_float_230:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_240
    bl __rt_ob_flush_all
    adrp x1, _str_145
    add x1, x1, :lo12:_str_145
    mov x2, #107
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_240:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_146
    add x9, x9, :lo12:_str_146
    str x9, [x0, #8]
    mov x9, #72
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_previous_3_bool_231:
    mov x0, x1
    cbz x0, 1f
    b .L_eir__class_propinit_17_typed_prop_bool_true_241
1:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_242
    bl __rt_ob_flush_all
    adrp x1, _str_147
    add x1, x1, :lo12:_str_147
    mov x2, #107
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_242:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_148
    add x9, x9, :lo12:_str_148
    str x9, [x0, #8]
    mov x9, #72
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_bool_true_241:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_243
    bl __rt_ob_flush_all
    adrp x1, _str_149
    add x1, x1, :lo12:_str_149
    mov x2, #106
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_243:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_150
    add x9, x9, :lo12:_str_150
    str x9, [x0, #8]
    mov x9, #71
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_previous_4_array_232:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_244
    bl __rt_ob_flush_all
    adrp x1, _str_151
    add x1, x1, :lo12:_str_151
    mov x2, #107
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_244:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_152
    add x9, x9, :lo12:_str_152
    str x9, [x0, #8]
    mov x9, #72
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_previous_5_array_233:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_245
    bl __rt_ob_flush_all
    adrp x1, _str_151
    add x1, x1, :lo12:_str_151
    mov x2, #107
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_245:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_152
    add x9, x9, :lo12:_str_152
    str x9, [x0, #8]
    mov x9, #72
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_previous_6_object_234:
    ldr x10, [x1]
    mov x11, #1
    cmp x10, x11
    b.eq .L_eir__class_propinit_17_typed_prop_accept_previous_225
    mov x11, #2
    cmp x10, x11
    b.eq .L_eir__class_propinit_17_typed_prop_accept_previous_225
    mov x11, #3
    cmp x10, x11
    b.eq .L_eir__class_propinit_17_typed_prop_accept_previous_225
    mov x11, #4
    cmp x10, x11
    b.eq .L_eir__class_propinit_17_typed_prop_accept_previous_225
    mov x11, #8
    cmp x10, x11
    b.eq .L_eir__class_propinit_17_typed_prop_accept_previous_225
    mov x11, #9
    cmp x10, x11
    b.eq .L_eir__class_propinit_17_typed_prop_accept_previous_225
    mov x11, #11
    cmp x10, x11
    b.eq .L_eir__class_propinit_17_typed_prop_accept_previous_225
    mov x11, #17
    cmp x10, x11
    b.eq .L_eir__class_propinit_17_typed_prop_accept_previous_225
    mov x0, x1
    ldr x9, [x0]
    adrp x10, _class_name_count
    add x10, x10, :lo12:_class_name_count
    ldr x10, [x10]
    cmp x9, x10
    b.hs .L_eir__class_propinit_17_runtime_class_name_fallback_247
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    lsl x11, x9, #4
    add x10, x10, x11
    ldr x1, [x10]
    ldr x2, [x10, #8]
    b .L_eir__class_propinit_17_runtime_class_name_done_248
.L_eir__class_propinit_17_runtime_class_name_fallback_247:
    adrp x1, _str_17
    add x1, x1, :lo12:_str_17
    mov x2, #6
.L_eir__class_propinit_17_runtime_class_name_done_248:
    mov x3, x1
    mov x4, x2
    adrp x1, _str_18
    add x1, x1, :lo12:_str_18
    mov x2, #14
    bl __rt_concat
    adrp x3, _str_153
    add x3, x3, :lo12:_str_153
    mov x4, #53
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_dynamic_error_throw_249
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_20
    add x1, x1, :lo12:_str_20
    mov x2, #34
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_dynamic_error_throw_249:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_previous_8_null_235:
    b .L_eir__class_propinit_17_typed_prop_accept_previous_225
.L_eir__class_propinit_17_typed_prop_previous_9_resource_236:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_250
    bl __rt_ob_flush_all
    adrp x1, _str_154
    add x1, x1, :lo12:_str_154
    mov x2, #110
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_250:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_155
    add x9, x9, :lo12:_str_155
    str x9, [x0, #8]
    mov x9, #75
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_previous_10_Closure_237:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_251
    bl __rt_ob_flush_all
    adrp x1, _str_156
    add x1, x1, :lo12:_str_156
    mov x2, #109
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_251:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_157
    add x9, x9, :lo12:_str_157
    str x9, [x0, #8]
    mov x9, #74
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_reject_previous_227:
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir__class_propinit_17_static_exception_throw_252
    bl __rt_ob_flush_all
    adrp x1, _str_158
    add x1, x1, :lo12:_str_158
    mov x2, #108
    mov x0, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir__class_propinit_17_static_exception_throw_252:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_type_error_class_id
    add x9, x9, :lo12:_spl_type_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    adrp x9, _str_159
    add x9, x9, :lo12:_str_159
    str x9, [x0, #8]
    mov x9, #73
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir__class_propinit_17_typed_prop_accept_previous_225:
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    ldr x9, [sp], #16
    str x0, [sp, #-16]!
    str x9, [sp, #-16]!
    ldr x0, [x9, #40]
    bl __rt_decref_mixed
    ldr x9, [sp], #16
    ldr x0, [sp], #16
    str x0, [x9, #40]
    str xzr, [x9, #48]
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_17_epilogue:
    ldur x10, [x29, #-104]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-48]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_17

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_17__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_17__cdylib_exception_cleanup
.type _class_propinit_17__cdylib_exception_cleanup, %function
_class_propinit_17__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_19 symbol=_class_propinit_19 synthetic=1
.align 2

.section .text._class_propinit_19,"ax",@progbits
.globl _class_propinit_19
.type _class_propinit_19, %function
_class_propinit_19:
    // prologue
    sub sp, sp, #112
    stp x29, x30, [sp, #96]
    add x29, sp, #96
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_19_stack_ok_254
    b __rt_stack_overflow
.L_eir__class_propinit_19_stack_ok_254:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-40]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-32]
    // param $this from x0
    stur x0, [x29, #-24]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-88]
    adrp x10, _class_propinit_19__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_19__cdylib_exception_cleanup
    stur x10, [x29, #-80]
    stur x29, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    stur x10, [x29, #-56]
    mov x10, #0
    stur x10, [x29, #-48]
    sub x10, x29, #88
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_19_entry_253:
    ldur x0, [x29, #-24]
    stur x0, [x29, #-8]
    mov x9, x0
    str xzr, [x9, #8]
    movz x10, #0xfffd
    movk x10, #0xffff, lsl #16
    movk x10, #0xffff, lsl #32
    movk x10, #0x7fff, lsl #48
    str x10, [x9, #16]
    ldur x10, [x29, #-88]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-32]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_19_epilogue:
    ldur x10, [x29, #-88]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-32]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_19

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_19__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_19__cdylib_exception_cleanup
.type _class_propinit_19__cdylib_exception_cleanup, %function
_class_propinit_19__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=_class_propinit_20 symbol=_class_propinit_20 synthetic=1
.align 2

.section .text._class_propinit_20,"ax",@progbits
.globl _class_propinit_20
.type _class_propinit_20, %function
_class_propinit_20:
    // prologue
    sub sp, sp, #112
    stp x29, x30, [sp, #96]
    add x29, sp, #96
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir__class_propinit_20_stack_ok_256
    b __rt_stack_overflow
.L_eir__class_propinit_20_stack_ok_256:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-40]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-32]
    // param $this from x0
    stur x0, [x29, #-24]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-88]
    adrp x10, _class_propinit_20__cdylib_exception_cleanup
    add x10, x10, :lo12:_class_propinit_20__cdylib_exception_cleanup
    stur x10, [x29, #-80]
    stur x29, [x29, #-72]
    mov x10, #0
    stur x10, [x29, #-64]
    stur x10, [x29, #-56]
    mov x10, #0
    stur x10, [x29, #-48]
    sub x10, x29, #88
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir__class_propinit_20_entry_255:
    ldur x0, [x29, #-24]
    stur x0, [x29, #-8]
    mov x9, x0
    str xzr, [x9, #8]
    movz x10, #0xfffd
    movk x10, #0xffff, lsl #16
    movk x10, #0xffff, lsl #32
    movk x10, #0x7fff, lsl #48
    str x10, [x9, #16]
    ldur x9, [x29, #-8]
    str xzr, [x9, #24]
    movz x10, #0xfffd
    movk x10, #0xffff, lsl #16
    movk x10, #0xffff, lsl #32
    movk x10, #0x7fff, lsl #48
    str x10, [x9, #32]
    ldur x10, [x29, #-88]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-32]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_class_propinit_20_epilogue:
    ldur x10, [x29, #-88]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-32]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=_class_propinit_20

    // exceptional PHP frame cleanup callback
.section .text._class_propinit_20__cdylib_exception_cleanup,"ax",@progbits
.globl _class_propinit_20__cdylib_exception_cleanup
.type _class_propinit_20__cdylib_exception_cleanup, %function
_class_propinit_20__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
    // @fn name=__elephc_internal_object_compare symbol=_fn__u__u_elephc_u_internal_u_object_u_compare synthetic=1
.align 2

.section .text._fn__u__u_elephc_u_internal_u_object_u_compare,"ax",@progbits
.globl _fn__u__u_elephc_u_internal_u_object_u_compare
.type _fn__u__u_elephc_u_internal_u_object_u_compare, %function
_fn__u__u_elephc_u_internal_u_object_u_compare:
    // prologue
    sub sp, sp, #96
    stp x29, x30, [sp, #80]
    add x29, sp, #80
    // call-stack overflow guard
    adrp x9, _stack_limit
    ldr x9, [x9, :lo12:_stack_limit]
    cmp sp, x9
    b.hs .L_eir___elephc_internal_object_compare_stack_ok_258
    b __rt_stack_overflow
.L_eir___elephc_internal_object_compare_stack_ok_258:
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    stur x10, [x29, #-32]
    // save callee-saved registers required by this frame
    stur x19, [x29, #-24]
    // param $left from x0
    stur x0, [x29, #-8]
    // param $right from x1
    stur x1, [x29, #-16]
    // publish PHP frame activation
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    ldr x10, [x9]
    stur x10, [x29, #-80]
    adrp x10, _fn__u__u_elephc_u_internal_u_object_u_compare__cdylib_exception_cleanup
    add x10, x10, :lo12:_fn__u__u_elephc_u_internal_u_object_u_compare__cdylib_exception_cleanup
    stur x10, [x29, #-72]
    stur x29, [x29, #-64]
    mov x10, #0
    stur x10, [x29, #-56]
    stur x10, [x29, #-48]
    mov x10, #0
    stur x10, [x29, #-40]
    sub x10, x29, #80
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // @block name=entry
.L_eir___elephc_internal_object_compare_entry_257:
    mov x0, #3
    ldur x10, [x29, #-80]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-24]
    mov x15, xzr
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
_fn__u__u_elephc_u_internal_u_object_u_compare_epilogue:
    ldur x10, [x29, #-80]
    adrp x9, _exc_call_frame_top
    add x9, x9, :lo12:_exc_call_frame_top
    str x10, [x9]
    // restore callee-saved registers required by this frame
    ldur x19, [x29, #-24]
    mov x15, #1
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @endfn name=__elephc_internal_object_compare

    // exceptional PHP frame cleanup callback
.section .text._fn__u__u_elephc_u_internal_u_object_u_compare__cdylib_exception_cleanup,"ax",@progbits
.globl _fn__u__u_elephc_u_internal_u_object_u_compare__cdylib_exception_cleanup
.type _fn__u__u_elephc_u_internal_u_object_u_compare__cdylib_exception_cleanup, %function
_fn__u__u_elephc_u_internal_u_object_u_compare__cdylib_exception_cleanup:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x19, [sp, #8]
    mov x19, sp
    mov x29, x0
    mov sp, x19
    ldr x19, [sp, #8]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
.align 2

    // --- lazy materializer: backed enum PropertyHookType ---
.section .text._enum_init_all_PropertyHookType_Get,"ax",@progbits
.globl _enum_init_all_PropertyHookType_Get
.type _enum_init_all_PropertyHookType_Get, %function
_enum_init_all_PropertyHookType_Get:
    // prologue
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, sp
    stp x0, x1, [sp, #-144]!
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    // skip the whole body once the cases already exist
    adrp x9, _enum_case_PropertyHookType_Get
    add x9, x9, :lo12:_enum_case_PropertyHookType_Get
    ldr x0, [x9]
    cbz x0, 1f
    b _enum_init_all_PropertyHookType_Get_done
1:
    // materialize enum singleton PropertyHookType::Get
    mov x0, #40
    bl __rt_heap_alloc
    mov x9, #4
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    mov x10, #20
    str x10, [x0]
    str xzr, [x0, #8]
    str xzr, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    adrp x10, _str_160
    add x10, x10, :lo12:_str_160
    str x10, [x0, #8]
    mov x10, #3
    str x10, [x0, #16]
    adrp x10, _str_161
    add x10, x10, :lo12:_str_161
    str x10, [x0, #24]
    mov x10, #3
    str x10, [x0, #32]
    adrp x9, _enum_case_PropertyHookType_Get
    add x9, x9, :lo12:_enum_case_PropertyHookType_Get
    str x0, [x9]
    // materialize enum singleton PropertyHookType::Set
    mov x0, #40
    bl __rt_heap_alloc
    mov x9, #4
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    mov x10, #20
    str x10, [x0]
    str xzr, [x0, #8]
    str xzr, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    adrp x10, _str_162
    add x10, x10, :lo12:_str_162
    str x10, [x0, #8]
    mov x10, #3
    str x10, [x0, #16]
    adrp x10, _str_163
    add x10, x10, :lo12:_str_163
    str x10, [x0, #24]
    mov x10, #3
    str x10, [x0, #32]
    adrp x9, _enum_case_PropertyHookType_Set
    add x9, x9, :lo12:_enum_case_PropertyHookType_Set
    str x0, [x9]
_enum_init_all_PropertyHookType_Get_done:
    ldp x16, x17, [sp, #128]
    ldp x14, x15, [sp, #112]
    ldp x12, x13, [sp, #96]
    ldp x10, x11, [sp, #80]
    ldp x8, x9, [sp, #64]
    ldp x6, x7, [sp, #48]
    ldp x4, x5, [sp, #32]
    ldp x2, x3, [sp, #16]
    ldp x0, x1, [sp], #144
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
.align 2

    // --- lazy materializer: enum case SortDirection::Ascending ---
.section .text._enum_init_SortDirection_Ascending,"ax",@progbits
.globl _enum_init_SortDirection_Ascending
.type _enum_init_SortDirection_Ascending, %function
_enum_init_SortDirection_Ascending:
    // prologue
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, sp
    stp x0, x1, [sp, #-144]!
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    // skip the whole body once the cases already exist
    adrp x9, _enum_case_SortDirection_Ascending
    add x9, x9, :lo12:_enum_case_SortDirection_Ascending
    ldr x0, [x9]
    cbz x0, 1f
    b _enum_init_SortDirection_Ascending_done
1:
    // materialize enum singleton SortDirection::Ascending
    mov x0, #24
    bl __rt_heap_alloc
    mov x9, #4
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    mov x10, #19
    str x10, [x0]
    str xzr, [x0, #8]
    str xzr, [x0, #16]
    adrp x10, _str_164
    add x10, x10, :lo12:_str_164
    str x10, [x0, #8]
    mov x10, #9
    str x10, [x0, #16]
    adrp x9, _enum_case_SortDirection_Ascending
    add x9, x9, :lo12:_enum_case_SortDirection_Ascending
    str x0, [x9]
_enum_init_SortDirection_Ascending_done:
    ldp x16, x17, [sp, #128]
    ldp x14, x15, [sp, #112]
    ldp x12, x13, [sp, #96]
    ldp x10, x11, [sp, #80]
    ldp x8, x9, [sp, #64]
    ldp x6, x7, [sp, #48]
    ldp x4, x5, [sp, #32]
    ldp x2, x3, [sp, #16]
    ldp x0, x1, [sp], #144
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
.align 2

    // --- lazy materializer: enum case SortDirection::Descending ---
.section .text._enum_init_SortDirection_Descending,"ax",@progbits
.globl _enum_init_SortDirection_Descending
.type _enum_init_SortDirection_Descending, %function
_enum_init_SortDirection_Descending:
    // prologue
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, sp
    stp x0, x1, [sp, #-144]!
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    // skip the whole body once the cases already exist
    adrp x9, _enum_case_SortDirection_Descending
    add x9, x9, :lo12:_enum_case_SortDirection_Descending
    ldr x0, [x9]
    cbz x0, 1f
    b _enum_init_SortDirection_Descending_done
1:
    // materialize enum singleton SortDirection::Descending
    mov x0, #24
    bl __rt_heap_alloc
    mov x9, #4
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    mov x10, #19
    str x10, [x0]
    str xzr, [x0, #8]
    str xzr, [x0, #16]
    adrp x10, _str_165
    add x10, x10, :lo12:_str_165
    str x10, [x0, #8]
    mov x10, #10
    str x10, [x0, #16]
    adrp x9, _enum_case_SortDirection_Descending
    add x9, x9, :lo12:_enum_case_SortDirection_Descending
    str x0, [x9]
_enum_init_SortDirection_Descending_done:
    ldp x16, x17, [sp, #128]
    ldp x14, x15, [sp, #112]
    ldp x12, x13, [sp, #96]
    ldp x10, x11, [sp, #80]
    ldp x8, x9, [sp, #64]
    ldp x6, x7, [sp, #48]
    ldp x4, x5, [sp, #32]
    ldp x2, x3, [sp, #16]
    ldp x0, x1, [sp], #144
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
    // @fn name=main symbol=main
.align 2

.section .text.main,"ax",@progbits
.globl main
.type main, %function
main:
    // prologue
    sub sp, sp, #1088
    mov x9, sp
    add x9, x9, #1072
    stp x29, x30, [x9]
    add x29, sp, #1072
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    ldr x10, [x9]
    sub x9, x29, #1064
    str x10, [x9]
    // save callee-saved registers required by this frame
    sub x9, x29, #1032
    str x21, [x9]
    sub x9, x29, #1040
    str x22, [x9]
    sub x9, x29, #1048
    str x23, [x9]
    sub x9, x29, #1056
    str x19, [x9]
    // save argc/argv to globals
    adrp x9, _global_argc
    add x9, x9, :lo12:_global_argc
    str x0, [x9]
    adrp x9, _global_argv
    add x9, x9, :lo12:_global_argv
    str x1, [x9]
    // publish the call-stack overflow floor for this process
    bl __rt_stack_limit_init
    sub x9, x29, #792
    str xzr, [x9]
    sub x9, x29, #808
    str xzr, [x9]
    sub x9, x29, #816
    str xzr, [x9]
    sub x9, x29, #824
    str xzr, [x9]
    sub x9, x29, #832
    str xzr, [x9]
    adrp x9, _global_argc
    add x9, x9, :lo12:_global_argc
    ldr x0, [x9]
    sub x9, x29, #800
    str x0, [x9]
    bl __rt_mbstring_startup
    bl __rt_mbstring_request_reset
    // @block name=entry
.L_eir_main_entry_259:
    // @src line=2 col=1 end=2:12 op=nop
    // @src line=2 col=15 end=2:16 op=array_new
    mov x0, #4
    mov x1, #8
    bl __rt_array_new
    stur x0, [x29, #-8]
    // @src line=2 col=16 end=2:20 op=const_i64
    // @src line=2 col=16 end=2:20 op=array_push
    cbz x0, .L_eir_main_array_push_indexed_272
    ldr x9, [x0, #-8]
    and x9, x9, #0xff
    cmp x9, #3
    b.eq .L_eir_main_array_push_hash_271
.L_eir_main_array_push_indexed_272:
    stur x0, [x29, #-8]
    mov x1, #65
    ldur x0, [x29, #-8]
    cbz x0, .L_eir_main_array_push_int_slow_274
    ldr w9, [x0, #-12]
    cmp w9, #1
    b.hi .L_eir_main_array_push_int_slow_274
    ldr x9, [x0]
    cbz x9, .L_eir_main_array_push_int_slow_274
    ldr x10, [x0, #8]
    cmp x9, x10
    b.hs .L_eir_main_array_push_int_slow_274
    ldr x10, [x0, #-8]
    ubfx x10, x10, #8, #7
    cbnz x10, .L_eir_main_array_push_int_slow_274
    add x10, x0, #24
    str x1, [x10, x9, lsl #3]
    add x9, x9, #1
    str x9, [x0]
    b .L_eir_main_array_push_int_done_275
.L_eir_main_array_push_int_slow_274:
    bl __rt_array_push_int
.L_eir_main_array_push_int_done_275:
    b .L_eir_main_array_push_done_273
.L_eir_main_array_push_hash_271:
    str x0, [sp, #-16]!
    mov x0, #65
    mov x1, x0
    mov x2, xzr
    mov x0, #0
    bl __rt_mixed_from_value
    mov x1, x0
    ldr x0, [sp], #16
    mov x2, xzr
    mov x3, #7
    bl __rt_hash_append
.L_eir_main_array_push_done_273:
    stur x0, [x29, #-8]
    // @src line=2 col=22 end=2:26 op=const_i64
    // @src line=2 col=22 end=2:26 op=array_push
    cbz x0, .L_eir_main_array_push_indexed_277
    ldr x9, [x0, #-8]
    and x9, x9, #0xff
    cmp x9, #3
    b.eq .L_eir_main_array_push_hash_276
.L_eir_main_array_push_indexed_277:
    stur x0, [x29, #-8]
    mov x1, #169
    ldur x0, [x29, #-8]
    cbz x0, .L_eir_main_array_push_int_slow_279
    ldr w9, [x0, #-12]
    cmp w9, #1
    b.hi .L_eir_main_array_push_int_slow_279
    ldr x9, [x0]
    cbz x9, .L_eir_main_array_push_int_slow_279
    ldr x10, [x0, #8]
    cmp x9, x10
    b.hs .L_eir_main_array_push_int_slow_279
    ldr x10, [x0, #-8]
    ubfx x10, x10, #8, #7
    cbnz x10, .L_eir_main_array_push_int_slow_279
    add x10, x0, #24
    str x1, [x10, x9, lsl #3]
    add x9, x9, #1
    str x9, [x0]
    b .L_eir_main_array_push_int_done_280
.L_eir_main_array_push_int_slow_279:
    bl __rt_array_push_int
.L_eir_main_array_push_int_done_280:
    b .L_eir_main_array_push_done_278
.L_eir_main_array_push_hash_276:
    str x0, [sp, #-16]!
    mov x0, #169
    mov x1, x0
    mov x2, xzr
    mov x0, #0
    bl __rt_mixed_from_value
    mov x1, x0
    ldr x0, [sp], #16
    mov x2, xzr
    mov x3, #7
    bl __rt_hash_append
.L_eir_main_array_push_done_278:
    stur x0, [x29, #-8]
    // @src line=2 col=28 end=2:34 op=const_i64
    // @src line=2 col=28 end=2:34 op=array_push
    cbz x0, .L_eir_main_array_push_indexed_282
    ldr x9, [x0, #-8]
    and x9, x9, #0xff
    cmp x9, #3
    b.eq .L_eir_main_array_push_hash_281
.L_eir_main_array_push_indexed_282:
    stur x0, [x29, #-8]
    mov x1, #8364
    ldur x0, [x29, #-8]
    cbz x0, .L_eir_main_array_push_int_slow_284
    ldr w9, [x0, #-12]
    cmp w9, #1
    b.hi .L_eir_main_array_push_int_slow_284
    ldr x9, [x0]
    cbz x9, .L_eir_main_array_push_int_slow_284
    ldr x10, [x0, #8]
    cmp x9, x10
    b.hs .L_eir_main_array_push_int_slow_284
    ldr x10, [x0, #-8]
    ubfx x10, x10, #8, #7
    cbnz x10, .L_eir_main_array_push_int_slow_284
    add x10, x0, #24
    str x1, [x10, x9, lsl #3]
    add x9, x9, #1
    str x9, [x0]
    b .L_eir_main_array_push_int_done_285
.L_eir_main_array_push_int_slow_284:
    bl __rt_array_push_int
.L_eir_main_array_push_int_done_285:
    b .L_eir_main_array_push_done_283
.L_eir_main_array_push_hash_281:
    str x0, [sp, #-16]!
    mov x0, #8364
    mov x1, x0
    mov x2, xzr
    mov x0, #0
    bl __rt_mixed_from_value
    mov x1, x0
    ldr x0, [sp], #16
    mov x2, xzr
    mov x3, #7
    bl __rt_hash_append
.L_eir_main_array_push_done_283:
    stur x0, [x29, #-8]
    // @src line=2 col=36 end=2:43 op=const_i64
    // @src line=2 col=36 end=2:43 op=array_push
    cbz x0, .L_eir_main_array_push_indexed_287
    ldr x9, [x0, #-8]
    and x9, x9, #0xff
    cmp x9, #3
    b.eq .L_eir_main_array_push_hash_286
.L_eir_main_array_push_indexed_287:
    stur x0, [x29, #-8]
    movz x1, #0xf600
    movk x1, #0x1, lsl #16
    ldur x0, [x29, #-8]
    cbz x0, .L_eir_main_array_push_int_slow_289
    ldr w9, [x0, #-12]
    cmp w9, #1
    b.hi .L_eir_main_array_push_int_slow_289
    ldr x9, [x0]
    cbz x9, .L_eir_main_array_push_int_slow_289
    ldr x10, [x0, #8]
    cmp x9, x10
    b.hs .L_eir_main_array_push_int_slow_289
    ldr x10, [x0, #-8]
    ubfx x10, x10, #8, #7
    cbnz x10, .L_eir_main_array_push_int_slow_289
    add x10, x0, #24
    str x1, [x10, x9, lsl #3]
    add x9, x9, #1
    str x9, [x0]
    b .L_eir_main_array_push_int_done_290
.L_eir_main_array_push_int_slow_289:
    bl __rt_array_push_int
.L_eir_main_array_push_int_done_290:
    b .L_eir_main_array_push_done_288
.L_eir_main_array_push_hash_286:
    str x0, [sp, #-16]!
    movz x0, #0xf600
    movk x0, #0x1, lsl #16
    mov x1, x0
    mov x2, xzr
    mov x0, #0
    bl __rt_mixed_from_value
    mov x1, x0
    ldr x0, [sp], #16
    mov x2, xzr
    mov x3, #7
    bl __rt_hash_append
.L_eir_main_array_push_done_288:
    stur x0, [x29, #-8]
    // @src line=2 col=1 end=2:12 op=acquire
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    stur x0, [x29, #-16]
    // @src line=2 col=1 end=2:12 op=store_local
    sub x9, x29, #792
    str x0, [x9]
    // @src line=2 col=1 end=2:12 op=release
    ldur x0, [x29, #-8]
    bl __rt_decref_array
    // @src line=3 col=1 end=3:7 op=nop
    // @src line=3 col=10 end=3:16 op=const_i64
    // @src line=3 col=19 end=3:24 op=load_local
    sub x9, x29, #800
    ldr x0, [x9]
    mov x21, x0
    // @src line=3 col=17 end=3:24 op=ichecked_add
    movz x0, #0x423f
    movk x0, #0xf, lsl #16
    mov x10, x21
    mov x1, x10
    bl __rt_int_add_checked
    stur x0, [x29, #-32]
    // @src line=3 col=1 end=3:7 op=nop
    // @src line=3 col=1 end=3:7 op=nop
    // @src line=3 col=1 end=3:7 op=acquire
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    stur x0, [x29, #-48]
    // @src line=3 col=1 end=3:7 op=store_local
    sub x9, x29, #808
    str x0, [x9]
    // @src line=3 col=1 end=3:7 op=release
    ldur x0, [x29, #-32]
    bl __rt_decref_mixed
    // @src line=4 col=1 end=4:3 op=nop
    // @src line=4 col=6 end=4:7 op=const_i64
    // @src line=4 col=1 end=4:3 op=store_local
    mov x0, #0
    mov x1, x0
    mov x2, xzr
    mov x0, #0
    bl __rt_mixed_from_value
    sub x9, x29, #816
    str x0, [x9]
    // @src line=5 col=1 end=5:4 op=nop
    // @src line=5 col=6 end=5:8 op=nop
    // @src line=5 col=11 end=5:12 op=const_i64
    // @src line=5 col=6 end=5:8 op=store_local
    mov x0, #0
    sub x9, x29, #952
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #960
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #944
    str x0, [x9]
    sub x0, x29, #960
    sub x9, x29, #824
    str x0, [x9]
    sub x9, x29, #824
    ldr x0, [x9]
    sub x9, x29, #400
    str x0, [x9]
    sub x9, x29, #400
    ldr x0, [x9]
    mov x10, x0
    mov x0, #8
    cbz x10, .L_eir_main_mixed_tag_peeled_291
    ldr x0, [x10]
    cmp x0, #7
    b.ne .L_eir_main_mixed_tag_peeled_291
    mov x0, x10
    bl __rt_mixed_unbox
.L_eir_main_mixed_tag_peeled_291:
    cmp x0, #0
    cset x0, eq
    mov x21, x0
    sub x9, x29, #400
    ldr x0, [x9]
    bl __rt_mixed_unbox
    mov x0, x1
    mov x22, x0
    sub x9, x29, #808
    ldr x0, [x9]
    sub x9, x29, #424
    str x0, [x9]
    sub x9, x29, #424
    ldr x0, [x9]
    mov x10, x0
    mov x0, #8
    cbz x10, .L_eir_main_mixed_tag_peeled_292
    ldr x0, [x10]
    cmp x0, #7
    b.ne .L_eir_main_mixed_tag_peeled_292
    mov x0, x10
    bl __rt_mixed_unbox
.L_eir_main_mixed_tag_peeled_292:
    cmp x0, #0
    cset x0, eq
    mov x23, x0
    mov x0, x21
    cbnz x0, .L_eir_main_counter_bound_guard_264
1:
    // @block name=for.cond.counter.slow
.L_eir_main_for_cond_counter_slow_260:
    // @src line=5 col=14 end=5:16 op=load_local
    sub x9, x29, #824
    ldr x0, [x9]
    stur x0, [x29, #-56]
    // @src line=5 col=19 end=5:25 op=load_local
    sub x9, x29, #808
    ldr x0, [x9]
    stur x0, [x29, #-64]
    // @src line=5 col=17 end=5:25 op=php_rel_cmp
    ldur x0, [x29, #-56]
    ldur x10, [x29, #-64]
    ldr x11, [x0]
    ldr x0, [x0, #8]
    ldr x9, [x10]
    ldr x10, [x10, #8]
    orr x12, x11, x9
    cbz x12, .L_eir_main_numeric_compare_integer_293
    cbnz x11, .L_eir_main_stack_numeric_float_payload_295
    scvtf d0, x0
    b .L_eir_main_stack_numeric_float_ready_296
.L_eir_main_stack_numeric_float_payload_295:
    fmov d0, x0
.L_eir_main_stack_numeric_float_ready_296:
    cbnz x9, .L_eir_main_stack_numeric_float_payload_297
    scvtf d1, x10
    b .L_eir_main_stack_numeric_float_ready_298
.L_eir_main_stack_numeric_float_payload_297:
    fmov d1, x10
.L_eir_main_stack_numeric_float_ready_298:
    fcmp d0, d1
    cset x0, lt
    cset x10, vc
    and x0, x0, x10
    b .L_eir_main_numeric_compare_done_294
.L_eir_main_numeric_compare_integer_293:
    cmp x0, x10
    cset x0, lt
.L_eir_main_numeric_compare_done_294:
    mov x21, x0
    cbz x0, .L_eir_main_for_exit_263
1:
    // @block name=for.body
.L_eir_main_for_body_261:
    // @src line=6 col=5 end=6:13 op=nop
    // @src line=6 col=23 end=6:34 op=load_local
    sub x9, x29, #792
    ldr x0, [x9]
    stur x0, [x29, #-80]
    // @src line=6 col=34 end=6:35 op=nop
    // @src line=6 col=35 end=6:37 op=load_local
    sub x9, x29, #824
    ldr x0, [x9]
    stur x0, [x29, #-88]
    // @src line=6 col=40 end=6:41 op=const_i64
    // @src line=6 col=35 end=6:37 op=cast
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    ldur x0, [x29, #-88]
    ldr x9, [x0]
    cbnz x9, .L_eir_main_frame_numeric_int_cast_slow_299
1:
    ldr x0, [x0, #8]
.L_eir_main_frame_numeric_int_cast_done_300:
    mov x21, x0
    // @src line=6 col=38 end=6:41 op=ibit_and
    and x21, x21, #0x3
    // @src line=6 col=34 end=6:35 op=array_get
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    ldur x9, [x29, #-80]
    cbz x9, .L_eir_main_array_get_null_recv_302
    movz x10, #0xfffe
    movk x10, #0xffff, lsl #16
    movk x10, #0xffff, lsl #32
    movk x10, #0x7fff, lsl #48
    cmp x9, x10
    b.eq .L_eir_main_array_get_null_recv_302
    ldr x10, [x9, #-8]
    and x10, x10, #0xff
    cmp x10, #3
    b.eq .L_eir_main_array_get_promoted_305
    mov x0, x21
    ldur x9, [x29, #-80]
    cmp x0, #0
    b.lt .L_eir_main_array_get_null_301
    ldr x10, [x9]
    cmp x0, x10
    b.ge .L_eir_main_array_get_null_301
    ldr x11, [x9, #-8]
    ubfx x11, x11, #8, #7
    cmp x11, #7
    b.ne .L_eir_main_array_get_typed_payload_306
    add x9, x9, #24
    ldr x0, [x9, x0, lsl #3]
    bl __rt_mixed_unbox
    mov x0, x1
    mov x1, #0
    b .L_eir_main_array_get_widened_done_307
.L_eir_main_array_get_typed_payload_306:
    add x9, x9, #24
    ldr x0, [x9, x0, lsl #3]
    mov x1, #0
.L_eir_main_array_get_widened_done_307:
    b .L_eir_main_array_get_done_304
.L_eir_main_array_get_promoted_305:
    mov x1, x21
    mov x2, #-1
    ldur x0, [x29, #-80]
    bl __rt_hash_get
    cbz x0, .L_eir_main_array_get_null_301
    cmp x3, #7
    b.ne .L_eir_main_hash_get_concrete_payload_308
    mov x0, x1
    bl __rt_mixed_unbox
.L_eir_main_hash_get_concrete_payload_308:
    mov x0, x1
    mov x1, #0
    b .L_eir_main_array_get_done_304
.L_eir_main_array_get_null_301:
    str x0, [sp, #-16]!
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    ldr x0, [sp], #16
    bl __rt_warn_undefined_array_key_int
    b .L_eir_main_array_get_fallback_303
.L_eir_main_array_get_null_recv_302:
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    bl __rt_warn_array_offset_on_null
.L_eir_main_array_get_fallback_303:
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, #8
.L_eir_main_array_get_done_304:
    stur x0, [x29, #-136]
    stur x1, [x29, #-128]
    // @src line=6 col=34 end=6:35 op=nop
    // @src line=6 col=34 end=6:35 op=nop
    // @src line=6 col=44 op=const_str
    adrp x1, _str_166
    add x1, x1, :lo12:_str_166
    mov x2, #5
    bl __rt_mbstring_ini_literal
    stur x1, [x29, #-152]
    stur x2, [x29, #-144]
    // @src line=6 col=44 op=acquire
    ldur x1, [x29, #-152]
    ldur x2, [x29, #-144]
    stur x1, [x29, #-168]
    stur x2, [x29, #-160]
    // @src line=6 col=44 op=runtime_call
    ldur x1, [x29, #-168]
    ldur x2, [x29, #-160]
    sub x9, x29, #968
    str x1, [x9]
    adrp x10, __rt_exception_release_owned
    add x10, x10, :lo12:__rt_exception_release_owned
    sub x9, x29, #984
    str x10, [x9]
    mov x0, #0
    mov x1, x0
    sub x0, x29, #992
    bl __rt_exception_guard_owned
    mov x21, x0
    // @src line=6 col=16 end=6:52 op=runtime_call
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    ldur x0, [x29, #-136]
    ldur x1, [x29, #-128]
    cmp x1, #8
    csel x0, xzr, x0, eq
    bl __rt_mb_chr_utf8
    cbz x1, .L_eir_main_mb_chr_invalid_309
    stp x1, x2, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    mov x9, #5
    str x9, [x0, #-8]
    mov x10, #1
    str x10, [x0]
    ldp x11, x12, [sp], #16
    stp x11, x12, [x0, #8]
    b .L_eir_main_mb_chr_fast_done_310
.L_eir_main_mb_chr_invalid_309:
    mov x0, #3
    mov x1, #0
    mov x2, #0
    bl __rt_mixed_from_value
.L_eir_main_mb_chr_fast_done_310:
    stur x0, [x29, #-184]
    // @src line=6 col=16 end=6:52 op=runtime_call
    mov x0, x21
    bl __rt_exception_unguard_owned
    // @src line=6 col=16 end=6:52 op=release
    ldur x1, [x29, #-168]
    ldur x2, [x29, #-160]
    mov x0, x1
    bl __rt_heap_free_safe
    // @src line=6 col=5 end=6:13 op=mixed_clone
    ldur x0, [x29, #-184]
    bl __rt_mixed_clone
    stur x0, [x29, #-192]
    // @src line=6 col=5 end=6:13 op=release
    ldur x0, [x29, #-184]
    bl __rt_decref_mixed
    // @src line=6 col=5 end=6:13 op=acquire
    ldur x0, [x29, #-192]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    stur x0, [x29, #-200]
    // @src line=6 col=5 end=6:13 op=release_local_slot
    sub x9, x29, #832
    ldr x0, [x9]
    cbz x0, .L_eir_main_release_local_slot_done_311
1:
    sub x9, x29, #832
    str xzr, [x9]
    cbz x0, .L_eir_main_release_local_slot_done_311
    movz x10, #0xfffe
    movk x10, #0xffff, lsl #16
    movk x10, #0xffff, lsl #32
    movk x10, #0x7fff, lsl #48
    cmp x0, x10
    b.eq .L_eir_main_release_local_slot_done_311
    ldr x10, [x0]
    cmp x10, #3
    b.ls .L_eir_main_release_local_slot_scalar_mixed_312
    cmp x10, #8
    b.eq .L_eir_main_release_local_slot_scalar_mixed_312
    mov x1, x0
    adrp x0, __rt_decref_mixed
    add x0, x0, :lo12:__rt_decref_mixed
    bl __rt_cleanup_preserve_exception
    cbz x0, 1f
    b __rt_throw_current
1:
    b .L_eir_main_release_local_slot_done_311
.L_eir_main_release_local_slot_scalar_mixed_312:
    bl __rt_decref_mixed
.L_eir_main_release_local_slot_done_311:
    // @src line=6 col=5 end=6:13 op=store_local
    ldur x0, [x29, #-200]
    sub x9, x29, #832
    str x0, [x9]
    // @src line=6 col=5 end=6:13 op=release
    ldur x0, [x29, #-192]
    bl __rt_decref_mixed
    // @src line=7 col=5 end=7:7 op=concat_reset
    sub x9, x29, #1064
    ldr x10, [x9]
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    str x10, [x9]
    // @src line=7 col=5 end=7:7 op=load_local
    sub x9, x29, #816
    ldr x0, [x9]
    stur x0, [x29, #-208]
    // @src line=7 col=18 end=7:26 op=load_local
    sub x9, x29, #832
    ldr x0, [x9]
    stur x0, [x29, #-216]
    // @src line=7 col=18 end=7:26 op=cast
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #7
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    ldur x0, [x29, #-216]
    bl _eir_shared_mixed_to_string
    stur x15, [x29, #-240]
    stur x1, [x29, #-232]
    stur x2, [x29, #-224]
    // @src line=7 col=11 end=7:27 op=str_len
    ldur x1, [x29, #-232]
    ldur x2, [x29, #-224]
    mov x0, x2
    mov x21, x0
    // @src line=7 col=11 end=7:27 op=release
    ldur x1, [x29, #-232]
    ldur x2, [x29, #-224]
    mov x0, x1
    bl __rt_heap_free_safe
    // @src line=7 col=34 end=7:42 op=load_local
    sub x9, x29, #832
    ldr x0, [x9]
    sub x9, x29, #256
    str x0, [x9]
    // @src line=7 col=43 end=7:44 op=const_i64
    // @src line=7 col=42 end=7:43 op=const_bool
    // @src line=7 col=42 end=7:43 op=runtime_call
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #7
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    mov x1, #0
    mov x2, #-1
    mov x3, #1
    sub x9, x29, #256
    ldr x0, [x9]
    bl __rt_mixed_array_get
    sub x9, x29, #264
    str x0, [x9]
    // @src line=7 col=42 end=7:43 op=cast
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #7
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    sub x9, x29, #264
    ldr x0, [x9]
    bl _eir_shared_mixed_to_string
    sub x9, x29, #288
    str x15, [x9]
    sub x9, x29, #280
    str x1, [x9]
    sub x9, x29, #272
    str x2, [x9]
    // @src line=7 col=42 end=7:43 op=release
    sub x9, x29, #264
    ldr x0, [x9]
    bl __rt_decref_mixed
    // @src line=7 col=30 end=7:46 op=runtime_call
    sub x9, x29, #280
    ldr x1, [x9]
    sub x9, x29, #272
    ldr x2, [x9]
    cbz x2, .L_eir_main_ord_empty_313
    ldrb w0, [x1]
    b .L_eir_main_ord_done_314
.L_eir_main_ord_empty_313:
    mov x0, #0
.L_eir_main_ord_done_314:
    mov x23, x0
    // @src line=7 col=30 end=7:46 op=release
    sub x9, x29, #280
    ldr x1, [x9]
    sub x9, x29, #272
    ldr x2, [x9]
    mov x0, x1
    bl __rt_heap_free_safe
    // @src line=7 col=28 end=7:46 op=ichecked_add
    mov x0, x21
    mov x10, x23
    adds x11, x0, x10
    b.vs .L_eir_main_stack_numeric_overflow_315
    mov x0, x11
    sub x9, x29, #856
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #864
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #848
    str x0, [x9]
    b .L_eir_main_stack_numeric_done_316
.L_eir_main_stack_numeric_overflow_315:
    scvtf d0, x0
    scvtf d1, x10
    fadd d0, d0, d1
    fmov x0, d0
    sub x9, x29, #856
    str x0, [x9]
    mov x0, #2
    sub x9, x29, #864
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #848
    str x0, [x9]
.L_eir_main_stack_numeric_done_316:
    sub x0, x29, #864
    sub x9, x29, #304
    str x0, [x9]
    // @src line=7 col=5 end=7:7 op=mixed_numeric_binop
    ldur x0, [x29, #-208]
    str x0, [sp, #-16]!
    sub x9, x29, #304
    ldr x0, [x9]
    str x0, [sp, #-16]!
    ldr x0, [sp, #16]
    ldr x1, [sp]
    bl __rt_mixed_numeric_add
    str x0, [sp, #-16]!
    ldr x0, [sp], #16
    add sp, sp, #32
    sub x9, x29, #312
    str x0, [x9]
    // @src line=7 col=5 end=7:7 op=release
    // @src line=7 col=5 end=7:7 op=nop
    // @src line=7 col=5 end=7:7 op=nop
    // @src line=7 col=5 end=7:7 op=acquire
    sub x9, x29, #312
    ldr x0, [x9]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    sub x9, x29, #328
    str x0, [x9]
    // @src line=7 col=5 end=7:7 op=release_local_slot
    sub x9, x29, #816
    ldr x0, [x9]
    cbz x0, .L_eir_main_release_local_slot_done_317
1:
    sub x9, x29, #816
    str xzr, [x9]
    bl __rt_decref_mixed
.L_eir_main_release_local_slot_done_317:
    // @src line=7 col=5 end=7:7 op=store_local
    sub x9, x29, #328
    ldr x0, [x9]
    sub x9, x29, #816
    str x0, [x9]
    // @src line=7 col=5 end=7:7 op=release
    sub x9, x29, #312
    ldr x0, [x9]
    bl __rt_decref_mixed
    // @src line=5 col=27 end=5:29 op=concat_reset
    sub x9, x29, #1064
    ldr x10, [x9]
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    str x10, [x9]
    // @src line=5 col=27 end=5:29 op=load_local
    sub x9, x29, #824
    ldr x0, [x9]
    sub x9, x29, #336
    str x0, [x9]
    // @src line=5 col=27 end=5:29 op=nop
    // @src line=5 col=27 end=5:29 op=nop
    // @src line=5 col=27 end=5:29 op=str_inc_dec
    sub x9, x29, #336
    ldr x0, [x9]
    ldr x11, [x0]
    ldr x0, [x0, #8]
    mov x10, #1
    mov x9, #0
    orr x12, x11, x9
    cbz x12, .L_eir_main_stack_numeric_integer_318
    cbnz x11, .L_eir_main_stack_numeric_float_payload_321
    scvtf d0, x0
    b .L_eir_main_stack_numeric_float_ready_322
.L_eir_main_stack_numeric_float_payload_321:
    fmov d0, x0
.L_eir_main_stack_numeric_float_ready_322:
    scvtf d1, x10
.L_eir_main_stack_numeric_float_ready_324:
    fadd d0, d0, d1
    fmov x0, d0
    sub x9, x29, #880
    str x0, [x9]
    mov x0, #2
    sub x9, x29, #888
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #872
    str x0, [x9]
    b .L_eir_main_stack_numeric_done_320
.L_eir_main_stack_numeric_integer_318:
    adds x11, x0, x10
    b.vs .L_eir_main_stack_numeric_overflow_319
    mov x0, x11
    sub x9, x29, #880
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #888
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #872
    str x0, [x9]
    b .L_eir_main_stack_numeric_done_320
.L_eir_main_stack_numeric_overflow_319:
    scvtf d0, x0
    scvtf d1, x10
    fadd d0, d0, d1
    fmov x0, d0
    sub x9, x29, #880
    str x0, [x9]
    mov x0, #2
    sub x9, x29, #888
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #872
    str x0, [x9]
.L_eir_main_stack_numeric_done_320:
    sub x0, x29, #888
    sub x9, x29, #360
    str x0, [x9]
    // @src line=5 col=27 end=5:29 op=acquire
    sub x9, x29, #360
    ldr x0, [x9]
    sub x9, x29, #368
    str x0, [x9]
    // @src line=5 col=27 end=5:29 op=release_local_slot
    sub x9, x29, #824
    ldr x0, [x9]
    cbz x0, .L_eir_main_numeric_local_cleanup_done_325
1:
    sub x9, x29, #824
    str xzr, [x9]
    sub x10, x29, #864
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_325
1:
    sub x10, x29, #888
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_325
1:
    sub x10, x29, #912
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_325
1:
    sub x10, x29, #936
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_325
1:
    bl __rt_decref_mixed
.L_eir_main_numeric_local_cleanup_done_325:
    // @src line=5 col=27 end=5:29 op=store_local
    sub x9, x29, #368
    ldr x0, [x9]
    sub x9, x29, #824
    str x0, [x9]
    // @src line=5 col=27 end=5:29 op=release
    // @src line=5 col=27 end=5:29 op=nop
    b .L_eir_main_for_cond_counter_slow_260
    // @block name=for.exit
.L_eir_main_for_exit_263:
    // @src line=9 col=1 end=9:5 op=nop
    // @src line=9 col=1 end=9:5 op=nop
    // @src line=9 col=6 end=9:8 op=load_local
    sub x9, x29, #816
    ldr x0, [x9]
    sub x9, x29, #376
    str x0, [x9]
    // @src line=9 col=1 end=9:5 op=echo_value
    sub x9, x29, #376
    ldr x0, [x9]
    str x0, [sp, #-16]!
    bl __rt_mixed_unbox
    cmp x0, #6
    b.eq .L_eir_main_mixed_string_object_326
    ldr x0, [sp], #16
    bl __rt_mixed_write_stdout
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_object_326:
    ldr x10, [sp], #16
    mov x19, x1
    ldr x9, [x19]
    mov x10, #1
    cmp x9, x10
    b.eq .L_eir_main_mixed_string_Error_329
    mov x10, #2
    cmp x9, x10
    b.eq .L_eir_main_mixed_string_TypeError_330
    mov x10, #3
    cmp x9, x10
    b.eq .L_eir_main_mixed_string_ArgumentCountError_331
    mov x10, #4
    cmp x9, x10
    b.eq .L_eir_main_mixed_string_ArithmeticError_332
    mov x10, #8
    cmp x9, x10
    b.eq .L_eir_main_mixed_string_DivisionByZeroError_333
    mov x10, #9
    cmp x9, x10
    b.eq .L_eir_main_mixed_string_Exception_334
    mov x10, #11
    cmp x9, x10
    b.eq .L_eir_main_mixed_string_JsonException_335
    mov x10, #17
    cmp x9, x10
    b.eq .L_eir_main_mixed_string_ValueError_336
    b .L_eir_main_mixed_string_no_match_327
.L_eir_main_mixed_string_Error_329:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_TypeError_330:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_ArgumentCountError_331:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_ArithmeticError_332:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_DivisionByZeroError_333:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_Exception_334:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_JsonException_335:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_ValueError_336:
    mov x0, x19
    str x0, [sp, #-16]!
    ldr x0, [sp]
    add sp, sp, #16
    ldr x10, [x0]
    adrp x9, _class_source_vtable_ptrs
    add x9, x9, :lo12:_class_source_vtable_ptrs
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9, #64]
    blr x9
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write
    b .L_eir_main_mixed_string_done_328
.L_eir_main_mixed_string_no_match_327:
    ldr x9, [x19]
    adrp x10, _class_name_entries
    add x10, x10, :lo12:_class_name_entries
    add x10, x10, x9, lsl #4
    ldr x1, [x10]
    ldr x2, [x10, #8]
    mov x3, x1
    mov x4, x2
    adrp x1, _str_0
    add x1, x1, :lo12:_str_0
    mov x2, #16
    bl __rt_concat
    adrp x3, _str_1
    add x3, x3, :lo12:_str_1
    mov x4, #33
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    adrp x9, _exc_handler_top
    add x9, x9, :lo12:_exc_handler_top
    ldr x9, [x9]
    cbnz x9, .L_eir_main_dynamic_error_throw_337
    bl __rt_ob_flush_all
    mov x0, #1
    adrp x1, _str_2
    add x1, x1, :lo12:_str_2
    mov x2, #30
    mov x8, #64
    svc #0
    mov x0, #1
    ldr x1, [sp]
    ldr x2, [sp, #8]
    mov x8, #64
    svc #0
    mov x0, #1
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    mov x8, #64
    svc #0
    bl __rt_ob_flush_all
    mov x0, #255
    mov x8, #94
    svc #0
.L_eir_main_dynamic_error_throw_337:
    mov x0, #56
    bl __rt_heap_alloc
    mov x9, #6
    str x9, [x0, #-8]
    bl __rt_object_handle_acquire
    adrp x9, _spl_error_class_id
    add x9, x9, :lo12:_spl_error_class_id
    ldr x9, [x9]
    str x9, [x0]
    ldr x9, [sp]
    str x9, [x0, #8]
    ldr x9, [sp, #8]
    str x9, [x0, #16]
    str xzr, [x0, #24]
    str xzr, [x0, #32]
    str xzr, [x0, #40]
    add sp, sp, #16
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    str x0, [x9]
    b __rt_throw_current
.L_eir_main_mixed_string_done_328:
    // @src line=9 col=1 end=9:5 op=concat_reset
    sub x9, x29, #1064
    ldr x10, [x9]
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    str x10, [x9]
    // @src line=9 col=10 op=const_str
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    bl __rt_mbstring_ini_literal
    sub x9, x29, #392
    str x1, [x9]
    sub x9, x29, #384
    str x2, [x9]
    // @src line=9 col=1 end=9:5 op=echo_value
    sub x9, x29, #392
    ldr x1, [x9]
    sub x9, x29, #384
    ldr x2, [x9]

    // echo
    mov x0, x1
    mov x1, x2
    bl __rt_stdout_write

    // epilogue + exit(0)
    bl __rt_ob_flush_all
    // epilogue cleanup $codepoints
    sub x9, x29, #792
    ldr x0, [x9]
    cbz x0, .L_eir_main_main_refcounted_cleanup_done_338
    sub x9, x29, #792
    str xzr, [x9]
    bl __rt_decref_array
.L_eir_main_main_refcounted_cleanup_done_338:
    // epilogue cleanup $limit
    sub x9, x29, #808
    ldr x0, [x9]
    cbz x0, .L_eir_main_main_refcounted_cleanup_done_339
    sub x9, x29, #808
    str xzr, [x9]
    bl __rt_decref_mixed
.L_eir_main_main_refcounted_cleanup_done_339:
    // epilogue cleanup $n
    sub x9, x29, #816
    ldr x0, [x9]
    cbz x0, .L_eir_main_main_refcounted_cleanup_done_340
    sub x9, x29, #816
    str xzr, [x9]
    bl __rt_decref_mixed
.L_eir_main_main_refcounted_cleanup_done_340:
    // epilogue cleanup $i
    sub x9, x29, #824
    ldr x0, [x9]
    cbz x0, .L_eir_main_numeric_local_cleanup_done_341
1:
    sub x9, x29, #824
    str xzr, [x9]
    sub x10, x29, #864
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_341
1:
    sub x10, x29, #888
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_341
1:
    sub x10, x29, #912
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_341
1:
    sub x10, x29, #936
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_341
1:
    bl __rt_decref_mixed
.L_eir_main_numeric_local_cleanup_done_341:
    // epilogue cleanup $encoded
    sub x9, x29, #832
    ldr x0, [x9]
    cbz x0, .L_eir_main_main_refcounted_cleanup_done_342
    sub x9, x29, #832
    str xzr, [x9]
    bl __rt_decref_mixed
.L_eir_main_main_refcounted_cleanup_done_342:
    // shutdown: collect cycles so their destructors run, as PHP's shutdown does
    bl __rt_gc_collect_cycles_explicit
    adrp x9, _exc_value
    add x9, x9, :lo12:_exc_value
    ldr x0, [x9]
    cbz x0, 1f
    b __rt_throw_current
1:
    bl __rt_mbstring_release_catalog
    bl __rt_stream_registry_cleanup
    // restore callee-saved registers required by this frame
    sub x9, x29, #1032
    ldr x21, [x9]
    sub x9, x29, #1040
    ldr x22, [x9]
    sub x9, x29, #1048
    ldr x23, [x9]
    sub x9, x29, #1056
    ldr x19, [x9]
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    bl __rt_ob_flush_all
    mov x0, #0
    mov x8, #94
    svc #0
    // @block name=counter.bound.guard
.L_eir_main_counter_bound_guard_264:
    mov x0, x23
    cbz x0, .L_eir_main_for_cond_counter_slow_260
1:
    // @block name=counter.integer.entry
.L_eir_main_counter_integer_entry_265:
    sub x9, x29, #424
    ldr x0, [x9]
    bl __rt_mixed_unbox
    mov x0, x1
    mov x23, x0
    mov x0, x22
    sub x9, x29, #840
    str x0, [x9]
    // @block name=counter.integer.ready
.L_eir_main_counter_integer_ready_266:
    // @src line=6 col=40 end=6:41 op=const_i64 origin=licm
    // @src line=6 col=44 op=const_str origin=licm
    adrp x1, _str_166
    add x1, x1, :lo12:_str_166
    mov x2, #5
    bl __rt_mbstring_ini_literal
    sub x9, x29, #544
    str x1, [x9]
    sub x9, x29, #536
    str x2, [x9]
    // @src line=7 col=43 end=7:44 op=const_i64 origin=licm
    // @src line=7 col=42 end=7:43 op=const_bool origin=licm
    // @src line=5 col=27 end=5:29 op=nop origin=licm
    // @block name=for.cond.counter.fast
.L_eir_main_for_cond_counter_fast_267:
    // @src line=5 col=14 end=5:16 op=load_local
    sub x9, x29, #840
    ldr x0, [x9]
    mov x22, x0
    // @src line=5 col=19 end=5:25 op=load_local
    sub x9, x29, #808
    ldr x0, [x9]
    sub x9, x29, #456
    str x0, [x9]
    // @src line=5 col=17 end=5:25 op=icmp
    cmp x22, x23
    b.ge .L_eir_main_counter_integer_exit_270
1:
    // @block name=for.body.counter.fast
.L_eir_main_for_body_counter_fast_268:
    // @src line=6 col=5 end=6:13 op=nop
    // @src line=6 col=23 end=6:34 op=load_local
    sub x9, x29, #792
    ldr x0, [x9]
    sub x9, x29, #472
    str x0, [x9]
    // @src line=6 col=34 end=6:35 op=nop
    // @src line=6 col=35 end=6:37 op=load_local
    sub x9, x29, #840
    ldr x0, [x9]
    mov x22, x0
    // @src line=6 col=35 end=6:37 op=cast
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    mov x0, x22
    // @src line=6 col=38 end=6:41 op=ibit_and
    and x22, x22, #0x3
    // @src line=6 col=34 end=6:35 op=array_get
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    sub x9, x29, #472
    ldr x9, [x9]
    cbz x9, .L_eir_main_array_get_null_recv_344
    movz x10, #0xfffe
    movk x10, #0xffff, lsl #16
    movk x10, #0xffff, lsl #32
    movk x10, #0x7fff, lsl #48
    cmp x9, x10
    b.eq .L_eir_main_array_get_null_recv_344
    ldr x10, [x9, #-8]
    and x10, x10, #0xff
    cmp x10, #3
    b.eq .L_eir_main_array_get_promoted_347
    mov x0, x22
    sub x9, x29, #472
    ldr x9, [x9]
    cmp x0, #0
    b.lt .L_eir_main_array_get_null_343
    ldr x10, [x9]
    cmp x0, x10
    b.ge .L_eir_main_array_get_null_343
    ldr x11, [x9, #-8]
    ubfx x11, x11, #8, #7
    cmp x11, #7
    b.ne .L_eir_main_array_get_typed_payload_348
    add x9, x9, #24
    ldr x0, [x9, x0, lsl #3]
    bl __rt_mixed_unbox
    mov x0, x1
    mov x1, #0
    b .L_eir_main_array_get_widened_done_349
.L_eir_main_array_get_typed_payload_348:
    add x9, x9, #24
    ldr x0, [x9, x0, lsl #3]
    mov x1, #0
.L_eir_main_array_get_widened_done_349:
    b .L_eir_main_array_get_done_346
.L_eir_main_array_get_promoted_347:
    mov x1, x22
    mov x2, #-1
    sub x9, x29, #472
    ldr x0, [x9]
    bl __rt_hash_get
    cbz x0, .L_eir_main_array_get_null_343
    cmp x3, #7
    b.ne .L_eir_main_hash_get_concrete_payload_350
    mov x0, x1
    bl __rt_mixed_unbox
.L_eir_main_hash_get_concrete_payload_350:
    mov x0, x1
    mov x1, #0
    b .L_eir_main_array_get_done_346
.L_eir_main_array_get_null_343:
    str x0, [sp, #-16]!
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    ldr x0, [sp], #16
    bl __rt_warn_undefined_array_key_int
    b .L_eir_main_array_get_fallback_345
.L_eir_main_array_get_null_recv_344:
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    bl __rt_warn_array_offset_on_null
.L_eir_main_array_get_fallback_345:
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    mov x1, #8
.L_eir_main_array_get_done_346:
    sub x9, x29, #528
    str x0, [x9]
    sub x9, x29, #520
    str x1, [x9]
    // @src line=6 col=34 end=6:35 op=nop
    // @src line=6 col=34 end=6:35 op=nop
    // @src line=6 col=44 op=acquire
    sub x9, x29, #544
    ldr x1, [x9]
    sub x9, x29, #536
    ldr x2, [x9]
    sub x9, x29, #560
    str x1, [x9]
    sub x9, x29, #552
    str x2, [x9]
    // @src line=6 col=44 op=runtime_call
    sub x9, x29, #560
    ldr x1, [x9]
    sub x9, x29, #552
    ldr x2, [x9]
    sub x9, x29, #1000
    str x1, [x9]
    adrp x10, __rt_exception_release_owned
    add x10, x10, :lo12:__rt_exception_release_owned
    sub x9, x29, #1016
    str x10, [x9]
    mov x0, #0
    mov x1, x0
    sub x0, x29, #1024
    bl __rt_exception_guard_owned
    mov x22, x0
    // @src line=6 col=16 end=6:52 op=runtime_call
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #6
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    sub x9, x29, #528
    ldr x0, [x9]
    sub x9, x29, #520
    ldr x1, [x9]
    cmp x1, #8
    csel x0, xzr, x0, eq
    bl __rt_mb_chr_utf8
    cbz x1, .L_eir_main_mb_chr_invalid_351
    stp x1, x2, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    mov x9, #5
    str x9, [x0, #-8]
    mov x10, #1
    str x10, [x0]
    ldp x11, x12, [sp], #16
    stp x11, x12, [x0, #8]
    b .L_eir_main_mb_chr_fast_done_352
.L_eir_main_mb_chr_invalid_351:
    mov x0, #3
    mov x1, #0
    mov x2, #0
    bl __rt_mixed_from_value
.L_eir_main_mb_chr_fast_done_352:
    sub x9, x29, #576
    str x0, [x9]
    // @src line=6 col=16 end=6:52 op=runtime_call
    mov x0, x22
    bl __rt_exception_unguard_owned
    // @src line=6 col=16 end=6:52 op=release
    sub x9, x29, #560
    ldr x1, [x9]
    sub x9, x29, #552
    ldr x2, [x9]
    mov x0, x1
    bl __rt_heap_free_safe
    // @src line=6 col=5 end=6:13 op=mixed_clone
    sub x9, x29, #576
    ldr x0, [x9]
    bl __rt_mixed_clone
    sub x9, x29, #584
    str x0, [x9]
    // @src line=6 col=5 end=6:13 op=release
    sub x9, x29, #576
    ldr x0, [x9]
    bl __rt_decref_mixed
    // @src line=6 col=5 end=6:13 op=acquire
    sub x9, x29, #584
    ldr x0, [x9]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    sub x9, x29, #592
    str x0, [x9]
    // @src line=6 col=5 end=6:13 op=release_local_slot
    sub x9, x29, #832
    ldr x0, [x9]
    cbz x0, .L_eir_main_release_local_slot_done_353
1:
    sub x9, x29, #832
    str xzr, [x9]
    cbz x0, .L_eir_main_release_local_slot_done_353
    movz x10, #0xfffe
    movk x10, #0xffff, lsl #16
    movk x10, #0xffff, lsl #32
    movk x10, #0x7fff, lsl #48
    cmp x0, x10
    b.eq .L_eir_main_release_local_slot_done_353
    ldr x10, [x0]
    cmp x10, #3
    b.ls .L_eir_main_release_local_slot_scalar_mixed_354
    cmp x10, #8
    b.eq .L_eir_main_release_local_slot_scalar_mixed_354
    mov x1, x0
    adrp x0, __rt_decref_mixed
    add x0, x0, :lo12:__rt_decref_mixed
    bl __rt_cleanup_preserve_exception
    cbz x0, 1f
    b __rt_throw_current
1:
    b .L_eir_main_release_local_slot_done_353
.L_eir_main_release_local_slot_scalar_mixed_354:
    bl __rt_decref_mixed
.L_eir_main_release_local_slot_done_353:
    // @src line=6 col=5 end=6:13 op=store_local
    sub x9, x29, #592
    ldr x0, [x9]
    sub x9, x29, #832
    str x0, [x9]
    // @src line=6 col=5 end=6:13 op=release
    sub x9, x29, #584
    ldr x0, [x9]
    bl __rt_decref_mixed
    // @src line=7 col=5 end=7:7 op=concat_reset
    sub x9, x29, #1064
    ldr x10, [x9]
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    str x10, [x9]
    // @src line=7 col=5 end=7:7 op=load_local
    sub x9, x29, #816
    ldr x0, [x9]
    sub x9, x29, #600
    str x0, [x9]
    // @src line=7 col=18 end=7:26 op=load_local
    sub x9, x29, #832
    ldr x0, [x9]
    sub x9, x29, #608
    str x0, [x9]
    // @src line=7 col=18 end=7:26 op=cast
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #7
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    sub x9, x29, #608
    ldr x0, [x9]
    bl _eir_shared_mixed_to_string
    sub x9, x29, #632
    str x15, [x9]
    sub x9, x29, #624
    str x1, [x9]
    sub x9, x29, #616
    str x2, [x9]
    // @src line=7 col=11 end=7:27 op=str_len
    sub x9, x29, #624
    ldr x1, [x9]
    sub x9, x29, #616
    ldr x2, [x9]
    mov x0, x2
    mov x22, x0
    // @src line=7 col=11 end=7:27 op=release
    sub x9, x29, #624
    ldr x1, [x9]
    sub x9, x29, #616
    ldr x2, [x9]
    mov x0, x1
    bl __rt_heap_free_safe
    // @src line=7 col=34 end=7:42 op=load_local
    sub x9, x29, #832
    ldr x0, [x9]
    sub x9, x29, #648
    str x0, [x9]
    // @src line=7 col=42 end=7:43 op=runtime_call
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #7
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    mov x1, #0
    mov x2, #-1
    mov x3, #1
    sub x9, x29, #648
    ldr x0, [x9]
    bl __rt_mixed_array_get
    sub x9, x29, #656
    str x0, [x9]
    // @src line=7 col=42 end=7:43 op=cast
    adrp x0, _script_source_file
    add x0, x0, :lo12:_script_source_file
    adrp x9, _php_diagnostic_file
    add x9, x9, :lo12:_php_diagnostic_file
    str x0, [x9]
    adrp x9, _script_source_file_len
    add x9, x9, :lo12:_script_source_file_len
    ldr x0, [x9]
    adrp x9, _php_diagnostic_file_len
    add x9, x9, :lo12:_php_diagnostic_file_len
    str x0, [x9]
    mov x0, #7
    adrp x9, _php_diagnostic_line
    add x9, x9, :lo12:_php_diagnostic_line
    str x0, [x9]
    sub x9, x29, #656
    ldr x0, [x9]
    bl _eir_shared_mixed_to_string
    sub x9, x29, #680
    str x15, [x9]
    sub x9, x29, #672
    str x1, [x9]
    sub x9, x29, #664
    str x2, [x9]
    // @src line=7 col=42 end=7:43 op=release
    sub x9, x29, #656
    ldr x0, [x9]
    bl __rt_decref_mixed
    // @src line=7 col=30 end=7:46 op=runtime_call
    sub x9, x29, #672
    ldr x1, [x9]
    sub x9, x29, #664
    ldr x2, [x9]
    cbz x2, .L_eir_main_ord_empty_355
    ldrb w0, [x1]
    b .L_eir_main_ord_done_356
.L_eir_main_ord_empty_355:
    mov x0, #0
.L_eir_main_ord_done_356:
    mov x21, x0
    // @src line=7 col=30 end=7:46 op=release
    sub x9, x29, #672
    ldr x1, [x9]
    sub x9, x29, #664
    ldr x2, [x9]
    mov x0, x1
    bl __rt_heap_free_safe
    // @src line=7 col=28 end=7:46 op=ichecked_add
    mov x0, x22
    mov x10, x21
    adds x11, x0, x10
    b.vs .L_eir_main_stack_numeric_overflow_357
    mov x0, x11
    sub x9, x29, #904
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #912
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #896
    str x0, [x9]
    b .L_eir_main_stack_numeric_done_358
.L_eir_main_stack_numeric_overflow_357:
    scvtf d0, x0
    scvtf d1, x10
    fadd d0, d0, d1
    fmov x0, d0
    sub x9, x29, #904
    str x0, [x9]
    mov x0, #2
    sub x9, x29, #912
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #896
    str x0, [x9]
.L_eir_main_stack_numeric_done_358:
    sub x0, x29, #912
    sub x9, x29, #696
    str x0, [x9]
    // @src line=7 col=5 end=7:7 op=mixed_numeric_binop
    sub x9, x29, #600
    ldr x0, [x9]
    str x0, [sp, #-16]!
    sub x9, x29, #696
    ldr x0, [x9]
    str x0, [sp, #-16]!
    ldr x0, [sp, #16]
    ldr x1, [sp]
    bl __rt_mixed_numeric_add
    str x0, [sp, #-16]!
    ldr x0, [sp], #16
    add sp, sp, #32
    sub x9, x29, #704
    str x0, [x9]
    // @src line=7 col=5 end=7:7 op=release
    // @src line=7 col=5 end=7:7 op=nop
    // @src line=7 col=5 end=7:7 op=nop
    // @src line=7 col=5 end=7:7 op=acquire
    sub x9, x29, #704
    ldr x0, [x9]
    str x0, [sp, #-16]!
    bl __rt_incref
    ldr x0, [sp], #16
    sub x9, x29, #720
    str x0, [x9]
    // @src line=7 col=5 end=7:7 op=release_local_slot
    sub x9, x29, #816
    ldr x0, [x9]
    cbz x0, .L_eir_main_release_local_slot_done_359
1:
    sub x9, x29, #816
    str xzr, [x9]
    bl __rt_decref_mixed
.L_eir_main_release_local_slot_done_359:
    // @src line=7 col=5 end=7:7 op=store_local
    sub x9, x29, #720
    ldr x0, [x9]
    sub x9, x29, #816
    str x0, [x9]
    // @src line=7 col=5 end=7:7 op=release
    sub x9, x29, #704
    ldr x0, [x9]
    bl __rt_decref_mixed
    // @src line=5 col=27 end=5:29 op=concat_reset
    sub x9, x29, #1064
    ldr x10, [x9]
    adrp x9, _concat_off
    add x9, x9, :lo12:_concat_off
    str x10, [x9]
    // @src line=5 col=27 end=5:29 op=load_local
    sub x9, x29, #840
    ldr x0, [x9]
    mov x21, x0
    // @src line=5 col=27 end=5:29 op=nop
    // @src line=5 col=27 end=5:29 op=iadd
    add x21, x21, #1
    // @src line=5 col=27 end=5:29 op=nop
    // @src line=5 col=27 end=5:29 op=nop
    // @src line=5 col=27 end=5:29 op=store_local
    mov x0, x21
    sub x9, x29, #840
    str x0, [x9]
    // @src line=5 col=27 end=5:29 op=nop
    // @src line=5 col=27 end=5:29 op=nop
    b .L_eir_main_for_cond_counter_fast_267
    // @block name=counter.integer.exit
.L_eir_main_counter_integer_exit_270:
    sub x9, x29, #840
    ldr x0, [x9]
    mov x21, x0
    sub x9, x29, #928
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #936
    str x0, [x9]
    mov x0, #0
    sub x9, x29, #920
    str x0, [x9]
    sub x0, x29, #936
    sub x9, x29, #776
    str x0, [x9]
    sub x9, x29, #824
    ldr x0, [x9]
    cbz x0, .L_eir_main_numeric_local_cleanup_done_360
1:
    sub x9, x29, #824
    str xzr, [x9]
    sub x10, x29, #864
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_360
1:
    sub x10, x29, #888
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_360
1:
    sub x10, x29, #912
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_360
1:
    sub x10, x29, #936
    cmp x0, x10
    b.eq .L_eir_main_numeric_local_cleanup_done_360
1:
    bl __rt_decref_mixed
.L_eir_main_numeric_local_cleanup_done_360:
    sub x9, x29, #776
    ldr x0, [x9]
    sub x9, x29, #824
    str x0, [x9]
    b .L_eir_main_for_exit_263
    // @block name=for.update
.L_eir_main_for_update_262:
    udf #0
    // @block name=for.update.counter.fast
.L_eir_main_for_update_counter_fast_269:
    udf #0
    // -- cold paths kept out of the hot fallthrough --
    // @src line=6 col=35 end=6:37 op=cast
.L_eir_main_frame_numeric_int_cast_slow_299:
    ldur x0, [x29, #-88]
    bl __rt_mixed_cast_int
    b .L_eir_main_frame_numeric_int_cast_done_300
    // @endfn name=main
.section .text.__rt_mbstring_startup,"ax",@progbits
.globl __rt_mbstring_startup
.type __rt_mbstring_startup, %function
__rt_mbstring_startup:
    // prologue
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, sp
    bl __rt_mbstring_startup_status
    cbz w0, .Lmbstring_startup_ready
    mov x0, #1
    bl exit
.Lmbstring_startup_ready:
    mov x9, x29
    ldp x29, x30, [x9]
    add sp, x9, #16
    ret
.section .text.__rt_mbstring_startup_status,"ax",@progbits
.globl __rt_mbstring_startup_status
.type __rt_mbstring_startup_status, %function
__rt_mbstring_startup_status:
    sub sp, sp, #80
    stp x29, x30, [sp, #64]
    adrp x0, _data_167
    add x0, x0, :lo12:_data_167
    adrp x9, _data_168
    add x9, x9, :lo12:_data_168
    ldr x1, [x9]
    adrp x2, _data_169
    add x2, x2, :lo12:_data_169
    mov x3, sp
    bl elephc_mbstring_configure_v1
    str w0, [sp, #48]
    mov x0, sp
    bl elephc_mbstring_release_v1
    ldr w0, [sp, #48]
.Lmbstring_startup_status_done:
    ldp x29, x30, [sp, #64]
    add sp, sp, #80
    ret
.section .text.__rt_mbstring_startup_diagnostic,"ax",@progbits
.globl __rt_mbstring_startup_diagnostic
.type __rt_mbstring_startup_diagnostic, %function
__rt_mbstring_startup_diagnostic:
    stp x29, x30, [sp, #-32]!
    stp x2, x3, [sp, #16]
    cmp w1, #8192
    b.eq .Lmbstring_startup_deprecated
    adrp x1, _str_170
    add x1, x1, :lo12:_str_170
    mov x2, #9
    b .Lmbstring_startup_prefix
.Lmbstring_startup_deprecated:
    adrp x1, _str_171
    add x1, x1, :lo12:_str_171
    mov x2, #12
.Lmbstring_startup_prefix:
    bl __rt_diag_warning
    ldp x1, x2, [sp, #16]
    bl __rt_diag_warning
    adrp x1, _str_3
    add x1, x1, :lo12:_str_3
    mov x2, #1
    bl __rt_diag_warning
    mov x0, #0
    ldp x29, x30, [sp], #32
    ret

.data
.globl _str_0
_str_0:
    .ascii "Object of class "
.globl _str_1
_str_1:
    .ascii " could not be converted to string"
.globl _str_2
_str_2:
    .ascii "\nFatal error: Uncaught Error: "
.globl _str_3
_str_3:
    .ascii "\n"
.globl _str_4
_str_4:
    .ascii ""
.globl _str_5
_str_5:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign int to property Error::$previous of type ?Throwable\n"
.globl _str_6
_str_6:
    .ascii "Cannot assign int to property Error::$previous of type ?Throwable"
.globl _str_7
_str_7:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign string to property Error::$previous of type ?Throwable\n"
.globl _str_8
_str_8:
    .ascii "Cannot assign string to property Error::$previous of type ?Throwable"
.globl _str_9
_str_9:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign float to property Error::$previous of type ?Throwable\n"
.globl _str_10
_str_10:
    .ascii "Cannot assign float to property Error::$previous of type ?Throwable"
.globl _str_11
_str_11:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign false to property Error::$previous of type ?Throwable\n"
.globl _str_12
_str_12:
    .ascii "Cannot assign false to property Error::$previous of type ?Throwable"
.globl _str_13
_str_13:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign true to property Error::$previous of type ?Throwable\n"
.globl _str_14
_str_14:
    .ascii "Cannot assign true to property Error::$previous of type ?Throwable"
.globl _str_15
_str_15:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign array to property Error::$previous of type ?Throwable\n"
.globl _str_16
_str_16:
    .ascii "Cannot assign array to property Error::$previous of type ?Throwable"
.globl _str_17
_str_17:
    .ascii "object"
.globl _str_18
_str_18:
    .ascii "Cannot assign "
.globl _str_19
_str_19:
    .ascii " to property Error::$previous of type ?Throwable"
.globl _str_20
_str_20:
    .ascii "\nFatal error: Uncaught TypeError: "
.globl _str_21
_str_21:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign resource to property Error::$previous of type ?Throwable\n"
.globl _str_22
_str_22:
    .ascii "Cannot assign resource to property Error::$previous of type ?Throwable"
.globl _str_23
_str_23:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign Closure to property Error::$previous of type ?Throwable\n"
.globl _str_24
_str_24:
    .ascii "Cannot assign Closure to property Error::$previous of type ?Throwable"
.globl _str_25
_str_25:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign object to property Error::$previous of type ?Throwable\n"
.globl _str_26
_str_26:
    .ascii "Cannot assign object to property Error::$previous of type ?Throwable"
.globl _str_27
_str_27:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign int to property TypeError::$previous of type ?Throwable\n"
.globl _str_28
_str_28:
    .ascii "Cannot assign int to property TypeError::$previous of type ?Throwable"
.globl _str_29
_str_29:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign string to property TypeError::$previous of type ?Throwable\n"
.globl _str_30
_str_30:
    .ascii "Cannot assign string to property TypeError::$previous of type ?Throwable"
.globl _str_31
_str_31:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign float to property TypeError::$previous of type ?Throwable\n"
.globl _str_32
_str_32:
    .ascii "Cannot assign float to property TypeError::$previous of type ?Throwable"
.globl _str_33
_str_33:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign false to property TypeError::$previous of type ?Throwable\n"
.globl _str_34
_str_34:
    .ascii "Cannot assign false to property TypeError::$previous of type ?Throwable"
.globl _str_35
_str_35:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign true to property TypeError::$previous of type ?Throwable\n"
.globl _str_36
_str_36:
    .ascii "Cannot assign true to property TypeError::$previous of type ?Throwable"
.globl _str_37
_str_37:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign array to property TypeError::$previous of type ?Throwable\n"
.globl _str_38
_str_38:
    .ascii "Cannot assign array to property TypeError::$previous of type ?Throwable"
.globl _str_39
_str_39:
    .ascii " to property TypeError::$previous of type ?Throwable"
.globl _str_40
_str_40:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign resource to property TypeError::$previous of type ?Throwable\n"
.globl _str_41
_str_41:
    .ascii "Cannot assign resource to property TypeError::$previous of type ?Throwable"
.globl _str_42
_str_42:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign Closure to property TypeError::$previous of type ?Throwable\n"
.globl _str_43
_str_43:
    .ascii "Cannot assign Closure to property TypeError::$previous of type ?Throwable"
.globl _str_44
_str_44:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign object to property TypeError::$previous of type ?Throwable\n"
.globl _str_45
_str_45:
    .ascii "Cannot assign object to property TypeError::$previous of type ?Throwable"
.globl _str_46
_str_46:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign int to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_47
_str_47:
    .ascii "Cannot assign int to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_48
_str_48:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign string to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_49
_str_49:
    .ascii "Cannot assign string to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_50
_str_50:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign float to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_51
_str_51:
    .ascii "Cannot assign float to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_52
_str_52:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign false to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_53
_str_53:
    .ascii "Cannot assign false to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_54
_str_54:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign true to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_55
_str_55:
    .ascii "Cannot assign true to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_56
_str_56:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign array to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_57
_str_57:
    .ascii "Cannot assign array to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_58
_str_58:
    .ascii " to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_59
_str_59:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign resource to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_60
_str_60:
    .ascii "Cannot assign resource to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_61
_str_61:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign Closure to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_62
_str_62:
    .ascii "Cannot assign Closure to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_63
_str_63:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign object to property ArgumentCountError::$previous of type ?Throwable\n"
.globl _str_64
_str_64:
    .ascii "Cannot assign object to property ArgumentCountError::$previous of type ?Throwable"
.globl _str_65
_str_65:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign int to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_66
_str_66:
    .ascii "Cannot assign int to property ArithmeticError::$previous of type ?Throwable"
.globl _str_67
_str_67:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign string to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_68
_str_68:
    .ascii "Cannot assign string to property ArithmeticError::$previous of type ?Throwable"
.globl _str_69
_str_69:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign float to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_70
_str_70:
    .ascii "Cannot assign float to property ArithmeticError::$previous of type ?Throwable"
.globl _str_71
_str_71:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign false to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_72
_str_72:
    .ascii "Cannot assign false to property ArithmeticError::$previous of type ?Throwable"
.globl _str_73
_str_73:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign true to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_74
_str_74:
    .ascii "Cannot assign true to property ArithmeticError::$previous of type ?Throwable"
.globl _str_75
_str_75:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign array to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_76
_str_76:
    .ascii "Cannot assign array to property ArithmeticError::$previous of type ?Throwable"
.globl _str_77
_str_77:
    .ascii " to property ArithmeticError::$previous of type ?Throwable"
.globl _str_78
_str_78:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign resource to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_79
_str_79:
    .ascii "Cannot assign resource to property ArithmeticError::$previous of type ?Throwable"
.globl _str_80
_str_80:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign Closure to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_81
_str_81:
    .ascii "Cannot assign Closure to property ArithmeticError::$previous of type ?Throwable"
.globl _str_82
_str_82:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign object to property ArithmeticError::$previous of type ?Throwable\n"
.globl _str_83
_str_83:
    .ascii "Cannot assign object to property ArithmeticError::$previous of type ?Throwable"
.globl _str_84
_str_84:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign int to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_85
_str_85:
    .ascii "Cannot assign int to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_86
_str_86:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign string to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_87
_str_87:
    .ascii "Cannot assign string to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_88
_str_88:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign float to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_89
_str_89:
    .ascii "Cannot assign float to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_90
_str_90:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign false to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_91
_str_91:
    .ascii "Cannot assign false to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_92
_str_92:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign true to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_93
_str_93:
    .ascii "Cannot assign true to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_94
_str_94:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign array to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_95
_str_95:
    .ascii "Cannot assign array to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_96
_str_96:
    .ascii " to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_97
_str_97:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign resource to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_98
_str_98:
    .ascii "Cannot assign resource to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_99
_str_99:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign Closure to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_100
_str_100:
    .ascii "Cannot assign Closure to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_101
_str_101:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign object to property DivisionByZeroError::$previous of type ?Throwable\n"
.globl _str_102
_str_102:
    .ascii "Cannot assign object to property DivisionByZeroError::$previous of type ?Throwable"
.globl _str_103
_str_103:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign int to property Exception::$previous of type ?Throwable\n"
.globl _str_104
_str_104:
    .ascii "Cannot assign int to property Exception::$previous of type ?Throwable"
.globl _str_105
_str_105:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign string to property Exception::$previous of type ?Throwable\n"
.globl _str_106
_str_106:
    .ascii "Cannot assign string to property Exception::$previous of type ?Throwable"
.globl _str_107
_str_107:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign float to property Exception::$previous of type ?Throwable\n"
.globl _str_108
_str_108:
    .ascii "Cannot assign float to property Exception::$previous of type ?Throwable"
.globl _str_109
_str_109:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign false to property Exception::$previous of type ?Throwable\n"
.globl _str_110
_str_110:
    .ascii "Cannot assign false to property Exception::$previous of type ?Throwable"
.globl _str_111
_str_111:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign true to property Exception::$previous of type ?Throwable\n"
.globl _str_112
_str_112:
    .ascii "Cannot assign true to property Exception::$previous of type ?Throwable"
.globl _str_113
_str_113:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign array to property Exception::$previous of type ?Throwable\n"
.globl _str_114
_str_114:
    .ascii "Cannot assign array to property Exception::$previous of type ?Throwable"
.globl _str_115
_str_115:
    .ascii " to property Exception::$previous of type ?Throwable"
.globl _str_116
_str_116:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign resource to property Exception::$previous of type ?Throwable\n"
.globl _str_117
_str_117:
    .ascii "Cannot assign resource to property Exception::$previous of type ?Throwable"
.globl _str_118
_str_118:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign Closure to property Exception::$previous of type ?Throwable\n"
.globl _str_119
_str_119:
    .ascii "Cannot assign Closure to property Exception::$previous of type ?Throwable"
.globl _str_120
_str_120:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign object to property Exception::$previous of type ?Throwable\n"
.globl _str_121
_str_121:
    .ascii "Cannot assign object to property Exception::$previous of type ?Throwable"
.globl _str_122
_str_122:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign int to property JsonException::$previous of type ?Throwable\n"
.globl _str_123
_str_123:
    .ascii "Cannot assign int to property JsonException::$previous of type ?Throwable"
.globl _str_124
_str_124:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign string to property JsonException::$previous of type ?Throwable\n"
.globl _str_125
_str_125:
    .ascii "Cannot assign string to property JsonException::$previous of type ?Throwable"
.globl _str_126
_str_126:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign float to property JsonException::$previous of type ?Throwable\n"
.globl _str_127
_str_127:
    .ascii "Cannot assign float to property JsonException::$previous of type ?Throwable"
.globl _str_128
_str_128:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign false to property JsonException::$previous of type ?Throwable\n"
.globl _str_129
_str_129:
    .ascii "Cannot assign false to property JsonException::$previous of type ?Throwable"
.globl _str_130
_str_130:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign true to property JsonException::$previous of type ?Throwable\n"
.globl _str_131
_str_131:
    .ascii "Cannot assign true to property JsonException::$previous of type ?Throwable"
.globl _str_132
_str_132:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign array to property JsonException::$previous of type ?Throwable\n"
.globl _str_133
_str_133:
    .ascii "Cannot assign array to property JsonException::$previous of type ?Throwable"
.globl _str_134
_str_134:
    .ascii " to property JsonException::$previous of type ?Throwable"
.globl _str_135
_str_135:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign resource to property JsonException::$previous of type ?Throwable\n"
.globl _str_136
_str_136:
    .ascii "Cannot assign resource to property JsonException::$previous of type ?Throwable"
.globl _str_137
_str_137:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign Closure to property JsonException::$previous of type ?Throwable\n"
.globl _str_138
_str_138:
    .ascii "Cannot assign Closure to property JsonException::$previous of type ?Throwable"
.globl _str_139
_str_139:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign object to property JsonException::$previous of type ?Throwable\n"
.globl _str_140
_str_140:
    .ascii "Cannot assign object to property JsonException::$previous of type ?Throwable"
.globl _str_141
_str_141:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign int to property ValueError::$previous of type ?Throwable\n"
.globl _str_142
_str_142:
    .ascii "Cannot assign int to property ValueError::$previous of type ?Throwable"
.globl _str_143
_str_143:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign string to property ValueError::$previous of type ?Throwable\n"
.globl _str_144
_str_144:
    .ascii "Cannot assign string to property ValueError::$previous of type ?Throwable"
.globl _str_145
_str_145:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign float to property ValueError::$previous of type ?Throwable\n"
.globl _str_146
_str_146:
    .ascii "Cannot assign float to property ValueError::$previous of type ?Throwable"
.globl _str_147
_str_147:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign false to property ValueError::$previous of type ?Throwable\n"
.globl _str_148
_str_148:
    .ascii "Cannot assign false to property ValueError::$previous of type ?Throwable"
.globl _str_149
_str_149:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign true to property ValueError::$previous of type ?Throwable\n"
.globl _str_150
_str_150:
    .ascii "Cannot assign true to property ValueError::$previous of type ?Throwable"
.globl _str_151
_str_151:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign array to property ValueError::$previous of type ?Throwable\n"
.globl _str_152
_str_152:
    .ascii "Cannot assign array to property ValueError::$previous of type ?Throwable"
.globl _str_153
_str_153:
    .ascii " to property ValueError::$previous of type ?Throwable"
.globl _str_154
_str_154:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign resource to property ValueError::$previous of type ?Throwable\n"
.globl _str_155
_str_155:
    .ascii "Cannot assign resource to property ValueError::$previous of type ?Throwable"
.globl _str_156
_str_156:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign Closure to property ValueError::$previous of type ?Throwable\n"
.globl _str_157
_str_157:
    .ascii "Cannot assign Closure to property ValueError::$previous of type ?Throwable"
.globl _str_158
_str_158:
    .ascii "\nFatal error: Uncaught TypeError: Cannot assign object to property ValueError::$previous of type ?Throwable\n"
.globl _str_159
_str_159:
    .ascii "Cannot assign object to property ValueError::$previous of type ?Throwable"
.globl _str_160
_str_160:
    .ascii "get"
.globl _str_161
_str_161:
    .ascii "Get"
.globl _str_162
_str_162:
    .ascii "set"
.globl _str_163
_str_163:
    .ascii "Set"
.globl _str_164
_str_164:
    .ascii "Ascending"
.globl _str_165
_str_165:
    .ascii "Descending"
.globl _str_166
_str_166:
    .ascii "UTF-8"
.globl _str_170
_str_170:
    .ascii "Warning: "
.globl _str_171
_str_171:
    .ascii "Deprecated: "
.p2align 3
.globl _data_167
_data_167:
    .quad 0x0000000000000002
    .quad 0x0000000000000000
    .quad _str_166
    .quad 0x0000000000000005
    .quad 0x0000000000000002
    .quad 0x0000000000000000
    .quad _str_166
    .quad 0x0000000000000005
    .quad 0x0000000000000002
    .quad 0x0000000000000000
    .quad _str_166
    .quad 0x0000000000000005
.p2align 3
.globl _data_168
_data_168:
    .quad 0x0000000000000003
.p2align 3
.globl _data_169
_data_169:
    .quad 0x0000001800000001
    .quad 0x0000000000000000
    .quad __rt_mbstring_startup_diagnostic

.comm _enum_case_PropertyHookType_Get, 8, 8
.comm _enum_case_PropertyHookType_Set, 8, 8
.comm _enum_case_SortDirection_Ascending, 8, 8
.comm _enum_case_SortDirection_Descending, 8, 8
.data
.p2align 3
.globl _callable_user_fn_name_0
_callable_user_fn_name_0:
    .ascii "__elephc_internal_object_compare"
.globl _callable_user_fn_name_1
_callable_user_fn_name_1:
    .ascii "main"
.p2align 3
.globl _callable_user_function_count
_callable_user_function_count:
    .quad 2
.globl _callable_user_function_table
_callable_user_function_table:
    .quad _callable_user_fn_name_0
    .quad 32
    .quad 0
    .quad _callable_user_fn_name_1
    .quad 4
    .quad 0
.p2align 3
.globl _instanceof_target_count
_instanceof_target_count:
    .quad 22
.globl _instanceof_target_entries
_instanceof_target_entries:
    .quad _instanceof_name_class_1
    .quad 5
    .quad 1
    .quad 0
    .quad _instanceof_name_class_abs_1
    .quad 6
    .quad 1
    .quad 0
    .quad _instanceof_name_class_2
    .quad 9
    .quad 2
    .quad 0
    .quad _instanceof_name_class_abs_2
    .quad 10
    .quad 2
    .quad 0
    .quad _instanceof_name_class_3
    .quad 18
    .quad 3
    .quad 0
    .quad _instanceof_name_class_abs_3
    .quad 19
    .quad 3
    .quad 0
    .quad _instanceof_name_class_4
    .quad 15
    .quad 4
    .quad 0
    .quad _instanceof_name_class_abs_4
    .quad 16
    .quad 4
    .quad 0
    .quad _instanceof_name_class_8
    .quad 19
    .quad 8
    .quad 0
    .quad _instanceof_name_class_abs_8
    .quad 20
    .quad 8
    .quad 0
    .quad _instanceof_name_class_9
    .quad 9
    .quad 9
    .quad 0
    .quad _instanceof_name_class_abs_9
    .quad 10
    .quad 9
    .quad 0
    .quad _instanceof_name_class_11
    .quad 13
    .quad 11
    .quad 0
    .quad _instanceof_name_class_abs_11
    .quad 14
    .quad 11
    .quad 0
    .quad _instanceof_name_class_17
    .quad 10
    .quad 17
    .quad 0
    .quad _instanceof_name_class_abs_17
    .quad 11
    .quad 17
    .quad 0
    .quad _instanceof_name_class_18
    .quad 8
    .quad 18
    .quad 0
    .quad _instanceof_name_class_abs_18
    .quad 9
    .quad 18
    .quad 0
    .quad _instanceof_name_interface_14
    .quad 10
    .quad 14
    .quad 1
    .quad _instanceof_name_interface_abs_14
    .quad 11
    .quad 14
    .quad 1
    .quad _instanceof_name_interface_15
    .quad 9
    .quad 15
    .quad 1
    .quad _instanceof_name_interface_abs_15
    .quad 10
    .quad 15
    .quad 1
.globl _instanceof_name_class_1
_instanceof_name_class_1:
    .ascii "Error"
.globl _instanceof_name_class_abs_1
_instanceof_name_class_abs_1:
    .ascii "\\Error"
.globl _instanceof_name_class_2
_instanceof_name_class_2:
    .ascii "TypeError"
.globl _instanceof_name_class_abs_2
_instanceof_name_class_abs_2:
    .ascii "\\TypeError"
.globl _instanceof_name_class_3
_instanceof_name_class_3:
    .ascii "ArgumentCountError"
.globl _instanceof_name_class_abs_3
_instanceof_name_class_abs_3:
    .ascii "\\ArgumentCountError"
.globl _instanceof_name_class_4
_instanceof_name_class_4:
    .ascii "ArithmeticError"
.globl _instanceof_name_class_abs_4
_instanceof_name_class_abs_4:
    .ascii "\\ArithmeticError"
.globl _instanceof_name_class_8
_instanceof_name_class_8:
    .ascii "DivisionByZeroError"
.globl _instanceof_name_class_abs_8
_instanceof_name_class_abs_8:
    .ascii "\\DivisionByZeroError"
.globl _instanceof_name_class_9
_instanceof_name_class_9:
    .ascii "Exception"
.globl _instanceof_name_class_abs_9
_instanceof_name_class_abs_9:
    .ascii "\\Exception"
.globl _instanceof_name_class_11
_instanceof_name_class_11:
    .ascii "JsonException"
.globl _instanceof_name_class_abs_11
_instanceof_name_class_abs_11:
    .ascii "\\JsonException"
.globl _instanceof_name_class_17
_instanceof_name_class_17:
    .ascii "ValueError"
.globl _instanceof_name_class_abs_17
_instanceof_name_class_abs_17:
    .ascii "\\ValueError"
.globl _instanceof_name_class_18
_instanceof_name_class_18:
    .ascii "stdClass"
.globl _instanceof_name_class_abs_18
_instanceof_name_class_abs_18:
    .ascii "\\stdClass"
.globl _instanceof_name_interface_14
_instanceof_name_interface_14:
    .ascii "Stringable"
.globl _instanceof_name_interface_abs_14
_instanceof_name_interface_abs_14:
    .ascii "\\Stringable"
.globl _instanceof_name_interface_15
_instanceof_name_interface_15:
    .ascii "Throwable"
.globl _instanceof_name_interface_abs_15
_instanceof_name_interface_abs_15:
    .ascii "\\Throwable"
    .p2align 3
.p2align 3
.globl _class_name_count
_class_name_count:
    .quad 19
.globl _class_name_entries
_class_name_entries:
    .quad _class_name_missing
    .quad 0
    .quad _class_name_1
    .quad 5
    .quad _class_name_2
    .quad 9
    .quad _class_name_3
    .quad 18
    .quad _class_name_4
    .quad 15
    .quad _class_name_missing
    .quad 0
    .quad _class_name_missing
    .quad 0
    .quad _class_name_missing
    .quad 0
    .quad _class_name_8
    .quad 19
    .quad _class_name_9
    .quad 9
    .quad _class_name_missing
    .quad 0
    .quad _class_name_11
    .quad 13
    .quad _class_name_missing
    .quad 0
    .quad _class_name_missing
    .quad 0
    .quad _class_name_missing
    .quad 0
    .quad _class_name_missing
    .quad 0
    .quad _class_name_missing
    .quad 0
    .quad _class_name_17
    .quad 10
    .quad _class_name_18
    .quad 8
.globl _class_name_missing
_class_name_missing:
    .byte 0
.globl _class_name_1
_class_name_1:
    .ascii "Error"
.globl _class_name_2
_class_name_2:
    .ascii "TypeError"
.globl _class_name_3
_class_name_3:
    .ascii "ArgumentCountError"
.globl _class_name_4
_class_name_4:
    .ascii "ArithmeticError"
.globl _class_name_8
_class_name_8:
    .ascii "DivisionByZeroError"
.globl _class_name_9
_class_name_9:
    .ascii "Exception"
.globl _class_name_11
_class_name_11:
    .ascii "JsonException"
.globl _class_name_17
_class_name_17:
    .ascii "ValueError"
.globl _class_name_18
_class_name_18:
    .ascii "stdClass"
    .p2align 3
.p2align 3
.globl _class_previous_slot_count
_class_previous_slot_count:
    .quad 19
.globl _class_previous_slots
_class_previous_slots:
    .quad 0
    .quad 41
    .quad 41
    .quad 41
    .quad 41
    .quad 0
    .quad 0
    .quad 0
    .quad 41
    .quad 41
    .quad 0
    .quad 41
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 41
    .quad 0
.p2align 3
.globl _weak_map_key_retire_fn
_weak_map_key_retire_fn:
    .quad 0
.globl _interface_name_0
_interface_name_0:
    .ascii "ArrayAccess"
.globl _interface_name_1
_interface_name_1:
    .ascii "BackedEnum"
.globl _interface_name_2
_interface_name_2:
    .ascii "Countable"
.globl _interface_name_3
_interface_name_3:
    .ascii "Iterator"
.globl _interface_name_4
_interface_name_4:
    .ascii "IteratorAggregate"
.globl _interface_name_5
_interface_name_5:
    .ascii "JsonSerializable"
.globl _interface_name_6
_interface_name_6:
    .ascii "OuterIterator"
.globl _interface_name_7
_interface_name_7:
    .ascii "RecursiveIterator"
.globl _interface_name_8
_interface_name_8:
    .ascii "SeekableIterator"
.globl _interface_name_9
_interface_name_9:
    .ascii "Serializable"
.globl _interface_name_10
_interface_name_10:
    .ascii "SplObserver"
.globl _interface_name_11
_interface_name_11:
    .ascii "SplSubject"
.globl _interface_name_12
_interface_name_12:
    .ascii "Stringable"
.globl _interface_name_13
_interface_name_13:
    .ascii "Throwable"
.globl _interface_name_14
_interface_name_14:
    .ascii "Traversable"
.globl _interface_name_15
_interface_name_15:
    .ascii "UnitEnum"
.p2align 3
.globl _interface_names_count
_interface_names_count:
    .quad 16
.globl _interface_names
_interface_names:
    .quad _interface_name_0
    .quad 11
    .quad _interface_name_1
    .quad 10
    .quad _interface_name_2
    .quad 9
    .quad _interface_name_3
    .quad 8
    .quad _interface_name_4
    .quad 17
    .quad _interface_name_5
    .quad 16
    .quad _interface_name_6
    .quad 13
    .quad _interface_name_7
    .quad 17
    .quad _interface_name_8
    .quad 16
    .quad _interface_name_9
    .quad 12
    .quad _interface_name_10
    .quad 11
    .quad _interface_name_11
    .quad 10
    .quad _interface_name_12
    .quad 10
    .quad _interface_name_13
    .quad 9
    .quad _interface_name_14
    .quad 11
    .quad _interface_name_15
    .quad 8
.p2align 3
.globl _trait_names_count
_trait_names_count:
    .quad 0
.globl _trait_names
_trait_names:
.globl _enum_name_0
_enum_name_0:
    .ascii "PropertyHookType"
.globl _enum_name_1
_enum_name_1:
    .ascii "SortDirection"
.p2align 3
.globl _enum_names_count
_enum_names_count:
    .quad 2
.globl _enum_names
_enum_names:
    .quad _enum_name_0
    .quad 16
    .quad _enum_name_1
    .quad 13
.globl _fiber_class_id
_fiber_class_id:
    .quad 18446744073709551615
.globl _fiber_error_class_id
_fiber_error_class_id:
    .quad 18446744073709551615
.globl _generator_class_id
_generator_class_id:
    .quad 18446744073709551615
.globl _spl_dll_class_id
_spl_dll_class_id:
    .quad 18446744073709551615
.globl _spl_stack_class_id
_spl_stack_class_id:
    .quad 18446744073709551615
.globl _spl_queue_class_id
_spl_queue_class_id:
    .quad 18446744073709551615
.globl _spl_fixed_array_class_id
_spl_fixed_array_class_id:
    .quad 18446744073709551615
.globl _spl_exception_class_id
_spl_exception_class_id:
    .quad 9
.globl _spl_error_class_id
_spl_error_class_id:
    .quad 1
.globl _spl_logic_exception_class_id
_spl_logic_exception_class_id:
    .quad 18446744073709551615
.globl _spl_runtime_exception_class_id
_spl_runtime_exception_class_id:
    .quad 18446744073709551615
.globl _spl_out_of_range_exception_class_id
_spl_out_of_range_exception_class_id:
    .quad 18446744073709551615
.globl _spl_out_of_bounds_exception_class_id
_spl_out_of_bounds_exception_class_id:
    .quad 18446744073709551615
.globl _spl_invalid_argument_exception_class_id
_spl_invalid_argument_exception_class_id:
    .quad 18446744073709551615
.globl _spl_type_error_class_id
_spl_type_error_class_id:
    .quad 2
.globl _error_exception_class_id
_error_exception_class_id:
    .quad 18446744073709551615
.globl _weak_reference_class_id
_weak_reference_class_id:
    .quad 18446744073709551615
.globl _weak_map_class_id
_weak_map_class_id:
    .quad 18446744073709551615
.globl _spl_value_error_class_id
_spl_value_error_class_id:
    .quad 17
.globl _spl_arithmetic_error_class_id
_spl_arithmetic_error_class_id:
    .quad 4
.globl _spl_division_by_zero_error_class_id
_spl_division_by_zero_error_class_id:
    .quad 8
.globl _spl_argument_count_error_class_id
_spl_argument_count_error_class_id:
    .quad 3
.globl _interface_count
_interface_count:
    .quad 2
.globl _interface_method_ptrs
_interface_method_ptrs:
    .quad _interface_methods_14
    .quad _interface_methods_15
.globl _class_interface_ptrs
_class_interface_ptrs:
    .quad _class_interfaces_missing
    .quad _class_interfaces_1
    .quad _class_interfaces_2
    .quad _class_interfaces_3
    .quad _class_interfaces_4
    .quad _class_interfaces_missing
    .quad _class_interfaces_missing
    .quad _class_interfaces_missing
    .quad _class_interfaces_8
    .quad _class_interfaces_9
    .quad _class_interfaces_missing
    .quad _class_interfaces_11
    .quad _class_interfaces_missing
    .quad _class_interfaces_missing
    .quad _class_interfaces_missing
    .quad _class_interfaces_missing
    .quad _class_interfaces_missing
    .quad _class_interfaces_17
    .quad _class_interfaces_18
.globl _class_json_desc_ptrs
_class_json_desc_ptrs:
    .quad _class_json_desc_missing
    .quad _class_json_desc_1
    .quad _class_json_desc_2
    .quad _class_json_desc_3
    .quad _class_json_desc_4
    .quad _class_json_desc_missing
    .quad _class_json_desc_missing
    .quad _class_json_desc_missing
    .quad _class_json_desc_8
    .quad _class_json_desc_9
    .quad _class_json_desc_missing
    .quad _class_json_desc_11
    .quad _class_json_desc_missing
    .quad _class_json_desc_missing
    .quad _class_json_desc_missing
    .quad _class_json_desc_missing
    .quad _class_json_desc_missing
    .quad _class_json_desc_17
    .quad _class_json_desc_18
.globl _class_vd_desc_ptrs
_class_vd_desc_ptrs:
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_1
    .quad _class_vd_desc_2
    .quad _class_vd_desc_3
    .quad _class_vd_desc_4
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_8
    .quad _class_vd_desc_9
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_11
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_missing
    .quad _class_vd_desc_17
    .quad _class_vd_desc_18
.globl _class_prop_desc_ptrs
_class_prop_desc_ptrs:
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_1
    .quad _class_prop_desc_2
    .quad _class_prop_desc_3
    .quad _class_prop_desc_4
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_8
    .quad _class_prop_desc_9
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_11
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_missing
    .quad _class_prop_desc_17
    .quad _class_prop_desc_18
.globl _class_enum_kinds
_class_enum_kinds:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_enum_name_offsets
_class_enum_name_offsets:
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
.globl _class_enum_value_offsets
_class_enum_value_offsets:
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
    .quad -1
.globl _json_exception_class_id
_json_exception_class_id:
    .quad 11
.globl _class_parent_ids
_class_parent_ids:
    .quad -2
    .quad -1
    .quad 1
    .quad 2
    .quad 1
    .quad -2
    .quad -2
    .quad -2
    .quad 4
    .quad -1
    .quad -2
    .quad 9
    .quad -2
    .quad -2
    .quad -2
    .quad -2
    .quad -2
    .quad 1
    .quad -1
.globl _class_object_payload_sizes
_class_object_payload_sizes:
    .quad 0
    .quad 56
    .quad 56
    .quad 56
    .quad 56
    .quad 0
    .quad 0
    .quad 0
    .quad 56
    .quad 56
    .quad 0
    .quad 56
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 56
    .quad 16
.globl _class_throwable_line_offsets
_class_throwable_line_offsets:
    .quad 0
    .quad 56
    .quad 56
    .quad 56
    .quad 56
    .quad 0
    .quad 0
    .quad 0
    .quad 56
    .quad 56
    .quad 0
    .quad 56
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 56
    .quad 0
.globl _class_object_allocation_sizes
_class_object_allocation_sizes:
    .quad 0
    .quad 64
    .quad 64
    .quad 64
    .quad 64
    .quad 0
    .quad 0
    .quad 0
    .quad 64
    .quad 64
    .quad 0
    .quad 64
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 64
    .quad 16
.globl _class_object_dynamic_prop_flags
_class_object_dynamic_prop_flags:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 1
.globl _class_dump_dyn_prop_flags
_class_dump_dyn_prop_flags:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 1
.globl _class_dump_dyn_first
_class_dump_dyn_first:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_reflection_public_cast_flags
_class_reflection_public_cast_flags:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_gc_desc_count
_class_gc_desc_count:
    .quad 19
.globl _class_gc_desc_ptrs
_class_gc_desc_ptrs:
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_1
    .quad _class_gc_desc_2
    .quad _class_gc_desc_3
    .quad _class_gc_desc_4
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_8
    .quad _class_gc_desc_9
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_11
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_missing
    .quad _class_gc_desc_17
    .quad _class_gc_desc_18
.globl _class_vtable_ptrs
_class_vtable_ptrs:
    .quad _class_vtable_missing
    .quad _class_vtable_1
    .quad _class_vtable_2
    .quad _class_vtable_3
    .quad _class_vtable_4
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_8
    .quad _class_vtable_9
    .quad _class_vtable_missing
    .quad _class_vtable_11
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_17
    .quad _class_vtable_18
.globl _class_source_vtable_ptrs
_class_source_vtable_ptrs:
    .quad _class_vtable_missing
    .quad _class_source_vtable_1
    .quad _class_source_vtable_2
    .quad _class_source_vtable_3
    .quad _class_source_vtable_4
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_source_vtable_8
    .quad _class_source_vtable_9
    .quad _class_vtable_missing
    .quad _class_source_vtable_11
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_vtable_missing
    .quad _class_source_vtable_17
    .quad _class_source_vtable_18
.globl _class_destruct_count
_class_destruct_count:
    .quad 19
.globl _class_destruct_ptrs
_class_destruct_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_tostring_count
_class_tostring_count:
    .quad 19
.globl _class_tostring_ptrs
_class_tostring_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_debuginfo_ptrs
_class_debuginfo_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_debuginfo_void
_class_debuginfo_void:
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .byte 0
    .p2align 3
.globl _debuginfo_any
_debuginfo_any:
    .quad 0
.globl _class_iface_method_count
_class_iface_method_count:
    .quad 19
.globl _class_count_ptrs
_class_count_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_offsetget_ptrs
_class_offsetget_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_offsetset_ptrs
_class_offsetset_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_offsetunset_ptrs
_class_offsetunset_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_serialize_ptrs
_class_serialize_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_unserialize_ptrs
_class_unserialize_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_sleep_ptrs
_class_sleep_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_wakeup_ptrs
_class_wakeup_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_serializable_serialize_ptrs
_class_serializable_serialize_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_serializable_unserialize_ptrs
_class_serializable_unserialize_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_serializable_serialize_return_abi
_class_serializable_serialize_return_abi:
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
.globl _class_serializable_unserialize_return_abi
_class_serializable_unserialize_return_abi:
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
    .quad 2
.globl _class_serializable_unserialize_boxed
_class_serializable_unserialize_boxed:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_compare_ptrs
_class_compare_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_to_number_ptrs
_class_to_number_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
_unser_refused_class_name_0:
    .ascii "WeakReference"
_unser_refused_class_name_1:
    .ascii "WeakMap"
_unser_refused_class_name_2:
    .ascii "SensitiveParameterValue"
_unser_refused_class_name_3:
    .ascii "CURLFile"
_unser_refused_class_name_4:
    .ascii "Closure"
_unser_refused_class_name_5:
    .ascii "CurlHandle"
_unser_refused_class_name_6:
    .ascii "CurlMultiHandle"
_unser_refused_class_name_7:
    .ascii "CurlShareHandle"
_unser_refused_class_name_8:
    .ascii "CurlSharePersistentHandle"
_unser_refused_class_name_9:
    .ascii "DirectoryIterator"
_unser_refused_class_name_10:
    .ascii "Fiber"
_unser_refused_class_name_11:
    .ascii "FilesystemIterator"
_unser_refused_class_name_12:
    .ascii "GdImage"
_unser_refused_class_name_13:
    .ascii "finfo"
_unser_refused_class_name_14:
    .ascii "Generator"
_unser_refused_class_name_15:
    .ascii "GlobIterator"
_unser_refused_class_name_16:
    .ascii "InternalIterator"
_unser_refused_class_name_17:
    .ascii "PDO"
_unser_refused_class_name_18:
    .ascii "PDORow"
_unser_refused_class_name_19:
    .ascii "PDOStatement"
_unser_refused_class_name_20:
    .ascii "Pdo\\Dblib"
_unser_refused_class_name_21:
    .ascii "Pdo\\Firebird"
_unser_refused_class_name_22:
    .ascii "Pdo\\Ibm"
_unser_refused_class_name_23:
    .ascii "Pdo\\Mysql"
_unser_refused_class_name_24:
    .ascii "Pdo\\Odbc"
_unser_refused_class_name_25:
    .ascii "Pdo\\Pgsql"
_unser_refused_class_name_26:
    .ascii "Pdo\\Sqlite"
_unser_refused_class_name_27:
    .ascii "Phar"
_unser_refused_class_name_28:
    .ascii "PharData"
_unser_refused_class_name_29:
    .ascii "PharFileInfo"
_unser_refused_class_name_30:
    .ascii "RecursiveDirectoryIterator"
_unser_refused_class_name_31:
    .ascii "ReflectionAttribute"
_unser_refused_class_name_32:
    .ascii "ReflectionClass"
_unser_refused_class_name_33:
    .ascii "ReflectionClassConstant"
_unser_refused_class_name_34:
    .ascii "ReflectionEnum"
_unser_refused_class_name_35:
    .ascii "ReflectionEnumBackedCase"
_unser_refused_class_name_36:
    .ascii "ReflectionEnumUnitCase"
_unser_refused_class_name_37:
    .ascii "ReflectionFunction"
_unser_refused_class_name_38:
    .ascii "ReflectionIntersectionType"
_unser_refused_class_name_39:
    .ascii "ReflectionMethod"
_unser_refused_class_name_40:
    .ascii "ReflectionNamedType"
_unser_refused_class_name_41:
    .ascii "ReflectionObject"
_unser_refused_class_name_42:
    .ascii "ReflectionParameter"
_unser_refused_class_name_43:
    .ascii "ReflectionProperty"
_unser_refused_class_name_44:
    .ascii "ReflectionUnionType"
_unser_refused_class_name_45:
    .ascii "SplFileInfo"
_unser_refused_class_name_46:
    .ascii "SplFileObject"
_unser_refused_class_name_47:
    .ascii "SplTempFileObject"
_unser_refused_class_name_48:
    .ascii "XMLParser"
.p2align 3
.globl _unser_refused_class_count
_unser_refused_class_count:
    .quad 49
.globl _unser_refused_classes
_unser_refused_classes:
    .quad _unser_refused_class_name_0
    .quad 13
    .quad _unser_refused_class_name_1
    .quad 7
    .quad _unser_refused_class_name_2
    .quad 23
    .quad _unser_refused_class_name_3
    .quad 8
    .quad _unser_refused_class_name_4
    .quad 7
    .quad _unser_refused_class_name_5
    .quad 10
    .quad _unser_refused_class_name_6
    .quad 15
    .quad _unser_refused_class_name_7
    .quad 15
    .quad _unser_refused_class_name_8
    .quad 25
    .quad _unser_refused_class_name_9
    .quad 17
    .quad _unser_refused_class_name_10
    .quad 5
    .quad _unser_refused_class_name_11
    .quad 18
    .quad _unser_refused_class_name_12
    .quad 7
    .quad _unser_refused_class_name_13
    .quad 5
    .quad _unser_refused_class_name_14
    .quad 9
    .quad _unser_refused_class_name_15
    .quad 12
    .quad _unser_refused_class_name_16
    .quad 16
    .quad _unser_refused_class_name_17
    .quad 3
    .quad _unser_refused_class_name_18
    .quad 6
    .quad _unser_refused_class_name_19
    .quad 12
    .quad _unser_refused_class_name_20
    .quad 9
    .quad _unser_refused_class_name_21
    .quad 12
    .quad _unser_refused_class_name_22
    .quad 7
    .quad _unser_refused_class_name_23
    .quad 9
    .quad _unser_refused_class_name_24
    .quad 8
    .quad _unser_refused_class_name_25
    .quad 9
    .quad _unser_refused_class_name_26
    .quad 10
    .quad _unser_refused_class_name_27
    .quad 4
    .quad _unser_refused_class_name_28
    .quad 8
    .quad _unser_refused_class_name_29
    .quad 12
    .quad _unser_refused_class_name_30
    .quad 26
    .quad _unser_refused_class_name_31
    .quad 19
    .quad _unser_refused_class_name_32
    .quad 15
    .quad _unser_refused_class_name_33
    .quad 23
    .quad _unser_refused_class_name_34
    .quad 14
    .quad _unser_refused_class_name_35
    .quad 24
    .quad _unser_refused_class_name_36
    .quad 22
    .quad _unser_refused_class_name_37
    .quad 18
    .quad _unser_refused_class_name_38
    .quad 26
    .quad _unser_refused_class_name_39
    .quad 16
    .quad _unser_refused_class_name_40
    .quad 19
    .quad _unser_refused_class_name_41
    .quad 16
    .quad _unser_refused_class_name_42
    .quad 19
    .quad _unser_refused_class_name_43
    .quad 18
    .quad _unser_refused_class_name_44
    .quad 19
    .quad _unser_refused_class_name_45
    .quad 11
    .quad _unser_refused_class_name_46
    .quad 13
    .quad _unser_refused_class_name_47
    .quad 17
    .quad _unser_refused_class_name_48
    .quad 9
.globl _class_unserialize_data_boxed
_class_unserialize_data_boxed:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_propinit_ptrs
_class_propinit_ptrs:
    .quad 0
    .quad _class_propinit_1
    .quad _class_propinit_2
    .quad _class_propinit_3
    .quad _class_propinit_4
    .quad 0
    .quad 0
    .quad 0
    .quad _class_propinit_8
    .quad _class_propinit_9
    .quad 0
    .quad _class_propinit_11
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad _class_propinit_17
    .quad 0
.globl _class_initcheck_ptrs
_class_initcheck_ptrs:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_serprop_ptrs
_class_serprop_ptrs:
    .quad _class_serprop_missing
    .quad _class_serprop_1
    .quad _class_serprop_2
    .quad _class_serprop_3
    .quad _class_serprop_4
    .quad _class_serprop_missing
    .quad _class_serprop_missing
    .quad _class_serprop_missing
    .quad _class_serprop_8
    .quad _class_serprop_9
    .quad _class_serprop_missing
    .quad _class_serprop_11
    .quad _class_serprop_missing
    .quad _class_serprop_missing
    .quad _class_serprop_missing
    .quad _class_serprop_missing
    .quad _class_serprop_missing
    .quad _class_serprop_17
    .quad _class_serprop_18
.globl _class_serprop_declaring_ptrs
_class_serprop_declaring_ptrs:
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_1
    .quad _class_serprop_declaring_2
    .quad _class_serprop_declaring_3
    .quad _class_serprop_declaring_4
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_8
    .quad _class_serprop_declaring_9
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_11
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_missing
    .quad _class_serprop_declaring_17
    .quad _class_serprop_declaring_18
.globl _class_serpdiag_ptrs
_class_serpdiag_ptrs:
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_1
    .quad _class_serpdiag_2
    .quad _class_serpdiag_3
    .quad _class_serpdiag_4
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_8
    .quad _class_serpdiag_9
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_11
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_missing
    .quad _class_serpdiag_17
    .quad _class_serpdiag_18
.globl _class_static_vtable_ptrs
_class_static_vtable_ptrs:
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_1
    .quad _class_static_vtable_2
    .quad _class_static_vtable_3
    .quad _class_static_vtable_4
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_8
    .quad _class_static_vtable_9
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_11
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_17
    .quad _class_static_vtable_18
.globl _class_source_static_vtable_ptrs
_class_source_static_vtable_ptrs:
    .quad _class_static_vtable_missing
    .quad _class_source_static_vtable_1
    .quad _class_source_static_vtable_2
    .quad _class_source_static_vtable_3
    .quad _class_source_static_vtable_4
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_source_static_vtable_8
    .quad _class_source_static_vtable_9
    .quad _class_static_vtable_missing
    .quad _class_source_static_vtable_11
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_static_vtable_missing
    .quad _class_source_static_vtable_17
    .quad _class_source_static_vtable_18
.globl _class_callable_method_ptrs
_class_callable_method_ptrs:
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_1
    .quad _class_callable_methods_2
    .quad _class_callable_methods_3
    .quad _class_callable_methods_4
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_8
    .quad _class_callable_methods_9
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_11
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_missing
    .quad _class_callable_methods_17
    .quad _class_callable_methods_18
.p2align 3
.globl _user_wrapper_vtable_ptrs
_user_wrapper_vtable_ptrs:
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
    .quad _user_wrapper_vtable_missing
.p2align 3
.globl _user_filter_vtable_ptrs
_user_filter_vtable_ptrs:
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
    .quad _user_filter_vtable_missing
.globl _class_interfaces_missing
_class_interfaces_missing:
    .quad 0
.globl _class_gc_desc_missing
_class_gc_desc_missing:
    .byte 0
    .p2align 3
.globl _class_serprop_missing
_class_serprop_missing:
    .quad 0
.globl _class_serprop_declaring_missing
_class_serprop_declaring_missing:
    .quad -1
    .p2align 3
.globl _class_serpdiag_missing
_class_serpdiag_missing:
    .quad 0
    .quad 0
    .quad -1
    .quad 0
    .p2align 3
.globl _class_json_desc_missing
_class_json_desc_missing:
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_vd_desc_missing
_class_vd_desc_missing:
    .quad 0
    .p2align 3
.globl _class_prop_desc_missing
_class_prop_desc_missing:
    .quad 0
    .p2align 3
.globl _class_vtable_missing
_class_vtable_missing:
    .quad 0
    .p2align 3
.globl _class_static_vtable_missing
_class_static_vtable_missing:
    .quad 0
    .p2align 3
.globl _class_callable_methods_missing
_class_callable_methods_missing:
    .quad 0
    .p2align 3
.globl _user_wrapper_vtable_missing
_user_wrapper_vtable_missing:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _user_filter_vtable_missing
_user_filter_vtable_missing:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.p2align 3
.p2align 3
.globl _class_callable_static_method_count
_class_callable_static_method_count:
    .quad 0
.globl _class_callable_static_method_table
_class_callable_static_method_table:
.p2align 3
.globl _script_source_file
_script_source_file:
    .ascii "/Users/guillaumeloulier/PhpstormProjects/oss/elephc/.claude/worktrees/perf-php-src-execution-speed/benchmark/cases/native/mb_chr/main.php"
.p2align 3
.globl _script_source_file_len
_script_source_file_len:
    .quad 137
.p2align 3
.globl _class_by_name_str_1
_class_by_name_str_1:
    .ascii "Error"
.globl _class_by_name_str_2
_class_by_name_str_2:
    .ascii "TypeError"
.globl _class_by_name_str_3
_class_by_name_str_3:
    .ascii "ArgumentCountError"
.globl _class_by_name_str_4
_class_by_name_str_4:
    .ascii "ArithmeticError"
.globl _class_by_name_str_8
_class_by_name_str_8:
    .ascii "DivisionByZeroError"
.globl _class_by_name_str_9
_class_by_name_str_9:
    .ascii "Exception"
.globl _class_by_name_str_11
_class_by_name_str_11:
    .ascii "JsonException"
.globl _class_by_name_str_17
_class_by_name_str_17:
    .ascii "ValueError"
.globl _class_by_name_str_18
_class_by_name_str_18:
    .ascii "stdClass"
.p2align 3
.globl _classes_by_name_count
_classes_by_name_count:
    .quad 9
.globl _classes_by_name
_classes_by_name:
    .quad _class_by_name_str_1
    .quad 5
    .quad 1
    .quad 64
    .quad _class_by_name_str_2
    .quad 9
    .quad 2
    .quad 64
    .quad _class_by_name_str_3
    .quad 18
    .quad 3
    .quad 64
    .quad _class_by_name_str_4
    .quad 15
    .quad 4
    .quad 64
    .quad _class_by_name_str_8
    .quad 19
    .quad 8
    .quad 64
    .quad _class_by_name_str_9
    .quad 9
    .quad 9
    .quad 64
    .quad _class_by_name_str_11
    .quad 13
    .quad 11
    .quad 64
    .quad _class_by_name_str_17
    .quad 10
    .quad 17
    .quad 64
    .quad _class_by_name_str_18
    .quad 8
    .quad 18
    .quad 16
.p2align 3
.globl _class_attribute_count
_class_attribute_count:
    .quad 19
.globl _class_attribute_ptrs
_class_attribute_ptrs:
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
    .quad _class_attributes_missing
.globl _class_attributes_missing
_class_attributes_missing:
    .quad 0
.globl _interface_methods_14
_interface_methods_14:
    .quad 1
    .quad 0
.globl _interface_methods_15
_interface_methods_15:
    .quad 8
    .quad 0
    .quad 1
    .quad 2
    .quad 3
    .quad 4
    .quad 5
    .quad 6
    .quad 7
.globl _class_interfaces_1
_class_interfaces_1:
    .quad 2
    .quad 15
    .quad _class_interface_impl_1_15
    .quad 14
    .quad _class_interface_impl_1_14
.globl _class_interface_impl_1_15
_class_interface_impl_1_15:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_interface_impl_1_14
_class_interface_impl_1_14:
    .quad 0
.globl _class_json_pname_1_0
_class_json_pname_1_0:
    .ascii "message"
    .p2align 3
.globl _class_json_desc_1
_class_json_desc_1:
    .quad 0
    .quad 0
    .quad 1
    .quad _class_json_pname_1_0
    .quad 7
    .quad 0
    .quad 1
    .p2align 3
.globl _class_gc_desc_1
_class_gc_desc_1:
    .byte 1, 0, 7
.globl _class_serpname_1_0
_class_serpname_1_0:
    .byte 109, 101, 115, 115, 97, 103, 101
.globl _class_serpname_1_1
_class_serpname_1_1:
    .byte 0, 42, 0, 99, 111, 100, 101
.globl _class_serpname_1_2
_class_serpname_1_2:
    .byte 0, 42, 0, 112, 114, 101, 118, 105, 111, 117, 115
    .p2align 3
.globl _class_serprop_1
_class_serprop_1:
    .quad 3
    .quad _class_serpname_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_serpname_1_1
    .quad 7
    .quad 24
    .quad 0
    .quad _class_serpname_1_2
    .quad 11
    .quad 40
    .quad 7
    .p2align 3
.globl _class_serprop_declaring_1
_class_serprop_declaring_1:
    .quad 1
    .quad 1
    .quad 1
.globl _class_serpsuffix_1_0
_class_serpsuffix_1_0:
    .ascii " to property Error::$message of type string"
.globl _class_serpsuffix_1_1
_class_serpsuffix_1_1:
    .ascii " to property Error::$code of type int"
.globl _class_serpsuffix_1_2
_class_serpsuffix_1_2:
    .ascii " to property Error::$previous of type ?Throwable"
    .p2align 3
.globl _class_serpdiag_1
_class_serpdiag_1:
    .quad _class_serpsuffix_1_0
    .quad 43
    .quad 2
    .quad 0
    .quad _class_serpsuffix_1_1
    .quad 37
    .quad 1
    .quad 0
    .quad _class_serpsuffix_1_2
    .quad 48
    .quad 320
    .quad 0
.globl _class_vd_pkey_1_0
_class_vd_pkey_1_0:
    .ascii "\"message\""
.globl _class_vd_ptype_1_0
_class_vd_ptype_1_0:
    .ascii "string"
.globl _class_vd_pkey_1_1
_class_vd_pkey_1_1:
    .ascii "\"code\":protected"
.globl _class_vd_ptype_1_1
_class_vd_ptype_1_1:
    .ascii "int"
.globl _class_vd_pkey_1_2
_class_vd_pkey_1_2:
    .ascii "\"previous\":protected"
.globl _class_vd_ptype_1_2
_class_vd_ptype_1_2:
    .ascii "mixed"
    .p2align 3
.globl _class_vd_desc_1
_class_vd_desc_1:
    .quad 3
    .quad _class_vd_pkey_1_0
    .quad 9
    .quad 8
    .quad 1
    .quad _class_vd_ptype_1_0
    .quad 6
    .quad _class_vd_pkey_1_1
    .quad 16
    .quad 24
    .quad 0
    .quad _class_vd_ptype_1_1
    .quad 3
    .quad _class_vd_pkey_1_2
    .quad 20
    .quad 40
    .quad 7
    .quad _class_vd_ptype_1_2
    .quad 5
.globl _class_prop_pkey_1_0
_class_prop_pkey_1_0:
    .ascii "message"
.globl _class_prop_pkey_1_1
_class_prop_pkey_1_1:
    .ascii "code:protected"
.globl _class_prop_nkey_1_1
_class_prop_nkey_1_1:
    .ascii "code"
.globl _class_prop_pkey_1_2
_class_prop_pkey_1_2:
    .ascii "previous:protected"
.globl _class_prop_nkey_1_2
_class_prop_nkey_1_2:
    .ascii "previous"
    .p2align 3
.globl _class_prop_desc_1
_class_prop_desc_1:
    .quad 3
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad _class_prop_pkey_1_1
    .quad 14
    .quad 24
    .quad 0
    .quad _class_prop_nkey_1_1
    .quad 4
    .quad _class_prop_pkey_1_2
    .quad 18
    .quad 40
    .quad 7
    .quad _class_prop_nkey_1_2
    .quad 8
    .p2align 3
.globl _class_vtable_1
_class_vtable_1:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_source_vtable_1
_class_source_vtable_1:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_static_vtable_1
_class_static_vtable_1:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_1
_class_source_static_vtable_1:
    .quad 0
.globl _class_callable_method_name_1__u__u_construct
_class_callable_method_name_1__u__u_construct:
    .ascii "__construct"
.globl _class_callable_method_name_1__u__u_tostring
_class_callable_method_name_1__u__u_tostring:
    .ascii "__tostring"
.globl _class_callable_method_name_1_getcode
_class_callable_method_name_1_getcode:
    .ascii "getcode"
.globl _class_callable_method_name_1_getfile
_class_callable_method_name_1_getfile:
    .ascii "getfile"
.globl _class_callable_method_name_1_getline
_class_callable_method_name_1_getline:
    .ascii "getline"
.globl _class_callable_method_name_1_getmessage
_class_callable_method_name_1_getmessage:
    .ascii "getmessage"
.globl _class_callable_method_name_1_getprevious
_class_callable_method_name_1_getprevious:
    .ascii "getprevious"
.globl _class_callable_method_name_1_gettrace
_class_callable_method_name_1_gettrace:
    .ascii "gettrace"
.globl _class_callable_method_name_1_gettraceasstring
_class_callable_method_name_1_gettraceasstring:
    .ascii "gettraceasstring"
.p2align 3
.globl _class_callable_methods_1
_class_callable_methods_1:
    .quad 9
    .quad _class_callable_method_name_1__u__u_construct
    .quad 11
    .quad _class_callable_method_name_1__u__u_tostring
    .quad 10
    .quad _class_callable_method_name_1_getcode
    .quad 7
    .quad _class_callable_method_name_1_getfile
    .quad 7
    .quad _class_callable_method_name_1_getline
    .quad 7
    .quad _class_callable_method_name_1_getmessage
    .quad 10
    .quad _class_callable_method_name_1_getprevious
    .quad 11
    .quad _class_callable_method_name_1_gettrace
    .quad 8
    .quad _class_callable_method_name_1_gettraceasstring
    .quad 16
.globl _class_interfaces_2
_class_interfaces_2:
    .quad 2
    .quad 15
    .quad _class_interface_impl_2_15
    .quad 14
    .quad _class_interface_impl_2_14
.globl _class_interface_impl_2_15
_class_interface_impl_2_15:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_interface_impl_2_14
_class_interface_impl_2_14:
    .quad 0
.globl _class_json_pname_2_0
_class_json_pname_2_0:
    .ascii "message"
    .p2align 3
.globl _class_json_desc_2
_class_json_desc_2:
    .quad 0
    .quad 0
    .quad 1
    .quad _class_json_pname_2_0
    .quad 7
    .quad 0
    .quad 1
    .p2align 3
.globl _class_gc_desc_2
_class_gc_desc_2:
    .byte 1, 0, 7
.globl _class_serpname_2_0
_class_serpname_2_0:
    .byte 109, 101, 115, 115, 97, 103, 101
.globl _class_serpname_2_1
_class_serpname_2_1:
    .byte 0, 42, 0, 99, 111, 100, 101
.globl _class_serpname_2_2
_class_serpname_2_2:
    .byte 0, 42, 0, 112, 114, 101, 118, 105, 111, 117, 115
    .p2align 3
.globl _class_serprop_2
_class_serprop_2:
    .quad 3
    .quad _class_serpname_2_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_serpname_2_1
    .quad 7
    .quad 24
    .quad 0
    .quad _class_serpname_2_2
    .quad 11
    .quad 40
    .quad 7
    .p2align 3
.globl _class_serprop_declaring_2
_class_serprop_declaring_2:
    .quad 1
    .quad 1
    .quad 1
.globl _class_serpsuffix_2_0
_class_serpsuffix_2_0:
    .ascii " to property Error::$message of type string"
.globl _class_serpsuffix_2_1
_class_serpsuffix_2_1:
    .ascii " to property Error::$code of type int"
.globl _class_serpsuffix_2_2
_class_serpsuffix_2_2:
    .ascii " to property Error::$previous of type ?Throwable"
    .p2align 3
.globl _class_serpdiag_2
_class_serpdiag_2:
    .quad _class_serpsuffix_2_0
    .quad 43
    .quad 2
    .quad 0
    .quad _class_serpsuffix_2_1
    .quad 37
    .quad 1
    .quad 0
    .quad _class_serpsuffix_2_2
    .quad 48
    .quad 320
    .quad 0
    .p2align 3
.globl _class_vd_desc_2
_class_vd_desc_2:
    .quad 3
    .quad _class_vd_pkey_1_0
    .quad 9
    .quad 8
    .quad 1
    .quad _class_vd_ptype_1_0
    .quad 6
    .quad _class_vd_pkey_1_1
    .quad 16
    .quad 24
    .quad 0
    .quad _class_vd_ptype_1_1
    .quad 3
    .quad _class_vd_pkey_1_2
    .quad 20
    .quad 40
    .quad 7
    .quad _class_vd_ptype_1_2
    .quad 5
    .p2align 3
.globl _class_prop_desc_2
_class_prop_desc_2:
    .quad 3
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad _class_prop_pkey_1_1
    .quad 14
    .quad 24
    .quad 0
    .quad _class_prop_nkey_1_1
    .quad 4
    .quad _class_prop_pkey_1_2
    .quad 18
    .quad 40
    .quad 7
    .quad _class_prop_nkey_1_2
    .quad 8
    .p2align 3
.globl _class_vtable_2
_class_vtable_2:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_source_vtable_2
_class_source_vtable_2:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_static_vtable_2
_class_static_vtable_2:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_2
_class_source_static_vtable_2:
    .quad 0
.p2align 3
.globl _class_callable_methods_2
_class_callable_methods_2:
    .quad 9
    .quad _class_callable_method_name_1__u__u_construct
    .quad 11
    .quad _class_callable_method_name_1__u__u_tostring
    .quad 10
    .quad _class_callable_method_name_1_getcode
    .quad 7
    .quad _class_callable_method_name_1_getfile
    .quad 7
    .quad _class_callable_method_name_1_getline
    .quad 7
    .quad _class_callable_method_name_1_getmessage
    .quad 10
    .quad _class_callable_method_name_1_getprevious
    .quad 11
    .quad _class_callable_method_name_1_gettrace
    .quad 8
    .quad _class_callable_method_name_1_gettraceasstring
    .quad 16
.globl _class_interfaces_3
_class_interfaces_3:
    .quad 2
    .quad 15
    .quad _class_interface_impl_3_15
    .quad 14
    .quad _class_interface_impl_3_14
.globl _class_interface_impl_3_15
_class_interface_impl_3_15:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_interface_impl_3_14
_class_interface_impl_3_14:
    .quad 0
.globl _class_json_pname_3_0
_class_json_pname_3_0:
    .ascii "message"
    .p2align 3
.globl _class_json_desc_3
_class_json_desc_3:
    .quad 0
    .quad 0
    .quad 1
    .quad _class_json_pname_3_0
    .quad 7
    .quad 0
    .quad 1
    .p2align 3
.globl _class_gc_desc_3
_class_gc_desc_3:
    .byte 1, 0, 7
.globl _class_serpname_3_0
_class_serpname_3_0:
    .byte 109, 101, 115, 115, 97, 103, 101
.globl _class_serpname_3_1
_class_serpname_3_1:
    .byte 0, 42, 0, 99, 111, 100, 101
.globl _class_serpname_3_2
_class_serpname_3_2:
    .byte 0, 42, 0, 112, 114, 101, 118, 105, 111, 117, 115
    .p2align 3
.globl _class_serprop_3
_class_serprop_3:
    .quad 3
    .quad _class_serpname_3_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_serpname_3_1
    .quad 7
    .quad 24
    .quad 0
    .quad _class_serpname_3_2
    .quad 11
    .quad 40
    .quad 7
    .p2align 3
.globl _class_serprop_declaring_3
_class_serprop_declaring_3:
    .quad 1
    .quad 1
    .quad 1
.globl _class_serpsuffix_3_0
_class_serpsuffix_3_0:
    .ascii " to property Error::$message of type string"
.globl _class_serpsuffix_3_1
_class_serpsuffix_3_1:
    .ascii " to property Error::$code of type int"
.globl _class_serpsuffix_3_2
_class_serpsuffix_3_2:
    .ascii " to property Error::$previous of type ?Throwable"
    .p2align 3
.globl _class_serpdiag_3
_class_serpdiag_3:
    .quad _class_serpsuffix_3_0
    .quad 43
    .quad 2
    .quad 0
    .quad _class_serpsuffix_3_1
    .quad 37
    .quad 1
    .quad 0
    .quad _class_serpsuffix_3_2
    .quad 48
    .quad 320
    .quad 0
    .p2align 3
.globl _class_vd_desc_3
_class_vd_desc_3:
    .quad 3
    .quad _class_vd_pkey_1_0
    .quad 9
    .quad 8
    .quad 1
    .quad _class_vd_ptype_1_0
    .quad 6
    .quad _class_vd_pkey_1_1
    .quad 16
    .quad 24
    .quad 0
    .quad _class_vd_ptype_1_1
    .quad 3
    .quad _class_vd_pkey_1_2
    .quad 20
    .quad 40
    .quad 7
    .quad _class_vd_ptype_1_2
    .quad 5
    .p2align 3
.globl _class_prop_desc_3
_class_prop_desc_3:
    .quad 3
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad _class_prop_pkey_1_1
    .quad 14
    .quad 24
    .quad 0
    .quad _class_prop_nkey_1_1
    .quad 4
    .quad _class_prop_pkey_1_2
    .quad 18
    .quad 40
    .quad 7
    .quad _class_prop_nkey_1_2
    .quad 8
    .p2align 3
.globl _class_vtable_3
_class_vtable_3:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_source_vtable_3
_class_source_vtable_3:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_static_vtable_3
_class_static_vtable_3:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_3
_class_source_static_vtable_3:
    .quad 0
.p2align 3
.globl _class_callable_methods_3
_class_callable_methods_3:
    .quad 9
    .quad _class_callable_method_name_1__u__u_construct
    .quad 11
    .quad _class_callable_method_name_1__u__u_tostring
    .quad 10
    .quad _class_callable_method_name_1_getcode
    .quad 7
    .quad _class_callable_method_name_1_getfile
    .quad 7
    .quad _class_callable_method_name_1_getline
    .quad 7
    .quad _class_callable_method_name_1_getmessage
    .quad 10
    .quad _class_callable_method_name_1_getprevious
    .quad 11
    .quad _class_callable_method_name_1_gettrace
    .quad 8
    .quad _class_callable_method_name_1_gettraceasstring
    .quad 16
.globl _class_interfaces_4
_class_interfaces_4:
    .quad 2
    .quad 15
    .quad _class_interface_impl_4_15
    .quad 14
    .quad _class_interface_impl_4_14
.globl _class_interface_impl_4_15
_class_interface_impl_4_15:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_interface_impl_4_14
_class_interface_impl_4_14:
    .quad 0
.globl _class_json_pname_4_0
_class_json_pname_4_0:
    .ascii "message"
    .p2align 3
.globl _class_json_desc_4
_class_json_desc_4:
    .quad 0
    .quad 0
    .quad 1
    .quad _class_json_pname_4_0
    .quad 7
    .quad 0
    .quad 1
    .p2align 3
.globl _class_gc_desc_4
_class_gc_desc_4:
    .byte 1, 0, 7
.globl _class_serpname_4_0
_class_serpname_4_0:
    .byte 109, 101, 115, 115, 97, 103, 101
.globl _class_serpname_4_1
_class_serpname_4_1:
    .byte 0, 42, 0, 99, 111, 100, 101
.globl _class_serpname_4_2
_class_serpname_4_2:
    .byte 0, 42, 0, 112, 114, 101, 118, 105, 111, 117, 115
    .p2align 3
.globl _class_serprop_4
_class_serprop_4:
    .quad 3
    .quad _class_serpname_4_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_serpname_4_1
    .quad 7
    .quad 24
    .quad 0
    .quad _class_serpname_4_2
    .quad 11
    .quad 40
    .quad 7
    .p2align 3
.globl _class_serprop_declaring_4
_class_serprop_declaring_4:
    .quad 1
    .quad 1
    .quad 1
.globl _class_serpsuffix_4_0
_class_serpsuffix_4_0:
    .ascii " to property Error::$message of type string"
.globl _class_serpsuffix_4_1
_class_serpsuffix_4_1:
    .ascii " to property Error::$code of type int"
.globl _class_serpsuffix_4_2
_class_serpsuffix_4_2:
    .ascii " to property Error::$previous of type ?Throwable"
    .p2align 3
.globl _class_serpdiag_4
_class_serpdiag_4:
    .quad _class_serpsuffix_4_0
    .quad 43
    .quad 2
    .quad 0
    .quad _class_serpsuffix_4_1
    .quad 37
    .quad 1
    .quad 0
    .quad _class_serpsuffix_4_2
    .quad 48
    .quad 320
    .quad 0
    .p2align 3
.globl _class_vd_desc_4
_class_vd_desc_4:
    .quad 3
    .quad _class_vd_pkey_1_0
    .quad 9
    .quad 8
    .quad 1
    .quad _class_vd_ptype_1_0
    .quad 6
    .quad _class_vd_pkey_1_1
    .quad 16
    .quad 24
    .quad 0
    .quad _class_vd_ptype_1_1
    .quad 3
    .quad _class_vd_pkey_1_2
    .quad 20
    .quad 40
    .quad 7
    .quad _class_vd_ptype_1_2
    .quad 5
    .p2align 3
.globl _class_prop_desc_4
_class_prop_desc_4:
    .quad 3
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad _class_prop_pkey_1_1
    .quad 14
    .quad 24
    .quad 0
    .quad _class_prop_nkey_1_1
    .quad 4
    .quad _class_prop_pkey_1_2
    .quad 18
    .quad 40
    .quad 7
    .quad _class_prop_nkey_1_2
    .quad 8
    .p2align 3
.globl _class_vtable_4
_class_vtable_4:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_source_vtable_4
_class_source_vtable_4:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_static_vtable_4
_class_static_vtable_4:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_4
_class_source_static_vtable_4:
    .quad 0
.p2align 3
.globl _class_callable_methods_4
_class_callable_methods_4:
    .quad 9
    .quad _class_callable_method_name_1__u__u_construct
    .quad 11
    .quad _class_callable_method_name_1__u__u_tostring
    .quad 10
    .quad _class_callable_method_name_1_getcode
    .quad 7
    .quad _class_callable_method_name_1_getfile
    .quad 7
    .quad _class_callable_method_name_1_getline
    .quad 7
    .quad _class_callable_method_name_1_getmessage
    .quad 10
    .quad _class_callable_method_name_1_getprevious
    .quad 11
    .quad _class_callable_method_name_1_gettrace
    .quad 8
    .quad _class_callable_method_name_1_gettraceasstring
    .quad 16
.globl _class_interfaces_8
_class_interfaces_8:
    .quad 2
    .quad 15
    .quad _class_interface_impl_8_15
    .quad 14
    .quad _class_interface_impl_8_14
.globl _class_interface_impl_8_15
_class_interface_impl_8_15:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_interface_impl_8_14
_class_interface_impl_8_14:
    .quad 0
.globl _class_json_pname_8_0
_class_json_pname_8_0:
    .ascii "message"
    .p2align 3
.globl _class_json_desc_8
_class_json_desc_8:
    .quad 0
    .quad 0
    .quad 1
    .quad _class_json_pname_8_0
    .quad 7
    .quad 0
    .quad 1
    .p2align 3
.globl _class_gc_desc_8
_class_gc_desc_8:
    .byte 1, 0, 7
.globl _class_serpname_8_0
_class_serpname_8_0:
    .byte 109, 101, 115, 115, 97, 103, 101
.globl _class_serpname_8_1
_class_serpname_8_1:
    .byte 0, 42, 0, 99, 111, 100, 101
.globl _class_serpname_8_2
_class_serpname_8_2:
    .byte 0, 42, 0, 112, 114, 101, 118, 105, 111, 117, 115
    .p2align 3
.globl _class_serprop_8
_class_serprop_8:
    .quad 3
    .quad _class_serpname_8_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_serpname_8_1
    .quad 7
    .quad 24
    .quad 0
    .quad _class_serpname_8_2
    .quad 11
    .quad 40
    .quad 7
    .p2align 3
.globl _class_serprop_declaring_8
_class_serprop_declaring_8:
    .quad 1
    .quad 1
    .quad 1
.globl _class_serpsuffix_8_0
_class_serpsuffix_8_0:
    .ascii " to property Error::$message of type string"
.globl _class_serpsuffix_8_1
_class_serpsuffix_8_1:
    .ascii " to property Error::$code of type int"
.globl _class_serpsuffix_8_2
_class_serpsuffix_8_2:
    .ascii " to property Error::$previous of type ?Throwable"
    .p2align 3
.globl _class_serpdiag_8
_class_serpdiag_8:
    .quad _class_serpsuffix_8_0
    .quad 43
    .quad 2
    .quad 0
    .quad _class_serpsuffix_8_1
    .quad 37
    .quad 1
    .quad 0
    .quad _class_serpsuffix_8_2
    .quad 48
    .quad 320
    .quad 0
    .p2align 3
.globl _class_vd_desc_8
_class_vd_desc_8:
    .quad 3
    .quad _class_vd_pkey_1_0
    .quad 9
    .quad 8
    .quad 1
    .quad _class_vd_ptype_1_0
    .quad 6
    .quad _class_vd_pkey_1_1
    .quad 16
    .quad 24
    .quad 0
    .quad _class_vd_ptype_1_1
    .quad 3
    .quad _class_vd_pkey_1_2
    .quad 20
    .quad 40
    .quad 7
    .quad _class_vd_ptype_1_2
    .quad 5
    .p2align 3
.globl _class_prop_desc_8
_class_prop_desc_8:
    .quad 3
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad _class_prop_pkey_1_1
    .quad 14
    .quad 24
    .quad 0
    .quad _class_prop_nkey_1_1
    .quad 4
    .quad _class_prop_pkey_1_2
    .quad 18
    .quad 40
    .quad 7
    .quad _class_prop_nkey_1_2
    .quad 8
    .p2align 3
.globl _class_vtable_8
_class_vtable_8:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_source_vtable_8
_class_source_vtable_8:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_static_vtable_8
_class_static_vtable_8:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_8
_class_source_static_vtable_8:
    .quad 0
.p2align 3
.globl _class_callable_methods_8
_class_callable_methods_8:
    .quad 9
    .quad _class_callable_method_name_1__u__u_construct
    .quad 11
    .quad _class_callable_method_name_1__u__u_tostring
    .quad 10
    .quad _class_callable_method_name_1_getcode
    .quad 7
    .quad _class_callable_method_name_1_getfile
    .quad 7
    .quad _class_callable_method_name_1_getline
    .quad 7
    .quad _class_callable_method_name_1_getmessage
    .quad 10
    .quad _class_callable_method_name_1_getprevious
    .quad 11
    .quad _class_callable_method_name_1_gettrace
    .quad 8
    .quad _class_callable_method_name_1_gettraceasstring
    .quad 16
.globl _class_interfaces_9
_class_interfaces_9:
    .quad 2
    .quad 15
    .quad _class_interface_impl_9_15
    .quad 14
    .quad _class_interface_impl_9_14
.globl _class_interface_impl_9_15
_class_interface_impl_9_15:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_interface_impl_9_14
_class_interface_impl_9_14:
    .quad 0
.globl _class_json_pname_9_0
_class_json_pname_9_0:
    .ascii "message"
    .p2align 3
.globl _class_json_desc_9
_class_json_desc_9:
    .quad 0
    .quad 0
    .quad 1
    .quad _class_json_pname_9_0
    .quad 7
    .quad 0
    .quad 1
    .p2align 3
.globl _class_gc_desc_9
_class_gc_desc_9:
    .byte 1, 0, 7
.globl _class_serpname_9_0
_class_serpname_9_0:
    .byte 109, 101, 115, 115, 97, 103, 101
.globl _class_serpname_9_1
_class_serpname_9_1:
    .byte 0, 42, 0, 99, 111, 100, 101
.globl _class_serpname_9_2
_class_serpname_9_2:
    .byte 0, 42, 0, 112, 114, 101, 118, 105, 111, 117, 115
    .p2align 3
.globl _class_serprop_9
_class_serprop_9:
    .quad 3
    .quad _class_serpname_9_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_serpname_9_1
    .quad 7
    .quad 24
    .quad 0
    .quad _class_serpname_9_2
    .quad 11
    .quad 40
    .quad 7
    .p2align 3
.globl _class_serprop_declaring_9
_class_serprop_declaring_9:
    .quad 9
    .quad 9
    .quad 9
.globl _class_serpsuffix_9_0
_class_serpsuffix_9_0:
    .ascii " to property Exception::$message of type string"
.globl _class_serpsuffix_9_1
_class_serpsuffix_9_1:
    .ascii " to property Exception::$code of type int"
.globl _class_serpsuffix_9_2
_class_serpsuffix_9_2:
    .ascii " to property Exception::$previous of type ?Throwable"
    .p2align 3
.globl _class_serpdiag_9
_class_serpdiag_9:
    .quad _class_serpsuffix_9_0
    .quad 47
    .quad 2
    .quad 0
    .quad _class_serpsuffix_9_1
    .quad 41
    .quad 1
    .quad 0
    .quad _class_serpsuffix_9_2
    .quad 52
    .quad 320
    .quad 0
    .p2align 3
.globl _class_vd_desc_9
_class_vd_desc_9:
    .quad 3
    .quad _class_vd_pkey_1_0
    .quad 9
    .quad 8
    .quad 1
    .quad _class_vd_ptype_1_0
    .quad 6
    .quad _class_vd_pkey_1_1
    .quad 16
    .quad 24
    .quad 0
    .quad _class_vd_ptype_1_1
    .quad 3
    .quad _class_vd_pkey_1_2
    .quad 20
    .quad 40
    .quad 7
    .quad _class_vd_ptype_1_2
    .quad 5
    .p2align 3
.globl _class_prop_desc_9
_class_prop_desc_9:
    .quad 3
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad _class_prop_pkey_1_1
    .quad 14
    .quad 24
    .quad 0
    .quad _class_prop_nkey_1_1
    .quad 4
    .quad _class_prop_pkey_1_2
    .quad 18
    .quad 40
    .quad 7
    .quad _class_prop_nkey_1_2
    .quad 8
    .p2align 3
.globl _class_vtable_9
_class_vtable_9:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_source_vtable_9
_class_source_vtable_9:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_static_vtable_9
_class_static_vtable_9:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_9
_class_source_static_vtable_9:
    .quad 0
.p2align 3
.globl _class_callable_methods_9
_class_callable_methods_9:
    .quad 9
    .quad _class_callable_method_name_1__u__u_construct
    .quad 11
    .quad _class_callable_method_name_1__u__u_tostring
    .quad 10
    .quad _class_callable_method_name_1_getcode
    .quad 7
    .quad _class_callable_method_name_1_getfile
    .quad 7
    .quad _class_callable_method_name_1_getline
    .quad 7
    .quad _class_callable_method_name_1_getmessage
    .quad 10
    .quad _class_callable_method_name_1_getprevious
    .quad 11
    .quad _class_callable_method_name_1_gettrace
    .quad 8
    .quad _class_callable_method_name_1_gettraceasstring
    .quad 16
.globl _class_interfaces_11
_class_interfaces_11:
    .quad 2
    .quad 15
    .quad _class_interface_impl_11_15
    .quad 14
    .quad _class_interface_impl_11_14
.globl _class_interface_impl_11_15
_class_interface_impl_11_15:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_interface_impl_11_14
_class_interface_impl_11_14:
    .quad 0
.globl _class_json_pname_11_0
_class_json_pname_11_0:
    .ascii "message"
    .p2align 3
.globl _class_json_desc_11
_class_json_desc_11:
    .quad 0
    .quad 0
    .quad 1
    .quad _class_json_pname_11_0
    .quad 7
    .quad 0
    .quad 1
    .p2align 3
.globl _class_gc_desc_11
_class_gc_desc_11:
    .byte 1, 0, 7
.globl _class_serpname_11_0
_class_serpname_11_0:
    .byte 109, 101, 115, 115, 97, 103, 101
.globl _class_serpname_11_1
_class_serpname_11_1:
    .byte 0, 42, 0, 99, 111, 100, 101
.globl _class_serpname_11_2
_class_serpname_11_2:
    .byte 0, 42, 0, 112, 114, 101, 118, 105, 111, 117, 115
    .p2align 3
.globl _class_serprop_11
_class_serprop_11:
    .quad 3
    .quad _class_serpname_11_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_serpname_11_1
    .quad 7
    .quad 24
    .quad 0
    .quad _class_serpname_11_2
    .quad 11
    .quad 40
    .quad 7
    .p2align 3
.globl _class_serprop_declaring_11
_class_serprop_declaring_11:
    .quad 9
    .quad 9
    .quad 9
.globl _class_serpsuffix_11_0
_class_serpsuffix_11_0:
    .ascii " to property Exception::$message of type string"
.globl _class_serpsuffix_11_1
_class_serpsuffix_11_1:
    .ascii " to property Exception::$code of type int"
.globl _class_serpsuffix_11_2
_class_serpsuffix_11_2:
    .ascii " to property Exception::$previous of type ?Throwable"
    .p2align 3
.globl _class_serpdiag_11
_class_serpdiag_11:
    .quad _class_serpsuffix_11_0
    .quad 47
    .quad 2
    .quad 0
    .quad _class_serpsuffix_11_1
    .quad 41
    .quad 1
    .quad 0
    .quad _class_serpsuffix_11_2
    .quad 52
    .quad 320
    .quad 0
    .p2align 3
.globl _class_vd_desc_11
_class_vd_desc_11:
    .quad 3
    .quad _class_vd_pkey_1_0
    .quad 9
    .quad 8
    .quad 1
    .quad _class_vd_ptype_1_0
    .quad 6
    .quad _class_vd_pkey_1_1
    .quad 16
    .quad 24
    .quad 0
    .quad _class_vd_ptype_1_1
    .quad 3
    .quad _class_vd_pkey_1_2
    .quad 20
    .quad 40
    .quad 7
    .quad _class_vd_ptype_1_2
    .quad 5
    .p2align 3
.globl _class_prop_desc_11
_class_prop_desc_11:
    .quad 3
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad _class_prop_pkey_1_1
    .quad 14
    .quad 24
    .quad 0
    .quad _class_prop_nkey_1_1
    .quad 4
    .quad _class_prop_pkey_1_2
    .quad 18
    .quad 40
    .quad 7
    .quad _class_prop_nkey_1_2
    .quad 8
    .p2align 3
.globl _class_vtable_11
_class_vtable_11:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_source_vtable_11
_class_source_vtable_11:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_static_vtable_11
_class_static_vtable_11:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_11
_class_source_static_vtable_11:
    .quad 0
.p2align 3
.globl _class_callable_methods_11
_class_callable_methods_11:
    .quad 9
    .quad _class_callable_method_name_1__u__u_construct
    .quad 11
    .quad _class_callable_method_name_1__u__u_tostring
    .quad 10
    .quad _class_callable_method_name_1_getcode
    .quad 7
    .quad _class_callable_method_name_1_getfile
    .quad 7
    .quad _class_callable_method_name_1_getline
    .quad 7
    .quad _class_callable_method_name_1_getmessage
    .quad 10
    .quad _class_callable_method_name_1_getprevious
    .quad 11
    .quad _class_callable_method_name_1_gettrace
    .quad 8
    .quad _class_callable_method_name_1_gettraceasstring
    .quad 16
.globl _class_interfaces_17
_class_interfaces_17:
    .quad 2
    .quad 15
    .quad _class_interface_impl_17_15
    .quad 14
    .quad _class_interface_impl_17_14
.globl _class_interface_impl_17_15
_class_interface_impl_17_15:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
.globl _class_interface_impl_17_14
_class_interface_impl_17_14:
    .quad 0
.globl _class_json_pname_17_0
_class_json_pname_17_0:
    .ascii "message"
    .p2align 3
.globl _class_json_desc_17
_class_json_desc_17:
    .quad 0
    .quad 0
    .quad 1
    .quad _class_json_pname_17_0
    .quad 7
    .quad 0
    .quad 1
    .p2align 3
.globl _class_gc_desc_17
_class_gc_desc_17:
    .byte 1, 0, 7
.globl _class_serpname_17_0
_class_serpname_17_0:
    .byte 109, 101, 115, 115, 97, 103, 101
.globl _class_serpname_17_1
_class_serpname_17_1:
    .byte 0, 42, 0, 99, 111, 100, 101
.globl _class_serpname_17_2
_class_serpname_17_2:
    .byte 0, 42, 0, 112, 114, 101, 118, 105, 111, 117, 115
    .p2align 3
.globl _class_serprop_17
_class_serprop_17:
    .quad 3
    .quad _class_serpname_17_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_serpname_17_1
    .quad 7
    .quad 24
    .quad 0
    .quad _class_serpname_17_2
    .quad 11
    .quad 40
    .quad 7
    .p2align 3
.globl _class_serprop_declaring_17
_class_serprop_declaring_17:
    .quad 1
    .quad 1
    .quad 1
.globl _class_serpsuffix_17_0
_class_serpsuffix_17_0:
    .ascii " to property Error::$message of type string"
.globl _class_serpsuffix_17_1
_class_serpsuffix_17_1:
    .ascii " to property Error::$code of type int"
.globl _class_serpsuffix_17_2
_class_serpsuffix_17_2:
    .ascii " to property Error::$previous of type ?Throwable"
    .p2align 3
.globl _class_serpdiag_17
_class_serpdiag_17:
    .quad _class_serpsuffix_17_0
    .quad 43
    .quad 2
    .quad 0
    .quad _class_serpsuffix_17_1
    .quad 37
    .quad 1
    .quad 0
    .quad _class_serpsuffix_17_2
    .quad 48
    .quad 320
    .quad 0
    .p2align 3
.globl _class_vd_desc_17
_class_vd_desc_17:
    .quad 3
    .quad _class_vd_pkey_1_0
    .quad 9
    .quad 8
    .quad 1
    .quad _class_vd_ptype_1_0
    .quad 6
    .quad _class_vd_pkey_1_1
    .quad 16
    .quad 24
    .quad 0
    .quad _class_vd_ptype_1_1
    .quad 3
    .quad _class_vd_pkey_1_2
    .quad 20
    .quad 40
    .quad 7
    .quad _class_vd_ptype_1_2
    .quad 5
    .p2align 3
.globl _class_prop_desc_17
_class_prop_desc_17:
    .quad 3
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad 8
    .quad 1
    .quad _class_prop_pkey_1_0
    .quad 7
    .quad _class_prop_pkey_1_1
    .quad 14
    .quad 24
    .quad 0
    .quad _class_prop_nkey_1_1
    .quad 4
    .quad _class_prop_pkey_1_2
    .quad 18
    .quad 40
    .quad 7
    .quad _class_prop_nkey_1_2
    .quad 8
    .p2align 3
.globl _class_vtable_17
_class_vtable_17:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_source_vtable_17
_class_source_vtable_17:
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_static_vtable_17
_class_static_vtable_17:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_17
_class_source_static_vtable_17:
    .quad 0
.p2align 3
.globl _class_callable_methods_17
_class_callable_methods_17:
    .quad 9
    .quad _class_callable_method_name_1__u__u_construct
    .quad 11
    .quad _class_callable_method_name_1__u__u_tostring
    .quad 10
    .quad _class_callable_method_name_1_getcode
    .quad 7
    .quad _class_callable_method_name_1_getfile
    .quad 7
    .quad _class_callable_method_name_1_getline
    .quad 7
    .quad _class_callable_method_name_1_getmessage
    .quad 10
    .quad _class_callable_method_name_1_getprevious
    .quad 11
    .quad _class_callable_method_name_1_gettrace
    .quad 8
    .quad _class_callable_method_name_1_gettraceasstring
    .quad 16
.globl _class_interfaces_18
_class_interfaces_18:
    .quad 0
    .p2align 3
.globl _class_json_desc_18
_class_json_desc_18:
    .quad 0
    .quad 0
    .quad 0
    .p2align 3
.globl _class_gc_desc_18
_class_gc_desc_18:
    .byte 0
    .p2align 3
.globl _class_serprop_18
_class_serprop_18:
    .quad 0
    .p2align 3
.globl _class_serprop_declaring_18
_class_serprop_declaring_18:
    .p2align 3
.globl _class_serpdiag_18
_class_serpdiag_18:
    .p2align 3
.globl _class_vd_desc_18
_class_vd_desc_18:
    .quad 0
    .p2align 3
.globl _class_prop_desc_18
_class_prop_desc_18:
    .quad 0
    .p2align 3
.globl _class_vtable_18
_class_vtable_18:
    .quad 0
    .p2align 3
.globl _class_source_vtable_18
_class_source_vtable_18:
    .quad 0
    .p2align 3
.globl _class_static_vtable_18
_class_static_vtable_18:
    .quad 0
    .p2align 3
.globl _class_source_static_vtable_18
_class_source_static_vtable_18:
    .quad 0
.p2align 3
.globl _class_callable_methods_18
_class_callable_methods_18:
    .quad 0
.p2align 3
.globl _stdclass_class_id
_stdclass_class_id:
    .quad 18

// -- internal symbols are local to the cdylib public ABI --
.hidden _class_propinit_1
.hidden _class_propinit_1__cdylib_exception_cleanup
.hidden _class_propinit_2
.hidden _class_propinit_2__cdylib_exception_cleanup
.hidden _class_propinit_3
.hidden _class_propinit_3__cdylib_exception_cleanup
.hidden _class_propinit_4
.hidden _class_propinit_4__cdylib_exception_cleanup
.hidden _class_propinit_8
.hidden _class_propinit_8__cdylib_exception_cleanup
.hidden _class_propinit_9
.hidden _class_propinit_9__cdylib_exception_cleanup
.hidden _class_propinit_11
.hidden _class_propinit_11__cdylib_exception_cleanup
.hidden _class_propinit_17
.hidden _class_propinit_17__cdylib_exception_cleanup
.hidden _class_propinit_19
.hidden _class_propinit_19__cdylib_exception_cleanup
.hidden _class_propinit_20
.hidden _class_propinit_20__cdylib_exception_cleanup
.hidden _fn__u__u_elephc_u_internal_u_object_u_compare
.hidden _fn__u__u_elephc_u_internal_u_object_u_compare__cdylib_exception_cleanup
.hidden _enum_init_all_PropertyHookType_Get
.hidden _enum_init_SortDirection_Ascending
.hidden _enum_init_SortDirection_Descending
.hidden __rt_mbstring_startup
.hidden __rt_mbstring_startup_status
.hidden __rt_mbstring_startup_diagnostic
.hidden _str_0
.hidden _str_1
.hidden _str_2
.hidden _str_3
.hidden _str_4
.hidden _str_5
.hidden _str_6
.hidden _str_7
.hidden _str_8
.hidden _str_9
.hidden _str_10
.hidden _str_11
.hidden _str_12
.hidden _str_13
.hidden _str_14
.hidden _str_15
.hidden _str_16
.hidden _str_17
.hidden _str_18
.hidden _str_19
.hidden _str_20
.hidden _str_21
.hidden _str_22
.hidden _str_23
.hidden _str_24
.hidden _str_25
.hidden _str_26
.hidden _str_27
.hidden _str_28
.hidden _str_29
.hidden _str_30
.hidden _str_31
.hidden _str_32
.hidden _str_33
.hidden _str_34
.hidden _str_35
.hidden _str_36
.hidden _str_37
.hidden _str_38
.hidden _str_39
.hidden _str_40
.hidden _str_41
.hidden _str_42
.hidden _str_43
.hidden _str_44
.hidden _str_45
.hidden _str_46
.hidden _str_47
.hidden _str_48
.hidden _str_49
.hidden _str_50
.hidden _str_51
.hidden _str_52
.hidden _str_53
.hidden _str_54
.hidden _str_55
.hidden _str_56
.hidden _str_57
.hidden _str_58
.hidden _str_59
.hidden _str_60
.hidden _str_61
.hidden _str_62
.hidden _str_63
.hidden _str_64
.hidden _str_65
.hidden _str_66
.hidden _str_67
.hidden _str_68
.hidden _str_69
.hidden _str_70
.hidden _str_71
.hidden _str_72
.hidden _str_73
.hidden _str_74
.hidden _str_75
.hidden _str_76
.hidden _str_77
.hidden _str_78
.hidden _str_79
.hidden _str_80
.hidden _str_81
.hidden _str_82
.hidden _str_83
.hidden _str_84
.hidden _str_85
.hidden _str_86
.hidden _str_87
.hidden _str_88
.hidden _str_89
.hidden _str_90
.hidden _str_91
.hidden _str_92
.hidden _str_93
.hidden _str_94
.hidden _str_95
.hidden _str_96
.hidden _str_97
.hidden _str_98
.hidden _str_99
.hidden _str_100
.hidden _str_101
.hidden _str_102
.hidden _str_103
.hidden _str_104
.hidden _str_105
.hidden _str_106
.hidden _str_107
.hidden _str_108
.hidden _str_109
.hidden _str_110
.hidden _str_111
.hidden _str_112
.hidden _str_113
.hidden _str_114
.hidden _str_115
.hidden _str_116
.hidden _str_117
.hidden _str_118
.hidden _str_119
.hidden _str_120
.hidden _str_121
.hidden _str_122
.hidden _str_123
.hidden _str_124
.hidden _str_125
.hidden _str_126
.hidden _str_127
.hidden _str_128
.hidden _str_129
.hidden _str_130
.hidden _str_131
.hidden _str_132
.hidden _str_133
.hidden _str_134
.hidden _str_135
.hidden _str_136
.hidden _str_137
.hidden _str_138
.hidden _str_139
.hidden _str_140
.hidden _str_141
.hidden _str_142
.hidden _str_143
.hidden _str_144
.hidden _str_145
.hidden _str_146
.hidden _str_147
.hidden _str_148
.hidden _str_149
.hidden _str_150
.hidden _str_151
.hidden _str_152
.hidden _str_153
.hidden _str_154
.hidden _str_155
.hidden _str_156
.hidden _str_157
.hidden _str_158
.hidden _str_159
.hidden _str_160
.hidden _str_161
.hidden _str_162
.hidden _str_163
.hidden _str_164
.hidden _str_165
.hidden _str_166
.hidden _str_170
.hidden _str_171
.hidden _data_167
.hidden _data_168
.hidden _data_169
.hidden _enum_case_PropertyHookType_Get
.hidden _enum_case_PropertyHookType_Set
.hidden _enum_case_SortDirection_Ascending
.hidden _enum_case_SortDirection_Descending
.hidden _callable_user_fn_name_0
.hidden _callable_user_fn_name_1
.hidden _callable_user_function_count
.hidden _callable_user_function_table
.hidden _instanceof_target_count
.hidden _instanceof_target_entries
.hidden _instanceof_name_class_1
.hidden _instanceof_name_class_abs_1
.hidden _instanceof_name_class_2
.hidden _instanceof_name_class_abs_2
.hidden _instanceof_name_class_3
.hidden _instanceof_name_class_abs_3
.hidden _instanceof_name_class_4
.hidden _instanceof_name_class_abs_4
.hidden _instanceof_name_class_8
.hidden _instanceof_name_class_abs_8
.hidden _instanceof_name_class_9
.hidden _instanceof_name_class_abs_9
.hidden _instanceof_name_class_11
.hidden _instanceof_name_class_abs_11
.hidden _instanceof_name_class_17
.hidden _instanceof_name_class_abs_17
.hidden _instanceof_name_class_18
.hidden _instanceof_name_class_abs_18
.hidden _instanceof_name_interface_14
.hidden _instanceof_name_interface_abs_14
.hidden _instanceof_name_interface_15
.hidden _instanceof_name_interface_abs_15
.hidden _class_name_count
.hidden _class_name_entries
.hidden _class_name_missing
.hidden _class_name_1
.hidden _class_name_2
.hidden _class_name_3
.hidden _class_name_4
.hidden _class_name_8
.hidden _class_name_9
.hidden _class_name_11
.hidden _class_name_17
.hidden _class_name_18
.hidden _class_previous_slot_count
.hidden _class_previous_slots
.hidden _weak_map_key_retire_fn
.hidden _interface_name_0
.hidden _interface_name_1
.hidden _interface_name_2
.hidden _interface_name_3
.hidden _interface_name_4
.hidden _interface_name_5
.hidden _interface_name_6
.hidden _interface_name_7
.hidden _interface_name_8
.hidden _interface_name_9
.hidden _interface_name_10
.hidden _interface_name_11
.hidden _interface_name_12
.hidden _interface_name_13
.hidden _interface_name_14
.hidden _interface_name_15
.hidden _interface_names_count
.hidden _interface_names
.hidden _trait_names_count
.hidden _trait_names
.hidden _enum_name_0
.hidden _enum_name_1
.hidden _enum_names_count
.hidden _enum_names
.hidden _fiber_class_id
.hidden _fiber_error_class_id
.hidden _generator_class_id
.hidden _spl_dll_class_id
.hidden _spl_stack_class_id
.hidden _spl_queue_class_id
.hidden _spl_fixed_array_class_id
.hidden _spl_exception_class_id
.hidden _spl_error_class_id
.hidden _spl_logic_exception_class_id
.hidden _spl_runtime_exception_class_id
.hidden _spl_out_of_range_exception_class_id
.hidden _spl_out_of_bounds_exception_class_id
.hidden _spl_invalid_argument_exception_class_id
.hidden _spl_type_error_class_id
.hidden _error_exception_class_id
.hidden _weak_reference_class_id
.hidden _weak_map_class_id
.hidden _spl_value_error_class_id
.hidden _spl_arithmetic_error_class_id
.hidden _spl_division_by_zero_error_class_id
.hidden _spl_argument_count_error_class_id
.hidden _interface_count
.hidden _interface_method_ptrs
.hidden _class_interface_ptrs
.hidden _class_json_desc_ptrs
.hidden _class_vd_desc_ptrs
.hidden _class_prop_desc_ptrs
.hidden _class_enum_kinds
.hidden _class_enum_name_offsets
.hidden _class_enum_value_offsets
.hidden _json_exception_class_id
.hidden _class_parent_ids
.hidden _class_object_payload_sizes
.hidden _class_throwable_line_offsets
.hidden _class_object_allocation_sizes
.hidden _class_object_dynamic_prop_flags
.hidden _class_dump_dyn_prop_flags
.hidden _class_dump_dyn_first
.hidden _class_reflection_public_cast_flags
.hidden _class_gc_desc_count
.hidden _class_gc_desc_ptrs
.hidden _class_vtable_ptrs
.hidden _class_source_vtable_ptrs
.hidden _class_destruct_count
.hidden _class_destruct_ptrs
.hidden _class_tostring_count
.hidden _class_tostring_ptrs
.hidden _class_debuginfo_ptrs
.hidden _class_debuginfo_void
.hidden _debuginfo_any
.hidden _class_iface_method_count
.hidden _class_count_ptrs
.hidden _class_offsetget_ptrs
.hidden _class_offsetset_ptrs
.hidden _class_offsetunset_ptrs
.hidden _class_serialize_ptrs
.hidden _class_unserialize_ptrs
.hidden _class_sleep_ptrs
.hidden _class_wakeup_ptrs
.hidden _class_serializable_serialize_ptrs
.hidden _class_serializable_unserialize_ptrs
.hidden _class_serializable_serialize_return_abi
.hidden _class_serializable_unserialize_return_abi
.hidden _class_serializable_unserialize_boxed
.hidden _class_compare_ptrs
.hidden _class_to_number_ptrs
.hidden _unser_refused_class_count
.hidden _unser_refused_classes
.hidden _class_unserialize_data_boxed
.hidden _class_propinit_ptrs
.hidden _class_initcheck_ptrs
.hidden _class_serprop_ptrs
.hidden _class_serprop_declaring_ptrs
.hidden _class_serpdiag_ptrs
.hidden _class_static_vtable_ptrs
.hidden _class_source_static_vtable_ptrs
.hidden _class_callable_method_ptrs
.hidden _user_wrapper_vtable_ptrs
.hidden _user_filter_vtable_ptrs
.hidden _class_interfaces_missing
.hidden _class_gc_desc_missing
.hidden _class_serprop_missing
.hidden _class_serprop_declaring_missing
.hidden _class_serpdiag_missing
.hidden _class_json_desc_missing
.hidden _class_vd_desc_missing
.hidden _class_prop_desc_missing
.hidden _class_vtable_missing
.hidden _class_static_vtable_missing
.hidden _class_callable_methods_missing
.hidden _user_wrapper_vtable_missing
.hidden _user_filter_vtable_missing
.hidden _class_callable_static_method_count
.hidden _class_callable_static_method_table
.hidden _script_source_file
.hidden _script_source_file_len
.hidden _class_by_name_str_1
.hidden _class_by_name_str_2
.hidden _class_by_name_str_3
.hidden _class_by_name_str_4
.hidden _class_by_name_str_8
.hidden _class_by_name_str_9
.hidden _class_by_name_str_11
.hidden _class_by_name_str_17
.hidden _class_by_name_str_18
.hidden _classes_by_name_count
.hidden _classes_by_name
.hidden _class_attribute_count
.hidden _class_attribute_ptrs
.hidden _class_attributes_missing
.hidden _interface_methods_14
.hidden _interface_methods_15
.hidden _class_interfaces_1
.hidden _class_interface_impl_1_15
.hidden _class_interface_impl_1_14
.hidden _class_json_pname_1_0
.hidden _class_json_desc_1
.hidden _class_gc_desc_1
.hidden _class_serpname_1_0
.hidden _class_serpname_1_1
.hidden _class_serpname_1_2
.hidden _class_serprop_1
.hidden _class_serprop_declaring_1
.hidden _class_serpsuffix_1_0
.hidden _class_serpsuffix_1_1
.hidden _class_serpsuffix_1_2
.hidden _class_serpdiag_1
.hidden _class_vd_pkey_1_0
.hidden _class_vd_ptype_1_0
.hidden _class_vd_pkey_1_1
.hidden _class_vd_ptype_1_1
.hidden _class_vd_pkey_1_2
.hidden _class_vd_ptype_1_2
.hidden _class_vd_desc_1
.hidden _class_prop_pkey_1_0
.hidden _class_prop_pkey_1_1
.hidden _class_prop_nkey_1_1
.hidden _class_prop_pkey_1_2
.hidden _class_prop_nkey_1_2
.hidden _class_prop_desc_1
.hidden _class_vtable_1
.hidden _class_source_vtable_1
.hidden _class_static_vtable_1
.hidden _class_source_static_vtable_1
.hidden _class_callable_method_name_1__u__u_construct
.hidden _class_callable_method_name_1__u__u_tostring
.hidden _class_callable_method_name_1_getcode
.hidden _class_callable_method_name_1_getfile
.hidden _class_callable_method_name_1_getline
.hidden _class_callable_method_name_1_getmessage
.hidden _class_callable_method_name_1_getprevious
.hidden _class_callable_method_name_1_gettrace
.hidden _class_callable_method_name_1_gettraceasstring
.hidden _class_callable_methods_1
.hidden _class_interfaces_2
.hidden _class_interface_impl_2_15
.hidden _class_interface_impl_2_14
.hidden _class_json_pname_2_0
.hidden _class_json_desc_2
.hidden _class_gc_desc_2
.hidden _class_serpname_2_0
.hidden _class_serpname_2_1
.hidden _class_serpname_2_2
.hidden _class_serprop_2
.hidden _class_serprop_declaring_2
.hidden _class_serpsuffix_2_0
.hidden _class_serpsuffix_2_1
.hidden _class_serpsuffix_2_2
.hidden _class_serpdiag_2
.hidden _class_vd_desc_2
.hidden _class_prop_desc_2
.hidden _class_vtable_2
.hidden _class_source_vtable_2
.hidden _class_static_vtable_2
.hidden _class_source_static_vtable_2
.hidden _class_callable_methods_2
.hidden _class_interfaces_3
.hidden _class_interface_impl_3_15
.hidden _class_interface_impl_3_14
.hidden _class_json_pname_3_0
.hidden _class_json_desc_3
.hidden _class_gc_desc_3
.hidden _class_serpname_3_0
.hidden _class_serpname_3_1
.hidden _class_serpname_3_2
.hidden _class_serprop_3
.hidden _class_serprop_declaring_3
.hidden _class_serpsuffix_3_0
.hidden _class_serpsuffix_3_1
.hidden _class_serpsuffix_3_2
.hidden _class_serpdiag_3
.hidden _class_vd_desc_3
.hidden _class_prop_desc_3
.hidden _class_vtable_3
.hidden _class_source_vtable_3
.hidden _class_static_vtable_3
.hidden _class_source_static_vtable_3
.hidden _class_callable_methods_3
.hidden _class_interfaces_4
.hidden _class_interface_impl_4_15
.hidden _class_interface_impl_4_14
.hidden _class_json_pname_4_0
.hidden _class_json_desc_4
.hidden _class_gc_desc_4
.hidden _class_serpname_4_0
.hidden _class_serpname_4_1
.hidden _class_serpname_4_2
.hidden _class_serprop_4
.hidden _class_serprop_declaring_4
.hidden _class_serpsuffix_4_0
.hidden _class_serpsuffix_4_1
.hidden _class_serpsuffix_4_2
.hidden _class_serpdiag_4
.hidden _class_vd_desc_4
.hidden _class_prop_desc_4
.hidden _class_vtable_4
.hidden _class_source_vtable_4
.hidden _class_static_vtable_4
.hidden _class_source_static_vtable_4
.hidden _class_callable_methods_4
.hidden _class_interfaces_8
.hidden _class_interface_impl_8_15
.hidden _class_interface_impl_8_14
.hidden _class_json_pname_8_0
.hidden _class_json_desc_8
.hidden _class_gc_desc_8
.hidden _class_serpname_8_0
.hidden _class_serpname_8_1
.hidden _class_serpname_8_2
.hidden _class_serprop_8
.hidden _class_serprop_declaring_8
.hidden _class_serpsuffix_8_0
.hidden _class_serpsuffix_8_1
.hidden _class_serpsuffix_8_2
.hidden _class_serpdiag_8
.hidden _class_vd_desc_8
.hidden _class_prop_desc_8
.hidden _class_vtable_8
.hidden _class_source_vtable_8
.hidden _class_static_vtable_8
.hidden _class_source_static_vtable_8
.hidden _class_callable_methods_8
.hidden _class_interfaces_9
.hidden _class_interface_impl_9_15
.hidden _class_interface_impl_9_14
.hidden _class_json_pname_9_0
.hidden _class_json_desc_9
.hidden _class_gc_desc_9
.hidden _class_serpname_9_0
.hidden _class_serpname_9_1
.hidden _class_serpname_9_2
.hidden _class_serprop_9
.hidden _class_serprop_declaring_9
.hidden _class_serpsuffix_9_0
.hidden _class_serpsuffix_9_1
.hidden _class_serpsuffix_9_2
.hidden _class_serpdiag_9
.hidden _class_vd_desc_9
.hidden _class_prop_desc_9
.hidden _class_vtable_9
.hidden _class_source_vtable_9
.hidden _class_static_vtable_9
.hidden _class_source_static_vtable_9
.hidden _class_callable_methods_9
.hidden _class_interfaces_11
.hidden _class_interface_impl_11_15
.hidden _class_interface_impl_11_14
.hidden _class_json_pname_11_0
.hidden _class_json_desc_11
.hidden _class_gc_desc_11
.hidden _class_serpname_11_0
.hidden _class_serpname_11_1
.hidden _class_serpname_11_2
.hidden _class_serprop_11
.hidden _class_serprop_declaring_11
.hidden _class_serpsuffix_11_0
.hidden _class_serpsuffix_11_1
.hidden _class_serpsuffix_11_2
.hidden _class_serpdiag_11
.hidden _class_vd_desc_11
.hidden _class_prop_desc_11
.hidden _class_vtable_11
.hidden _class_source_vtable_11
.hidden _class_static_vtable_11
.hidden _class_source_static_vtable_11
.hidden _class_callable_methods_11
.hidden _class_interfaces_17
.hidden _class_interface_impl_17_15
.hidden _class_interface_impl_17_14
.hidden _class_json_pname_17_0
.hidden _class_json_desc_17
.hidden _class_gc_desc_17
.hidden _class_serpname_17_0
.hidden _class_serpname_17_1
.hidden _class_serpname_17_2
.hidden _class_serprop_17
.hidden _class_serprop_declaring_17
.hidden _class_serpsuffix_17_0
.hidden _class_serpsuffix_17_1
.hidden _class_serpsuffix_17_2
.hidden _class_serpdiag_17
.hidden _class_vd_desc_17
.hidden _class_prop_desc_17
.hidden _class_vtable_17
.hidden _class_source_vtable_17
.hidden _class_static_vtable_17
.hidden _class_source_static_vtable_17
.hidden _class_callable_methods_17
.hidden _class_interfaces_18
.hidden _class_json_desc_18
.hidden _class_gc_desc_18
.hidden _class_serprop_18
.hidden _class_serprop_declaring_18
.hidden _class_serpdiag_18
.hidden _class_vd_desc_18
.hidden _class_prop_desc_18
.hidden _class_vtable_18
.hidden _class_source_vtable_18
.hidden _class_static_vtable_18
.hidden _class_source_static_vtable_18
.hidden _class_callable_methods_18
.hidden _stdclass_class_id
