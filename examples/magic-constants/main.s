.align 2
.globl _fn_App_N_Demo_N_greet
_fn_App_N_Demo_N_greet:
    ; prologue
    sub sp, sp, #80
    stp x29, x30, [sp, #64]
    add x29, sp, #64
    ; register exception cleanup frame
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    ldr x10, [x9]
    stur x10, [x29, #-24]
    adrp x10, _cleanup_frame_0@PAGE
    add x10, x10, _cleanup_frame_0@PAGEOFF
    stur x10, [x29, #-16]
    mov x10, x29
    stur x10, [x29, #-8]
    stur xzr, [x29, #-32]
    sub x10, x29, #24
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ; @src line=18 col=5
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "  in greet():  __FUNCTION__ = App\\Demo\\greet\n"
    adrp x1, _str_0@PAGE
    add x1, x1, _str_0@PAGEOFF
    mov x2, #45
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=19 col=5
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "  in greet():  __METHOD__   = App\\Demo\\greet\n"
    adrp x1, _str_1@PAGE
    add x1, x1, _str_1@PAGEOFF
    mov x2, #45
    mov x0, #1
    mov x16, #4
    svc #0x80
_fn_App_N_Demo_N_greet_epilogue:
    ; unregister exception cleanup frame
    ldur x10, [x29, #-24]
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ldp x29, x30, [sp, #64]
    add sp, sp, #80
    ret

_cleanup_frame_0:
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, x0
    ldp x29, x30, [sp, #0]
    add sp, sp, #16
    ret

.align 2
.globl _method_App_N_Demo_N_Greeter_hello
_method_App_N_Demo_N_Greeter_hello:
    ; prologue
    sub sp, sp, #96
    stp x29, x30, [sp, #80]
    add x29, sp, #80
    ; param $this from x0
    stur x0, [x29, #-8]
    ; register exception cleanup frame
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    ldr x10, [x9]
    stur x10, [x29, #-32]
    adrp x10, _cleanup_frame_1@PAGE
    add x10, x10, _cleanup_frame_1@PAGEOFF
    stur x10, [x29, #-24]
    mov x10, x29
    stur x10, [x29, #-16]
    stur xzr, [x29, #-40]
    sub x10, x29, #32
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ; @src line=26 col=9
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "  in Greeter::hello():\n"
    adrp x1, _str_2@PAGE
    add x1, x1, _str_2@PAGEOFF
    mov x2, #23
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=27 col=9
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "    __CLASS__    = App\\Demo\\Greeter\n"
    adrp x1, _str_3@PAGE
    add x1, x1, _str_3@PAGEOFF
    mov x2, #36
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=28 col=9
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "    __METHOD__   = App\\Demo\\Greeter::hello\n"
    adrp x1, _str_4@PAGE
    add x1, x1, _str_4@PAGEOFF
    mov x2, #43
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=29 col=9
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "    __FUNCTION__ = hello\n"
    adrp x1, _str_5@PAGE
    add x1, x1, _str_5@PAGEOFF
    mov x2, #25
    mov x0, #1
    mov x16, #4
    svc #0x80
_method_App_N_Demo_N_Greeter_hello_epilogue:
    ; unregister exception cleanup frame
    ldur x10, [x29, #-32]
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ldp x29, x30, [sp, #80]
    add sp, sp, #96
    ret

_cleanup_frame_1:
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, x0
    ldp x29, x30, [sp, #0]
    add sp, sp, #16
    ret

.align 2
.globl _method_App_N_Demo_N_Service_report
_method_App_N_Demo_N_Service_report:
    ; prologue
    sub sp, sp, #96
    stp x29, x30, [sp, #80]
    add x29, sp, #80
    ; param $this from x0
    stur x0, [x29, #-8]
    ; register exception cleanup frame
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    ldr x10, [x9]
    stur x10, [x29, #-32]
    adrp x10, _cleanup_frame_2@PAGE
    add x10, x10, _cleanup_frame_2@PAGEOFF
    stur x10, [x29, #-24]
    mov x10, x29
    stur x10, [x29, #-16]
    stur xzr, [x29, #-40]
    sub x10, x29, #32
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ; @src line=38 col=9
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "  in Reportable::report():\n"
    adrp x1, _str_6@PAGE
    add x1, x1, _str_6@PAGEOFF
    mov x2, #27
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=39 col=9
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "    __TRAIT__    = App\\Demo\\Reportable\n"
    adrp x1, _str_7@PAGE
    add x1, x1, _str_7@PAGEOFF
    mov x2, #39
    mov x0, #1
    mov x16, #4
    svc #0x80
_method_App_N_Demo_N_Service_report_epilogue:
    ; unregister exception cleanup frame
    ldur x10, [x29, #-32]
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ldp x29, x30, [sp, #80]
    add sp, sp, #96
    ret

_cleanup_frame_2:
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, x0
    ldp x29, x30, [sp, #0]
    add sp, sp, #16
    ret

.align 2
.globl _method_Exception__u__u_construct
_method_Exception__u__u_construct:
    ; prologue
    sub sp, sp, #112
    stp x29, x30, [sp, #96]
    add x29, sp, #96
    ; param $this from x0
    stur x0, [x29, #-8]
    ; param $message from x1,x2
    stur x1, [x29, #-24]
    stur x2, [x29, #-16]
    ; register exception cleanup frame
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    ldr x10, [x9]
    stur x10, [x29, #-48]
    adrp x10, _cleanup_frame_3@PAGE
    add x10, x10, _cleanup_frame_3@PAGEOFF
    stur x10, [x29, #-40]
    mov x10, x29
    stur x10, [x29, #-32]
    stur xzr, [x29, #-56]
    sub x10, x29, #48
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; ->message  = ...
    ; load $message
    ldur x1, [x29, #-24]
    ldur x2, [x29, #-16]
    stp x1, x2, [sp, #-16]!
    ; $this
    ldur x0, [x29, #-8]
    mov x9, x0
    str x9, [sp, #-16]!
    ldr x0, [x9, #8]
    bl __rt_heap_free_safe
    ldr x9, [sp], #16
    ldp x1, x2, [sp], #16
    str x9, [sp, #-16]!
    bl __rt_str_persist
    ldr x9, [sp], #16
    str x1, [x9, #8]
    str x2, [x9, #16]
_method_Exception__u__u_construct_epilogue:
    ; unregister exception cleanup frame
    ldur x10, [x29, #-48]
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ldp x29, x30, [sp, #96]
    add sp, sp, #112
    ret

_cleanup_frame_3:
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, x0
    ldp x29, x30, [sp, #0]
    add sp, sp, #16
    ret

.align 2
.globl _method_Exception_getMessage
_method_Exception_getMessage:
    ; prologue
    sub sp, sp, #96
    stp x29, x30, [sp, #80]
    add x29, sp, #80
    ; param $this from x0
    stur x0, [x29, #-8]
    ; register exception cleanup frame
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    ldr x10, [x9]
    stur x10, [x29, #-32]
    adrp x10, _cleanup_frame_4@PAGE
    add x10, x10, _cleanup_frame_4@PAGEOFF
    stur x10, [x29, #-24]
    mov x10, x29
    stur x10, [x29, #-16]
    stur xzr, [x29, #-40]
    sub x10, x29, #32
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; return
    ; $this
    ldur x0, [x29, #-8]
    ; ->message  (offset 8)
    mov x9, x0
    ldr x1, [x9, #8]
    ldr x2, [x9, #16]
    bl __rt_str_persist
    b _method_Exception_getMessage_epilogue
_method_Exception_getMessage_epilogue:
    ; unregister exception cleanup frame
    ldur x10, [x29, #-32]
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ldp x29, x30, [sp, #80]
    add sp, sp, #96
    ret

_cleanup_frame_4:
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, x0
    ldp x29, x30, [sp, #0]
    add sp, sp, #16
    ret

.align 2

.globl _main
_main:
    ; prologue
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    add x29, sp, #112
    ; save argc/argv to globals
    adrp x9, _global_argc@PAGE
    add x9, x9, _global_argc@PAGEOFF
    str x0, [x9]
    adrp x9, _global_argv@PAGE
    add x9, x9, _global_argv@PAGEOFF
    str x1, [x9]
    stur xzr, [x29, #-40]
    stur xzr, [x29, #-32]
    stur xzr, [x29, #-16]
    ; register main exception cleanup frame
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    ldr x10, [x9]
    stur x10, [x29, #-64]
    adrp x10, _main_cleanup_frame_5@PAGE
    add x10, x10, _main_cleanup_frame_5@PAGEOFF
    stur x10, [x29, #-56]
    mov x10, x29
    stur x10, [x29, #-48]
    stur xzr, [x29, #-72]
    sub x10, x29, #64
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ; @src line=4 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "  helper.php loaded \u{2014} its own __FILE__ = /Users/guillaumeloulier/PhpstormProjects/oss/elephc/examples/magic-constants/lib/helper.php\n"
    adrp x1, _str_8@PAGE
    add x1, x1, _str_8@PAGEOFF
    mov x2, #135
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=11 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "__FILE__ = /Users/guillaumeloulier/PhpstormProjects/oss/elephc/examples/magic-constants/main.php\n"
    adrp x1, _str_9@PAGE
    add x1, x1, _str_9@PAGEOFF
    mov x2, #97
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=12 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "__DIR__  = /Users/guillaumeloulier/PhpstormProjects/oss/elephc/examples/magic-constants\n"
    adrp x1, _str_10@PAGE
    add x1, x1, _str_10@PAGEOFF
    mov x2, #88
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=13 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "__LINE__ = "
    adrp x1, _str_11@PAGE
    add x1, x1, _str_11@PAGEOFF
    mov x2, #11
    stp x1, x2, [sp, #-16]!
    ; load int 13
    mov x0, #13
    bl __rt_itoa
    mov x3, x1
    mov x4, x2
    ldp x1, x2, [sp], #16
    bl __rt_concat
    bl __rt_str_persist
    stp x1, x2, [sp, #-16]!
    ; load string "\n"
    adrp x1, _str_12@PAGE
    add x1, x1, _str_12@PAGEOFF
    mov x2, #1
    mov x3, x1
    mov x4, x2
    ldp x1, x2, [sp], #16
    bl __rt_concat
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=14 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "__NAMESPACE__ = App\\Demo\n"
    adrp x1, _str_13@PAGE
    add x1, x1, _str_13@PAGEOFF
    mov x2, #25
    mov x0, #1
    mov x16, #4
    svc #0x80
    ; @src line=21 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; call App\Demo\greet()
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    ldr x10, [x9]
    str x10, [sp, #-16]!
    bl _fn_App_N_Demo_N_greet
    ldr x10, [sp], #16
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str x10, [x9]
    ; @src line=32 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; $g = ...
    ; new App\Demo\Greeter()
    mov x0, #8
    bl __rt_heap_alloc
    mov x9, #4
    str x9, [x0, #-8]
    mov x10, #0
    str x10, [x0]
    str x0, [sp, #-16]!
    ldr x0, [sp], #16
    str x0, [sp, #-16]!
    ldur x0, [x29, #-32]
    bl __rt_decref_object
    ldr x0, [sp], #16
    stur x0, [x29, #-32]
    ; @src line=33 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; ->hello()
    ; load $g
    ldur x0, [x29, #-32]
    str x0, [sp, #-16]!
    ldr x0, [sp], #16
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    ldr x10, [x9]
    str x10, [sp, #-16]!
    ldr x10, [x0]
    adrp x9, _class_vtable_ptrs@PAGE
    add x9, x9, _class_vtable_ptrs@PAGEOFF
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9]
    blr x9
    ldr x10, [sp], #16
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str x10, [x9]
    ; @src line=45 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; $s = ...
    ; new App\Demo\Service()
    mov x0, #8
    bl __rt_heap_alloc
    mov x9, #4
    str x9, [x0, #-8]
    mov x10, #1
    str x10, [x0]
    str x0, [sp, #-16]!
    ldr x0, [sp], #16
    str x0, [sp, #-16]!
    ldur x0, [x29, #-16]
    bl __rt_decref_object
    ldr x0, [sp], #16
    stur x0, [x29, #-16]
    ; @src line=46 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; ->report()
    ; load $s
    ldur x0, [x29, #-16]
    str x0, [sp, #-16]!
    ldr x0, [sp], #16
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    ldr x10, [x9]
    str x10, [sp, #-16]!
    ldr x10, [x0]
    adrp x9, _class_vtable_ptrs@PAGE
    add x9, x9, _class_vtable_ptrs@PAGEOFF
    ldr x9, [x9, x10, lsl #3]
    ldr x9, [x9]
    blr x9
    ldr x10, [sp], #16
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str x10, [x9]
    ; @src line=49 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; $f = ...
    ; closure: load function address
    adrp x0, _closure_6@PAGE
    add x0, x0, _closure_6@PAGEOFF
    str x0, [sp, #-16]!
    ldr x0, [sp], #16
    stur x0, [x29, #-8]
    ; @src line=52 col=1
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; call $f()
    ldur x19, [x29, #-8]
    str x19, [sp, #-16]!
    ldr x19, [sp], #16
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    ldr x10, [x9]
    str x10, [sp, #-16]!
    blr x19
    ldr x10, [sp], #16
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str x10, [x9]

    ; epilogue + exit(0)
    ; epilogue cleanup $s
    ldur x0, [x29, #-16]
    bl __rt_decref_object
    ; epilogue cleanup $g
    ldur x0, [x29, #-32]
    bl __rt_decref_object
    ; unregister main exception cleanup frame
    ldur x10, [x29, #-64]
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ldp x29, x30, [sp, #112]
    add sp, sp, #128
    mov x0, #0
    mov x16, #1
    svc #0x80
.align 2
.globl _closure_6
_closure_6:
    ; prologue
    sub sp, sp, #80
    stp x29, x30, [sp, #64]
    add x29, sp, #64
    ; register exception cleanup frame
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    ldr x10, [x9]
    stur x10, [x29, #-24]
    adrp x10, _cleanup_frame_7@PAGE
    add x10, x10, _cleanup_frame_7@PAGEOFF
    stur x10, [x29, #-16]
    mov x10, x29
    stur x10, [x29, #-8]
    stur xzr, [x29, #-32]
    sub x10, x29, #24
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ; @src line=50 col=5
    adrp x9, _concat_off@PAGE
    add x9, x9, _concat_off@PAGEOFF
    str xzr, [x9]

    ; echo
    ; load string "  inside closure: __FUNCTION__ = {closure}\n"
    adrp x1, _str_14@PAGE
    add x1, x1, _str_14@PAGEOFF
    mov x2, #43
    mov x0, #1
    mov x16, #4
    svc #0x80
_closure_6_epilogue:
    ; unregister exception cleanup frame
    ldur x10, [x29, #-24]
    adrp x9, _exc_call_frame_top@PAGE
    add x9, x9, _exc_call_frame_top@PAGEOFF
    str x10, [x9]
    ldp x29, x30, [sp, #64]
    add sp, sp, #80
    ret

_cleanup_frame_7:
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, x0
    ldp x29, x30, [sp, #0]
    add sp, sp, #16
    ret

_main_cleanup_frame_5:
    sub sp, sp, #16
    stp x29, x30, [sp, #0]
    mov x29, x0
    ; epilogue cleanup $s
    ldur x0, [x29, #-16]
    bl __rt_decref_object
    ; epilogue cleanup $g
    ldur x0, [x29, #-32]
    bl __rt_decref_object
    ldp x29, x30, [sp, #0]
    add sp, sp, #16
    ret


.data
.globl _str_0
_str_0:
    .ascii "  in greet():  __FUNCTION__ = App\\Demo\\greet\n"
.globl _str_1
_str_1:
    .ascii "  in greet():  __METHOD__   = App\\Demo\\greet\n"
.globl _str_2
_str_2:
    .ascii "  in Greeter::hello():\n"
.globl _str_3
_str_3:
    .ascii "    __CLASS__    = App\\Demo\\Greeter\n"
.globl _str_4
_str_4:
    .ascii "    __METHOD__   = App\\Demo\\Greeter::hello\n"
.globl _str_5
_str_5:
    .ascii "    __FUNCTION__ = hello\n"
.globl _str_6
_str_6:
    .ascii "  in Reportable::report():\n"
.globl _str_7
_str_7:
    .ascii "    __TRAIT__    = App\\Demo\\Reportable\n"
.globl _str_8
_str_8:
    .ascii "  helper.php loaded \xe2\x80\x94 its own __FILE__ = /Users/guillaumeloulier/PhpstormProjects/oss/elephc/examples/magic-constants/lib/helper.php\n"
.globl _str_9
_str_9:
    .ascii "__FILE__ = /Users/guillaumeloulier/PhpstormProjects/oss/elephc/examples/magic-constants/main.php\n"
.globl _str_10
_str_10:
    .ascii "__DIR__  = /Users/guillaumeloulier/PhpstormProjects/oss/elephc/examples/magic-constants\n"
.globl _str_11
_str_11:
    .ascii "__LINE__ = "
.globl _str_12
_str_12:
    .ascii "\n"
.globl _str_13
_str_13:
    .ascii "__NAMESPACE__ = App\\Demo\n"
.globl _str_14
_str_14:
    .ascii "  inside closure: __FUNCTION__ = {closure}\n"

.data
.p2align 3
.globl _interface_count
_interface_count:
    .quad 1
.globl _interface_method_ptrs
_interface_method_ptrs:
    .quad _interface_methods_0
.globl _class_interface_ptrs
_class_interface_ptrs:
    .quad _class_interfaces_0
    .quad _class_interfaces_1
    .quad _class_interfaces_2
.globl _class_parent_ids
_class_parent_ids:
    .quad -1
    .quad -1
    .quad -1
.globl _class_gc_desc_count
_class_gc_desc_count:
    .quad 3
.globl _class_gc_desc_ptrs
_class_gc_desc_ptrs:
    .quad _class_gc_desc_0
    .quad _class_gc_desc_1
    .quad _class_gc_desc_2
.globl _class_vtable_ptrs
_class_vtable_ptrs:
    .quad _class_vtable_0
    .quad _class_vtable_1
    .quad _class_vtable_2
.globl _class_static_vtable_ptrs
_class_static_vtable_ptrs:
    .quad _class_static_vtable_0
    .quad _class_static_vtable_1
    .quad _class_static_vtable_2
.globl _class_interfaces_missing
_class_interfaces_missing:
    .quad 0
.globl _class_gc_desc_missing
_class_gc_desc_missing:
    .byte 0
    .p2align 3
.globl _class_vtable_missing
_class_vtable_missing:
    .quad 0
    .p2align 3
.globl _class_static_vtable_missing
_class_static_vtable_missing:
    .quad 0
.globl _interface_methods_0
_interface_methods_0:
    .quad 1
    .quad 0
.globl _class_interfaces_0
_class_interfaces_0:
    .quad 0
.globl _class_gc_desc_0
_class_gc_desc_0:
    .byte 0
    .p2align 3
.globl _class_vtable_0
_class_vtable_0:
    .quad _method_App_N_Demo_N_Greeter_hello
    .p2align 3
.globl _class_static_vtable_0
_class_static_vtable_0:
    .quad 0
.globl _class_interfaces_1
_class_interfaces_1:
    .quad 0
.globl _class_gc_desc_1
_class_gc_desc_1:
    .byte 0
    .p2align 3
.globl _class_vtable_1
_class_vtable_1:
    .quad _method_App_N_Demo_N_Service_report
    .p2align 3
.globl _class_static_vtable_1
_class_static_vtable_1:
    .quad 0
.globl _class_interfaces_2
_class_interfaces_2:
    .quad 1
    .quad 0
    .quad _class_interface_impl_2_0
.globl _class_interface_impl_2_0
_class_interface_impl_2_0:
    .quad _method_Exception_getMessage
.globl _class_gc_desc_2
_class_gc_desc_2:
    .byte 1
    .p2align 3
.globl _class_vtable_2
_class_vtable_2:
    .quad _method_Exception__u__u_construct
    .quad _method_Exception_getMessage
    .p2align 3
.globl _class_static_vtable_2
_class_static_vtable_2:
    .quad 0
