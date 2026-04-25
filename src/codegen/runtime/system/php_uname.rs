use crate::codegen::{
    abi,
    emit::Emitter,
    platform::{Arch, Platform},
};

const MODE_LEN_MSG: &str =
    "Fatal error: php_uname(): Argument #1 ($mode) must be a single character\n";
const MODE_VALUE_MSG: &str =
    "Fatal error: php_uname(): Argument #1 ($mode) must be one of \"a\", \"m\", \"n\", \"r\", \"s\", or \"v\"\n";

pub(crate) fn emit_php_uname_data() -> String {
    format!(
        ".globl _php_uname_mode_len_msg\n_php_uname_mode_len_msg:\n    .ascii {:?}\n\
         .globl _php_uname_mode_value_msg\n_php_uname_mode_value_msg:\n    .ascii {:?}\n",
        MODE_LEN_MSG, MODE_VALUE_MSG
    )
}

pub(crate) fn emit_php_uname(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_php_uname_aarch64(emitter),
        Arch::X86_64 => emit_php_uname_linux_x86_64(emitter),
    }
}

fn uts_field_len(platform: Platform) -> usize {
    match platform {
        Platform::MacOS => 256,
        Platform::Linux => 65,
    }
}

fn uts_offset(platform: Platform, field_index: usize) -> usize {
    uts_field_len(platform) * field_index
}

fn emit_php_uname_aarch64(emitter: &mut Emitter) {
    let uts_base = 32usize;
    let sysname = uts_base + uts_offset(emitter.target.platform, 0);
    let nodename = uts_base + uts_offset(emitter.target.platform, 1);
    let release = uts_base + uts_offset(emitter.target.platform, 2);
    let version = uts_base + uts_offset(emitter.target.platform, 3);
    let machine = uts_base + uts_offset(emitter.target.platform, 4);

    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: php_uname ---");
    emitter.label_global("__rt_php_uname");

    // -- validate mode string and preserve the selected mode across libc uname() --
    emitter.instruction("sub sp, sp, #1536");                                   // reserve stack storage for frame metadata and the largest supported utsname layout
    emitter.instruction("stp x29, x30, [sp, #0]");                              // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #0");                                     // establish a stable frame pointer for this runtime helper
    emitter.instruction("cmp x2, #1");                                          // PHP requires php_uname($mode) to receive exactly one mode byte
    emitter.instruction("b.ne __rt_php_uname_mode_len_fail");                   // reject empty or multi-byte mode strings before reading the mode byte
    emitter.instruction("ldrb w8, [x1]");                                       // load the requested php_uname mode byte
    emit_aarch64_validate_mode(emitter);
    emitter.instruction("str x8, [sp, #16]");                                   // preserve the accepted mode byte across the libc uname() call

    // -- ask libc for the target runtime's uname fields --
    emitter.instruction(&format!("add x0, sp, #{}", uts_base));                 // pass stack storage for struct utsname to libc uname()
    emitter.bl_c("uname");                                                      // fill the stack-backed utsname buffer through libc
    emitter.instruction("ldr x8, [sp, #16]");                                   // reload the accepted mode byte after libc returns

    // -- set up concat-backed transient string storage for the result --
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x7, [x6]");                                        // load the current concat-buffer write offset
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("add x9, x9, x7");                                      // compute the destination cursor for the returned uname string
    emitter.instruction("mov x1, x9");                                          // remember the result start pointer for the string return value
    emitter.instruction("mov x2, #0");                                          // initialize the returned uname string length to zero bytes

    // -- dispatch by PHP mode --
    emitter.instruction(&format!("cmp w8, #{}", b'a'));                         // does the caller want the complete uname string?
    emitter.instruction("b.eq __rt_php_uname_copy_all");                        // mode "a" returns sysname, nodename, release, version, and machine
    emitter.instruction(&format!("cmp w8, #{}", b's'));                         // does the caller want sysname?
    emitter.instruction("b.eq __rt_php_uname_select_sysname");                  // mode "s" returns the operating-system name
    emitter.instruction(&format!("cmp w8, #{}", b'n'));                         // does the caller want nodename?
    emitter.instruction("b.eq __rt_php_uname_select_nodename");                 // mode "n" returns the network node name
    emitter.instruction(&format!("cmp w8, #{}", b'r'));                         // does the caller want release?
    emitter.instruction("b.eq __rt_php_uname_select_release");                  // mode "r" returns the operating-system release
    emitter.instruction(&format!("cmp w8, #{}", b'v'));                         // does the caller want version?
    emitter.instruction("b.eq __rt_php_uname_select_version");                  // mode "v" returns the operating-system version
    emitter.instruction(&format!("cmp w8, #{}", b'm'));                         // does the caller want machine?
    emitter.instruction("b.eq __rt_php_uname_select_machine");                  // mode "m" returns the machine hardware name
    emitter.instruction("b __rt_php_uname_mode_value_fail");                    // defensive fallback for any mode that escaped validation

    emitter.label("__rt_php_uname_select_sysname");
    emitter.instruction(&format!("add x10, sp, #{}", sysname));                 // select utsname.sysname as the C-string source
    emitter.instruction("b __rt_php_uname_copy_selected");                      // copy the selected field into concat-backed string storage
    emitter.label("__rt_php_uname_select_nodename");
    emitter.instruction(&format!("add x10, sp, #{}", nodename));                // select utsname.nodename as the C-string source
    emitter.instruction("b __rt_php_uname_copy_selected");                      // copy the selected field into concat-backed string storage
    emitter.label("__rt_php_uname_select_release");
    emitter.instruction(&format!("add x10, sp, #{}", release));                 // select utsname.release as the C-string source
    emitter.instruction("b __rt_php_uname_copy_selected");                      // copy the selected field into concat-backed string storage
    emitter.label("__rt_php_uname_select_version");
    emitter.instruction(&format!("add x10, sp, #{}", version));                 // select utsname.version as the C-string source
    emitter.instruction("b __rt_php_uname_copy_selected");                      // copy the selected field into concat-backed string storage
    emitter.label("__rt_php_uname_select_machine");
    emitter.instruction(&format!("add x10, sp, #{}", machine));                 // select utsname.machine as the C-string source
    emitter.instruction("b __rt_php_uname_copy_selected");                      // copy the selected field into concat-backed string storage

    emit_aarch64_copy_selected(emitter);

    emitter.label("__rt_php_uname_copy_all");
    emit_aarch64_copy_field(emitter, "sysname_all", sysname, true);
    emit_aarch64_copy_field(emitter, "nodename_all", nodename, true);
    emit_aarch64_copy_field(emitter, "release_all", release, true);
    emit_aarch64_copy_field(emitter, "version_all", version, true);
    emit_aarch64_copy_field(emitter, "machine_all", machine, false);
    emitter.instruction("b __rt_php_uname_done");                               // finish after assembling the complete uname string

    emit_aarch64_done(emitter);
    emit_aarch64_fail(emitter, "__rt_php_uname_mode_len_fail", "_php_uname_mode_len_msg", MODE_LEN_MSG.len());
    emit_aarch64_fail(
        emitter,
        "__rt_php_uname_mode_value_fail",
        "_php_uname_mode_value_msg",
        MODE_VALUE_MSG.len(),
    );
}

fn emit_aarch64_validate_mode(emitter: &mut Emitter) {
    emitter.instruction(&format!("cmp w8, #{}", b'a'));                         // accept PHP mode "a" for the complete uname string
    emitter.instruction("b.eq __rt_php_uname_mode_ok");                         // finish validation when mode "a" was requested
    emitter.instruction(&format!("cmp w8, #{}", b'm'));                         // accept PHP mode "m" for machine
    emitter.instruction("b.eq __rt_php_uname_mode_ok");                         // finish validation when mode "m" was requested
    emitter.instruction(&format!("cmp w8, #{}", b'n'));                         // accept PHP mode "n" for nodename
    emitter.instruction("b.eq __rt_php_uname_mode_ok");                         // finish validation when mode "n" was requested
    emitter.instruction(&format!("cmp w8, #{}", b'r'));                         // accept PHP mode "r" for release
    emitter.instruction("b.eq __rt_php_uname_mode_ok");                         // finish validation when mode "r" was requested
    emitter.instruction(&format!("cmp w8, #{}", b's'));                         // accept PHP mode "s" for sysname
    emitter.instruction("b.eq __rt_php_uname_mode_ok");                         // finish validation when mode "s" was requested
    emitter.instruction(&format!("cmp w8, #{}", b'v'));                         // accept PHP mode "v" for version
    emitter.instruction("b.eq __rt_php_uname_mode_ok");                         // finish validation when mode "v" was requested
    emitter.instruction("b __rt_php_uname_mode_value_fail");                    // reject any single-byte mode outside PHP's accepted set
    emitter.label("__rt_php_uname_mode_ok");
}

fn emit_aarch64_copy_selected(emitter: &mut Emitter) {
    emitter.label("__rt_php_uname_copy_selected");
    emitter.instruction("ldrb w11, [x10], #1");                                 // read the next byte from the selected utsname C string
    emitter.instruction("cbz w11, __rt_php_uname_done");                        // stop once the selected field's C null terminator is reached
    emitter.instruction("strb w11, [x9], #1");                                  // append the selected field byte to the concat-backed result
    emitter.instruction("add x2, x2, #1");                                      // count the appended byte in the returned string length
    emitter.instruction("b __rt_php_uname_copy_selected");                      // continue copying the selected utsname field
}

fn emit_aarch64_copy_field(
    emitter: &mut Emitter,
    label: &str,
    offset: usize,
    append_space: bool,
) {
    let loop_label = format!("__rt_php_uname_copy_{}", label);
    let done_label = format!("__rt_php_uname_copy_{}_done", label);
    emitter.instruction(&format!("add x10, sp, #{}", offset));                  // select the next utsname field for the complete uname string
    emitter.label(&loop_label);
    emitter.instruction("ldrb w11, [x10], #1");                                 // read the next byte from the current utsname field
    emitter.instruction(&format!("cbz w11, {}", done_label));                   // stop copying this field at its C null terminator
    emitter.instruction("strb w11, [x9], #1");                                  // append the current field byte to the complete uname string
    emitter.instruction("add x2, x2, #1");                                      // count the appended field byte in the complete uname length
    emitter.instruction(&format!("b {}", loop_label));                          // continue copying the current utsname field
    emitter.label(&done_label);
    if append_space {
        emitter.instruction("mov w11, #32");                                    // materialize the space separator used by php_uname(\"a\")
        emitter.instruction("strb w11, [x9], #1");                              // append the separator between adjacent utsname fields
        emitter.instruction("add x2, x2, #1");                                  // count the separator in the complete uname length
    }
}

fn emit_aarch64_done(emitter: &mut Emitter) {
    emitter.label("__rt_php_uname_done");
    emitter.instruction("ldr x7, [x6]");                                        // reload the concat-buffer write offset from before this result
    emitter.instruction("add x7, x7, x2");                                      // advance the concat-buffer offset by the returned uname length
    emitter.instruction("str x7, [x6]");                                        // publish the updated concat-buffer write offset
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #1536");                                   // release the stack-backed utsname storage
    emitter.instruction("ret");                                                 // return the selected uname string in x1/x2
}

fn emit_aarch64_fail(emitter: &mut Emitter, label: &str, msg_label: &str, msg_len: usize) {
    emitter.label(label);
    abi::emit_symbol_address(emitter, "x1", msg_label);
    emitter.instruction(&format!("mov x2, #{}", msg_len));                      // pass the fatal php_uname diagnostic length to write()
    emitter.instruction("mov x0, #2");                                          // write php_uname diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // use a failing process exit status for invalid php_uname modes
    emitter.syscall(1);
}

fn emit_php_uname_linux_x86_64(emitter: &mut Emitter) {
    let uts_base = -512isize;
    let sysname = x86_field_addr(uts_base, uts_offset(Platform::Linux, 0));
    let nodename = x86_field_addr(uts_base, uts_offset(Platform::Linux, 1));
    let release = x86_field_addr(uts_base, uts_offset(Platform::Linux, 2));
    let version = x86_field_addr(uts_base, uts_offset(Platform::Linux, 3));
    let machine = x86_field_addr(uts_base, uts_offset(Platform::Linux, 4));

    emitter.blank();
    emitter.comment("--- runtime: php_uname ---");
    emitter.label_global("__rt_php_uname");

    // -- validate mode string and preserve the selected mode across libc uname() --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while php_uname uses stack storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for locals and struct utsname
    emitter.instruction("sub rsp, 640");                                        // reserve aligned storage for mode metadata and Linux struct utsname
    emitter.instruction("cmp rdx, 1");                                          // PHP requires php_uname($mode) to receive exactly one mode byte
    emitter.instruction("jne __rt_php_uname_mode_len_fail");                    // reject empty or multi-byte mode strings before reading the mode byte
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // load the requested php_uname mode byte
    emit_x86_64_validate_mode(emitter);
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // preserve the accepted mode byte across the libc uname() call

    // -- ask libc for the target runtime's uname fields --
    emitter.instruction("lea rdi, [rbp - 512]");                                // pass stack storage for struct utsname to libc uname()
    emitter.bl_c("uname");                                                      // fill the stack-backed utsname buffer through libc
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the accepted mode byte after libc returns

    // -- set up concat-backed transient string storage for the result --
    abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current concat-buffer write offset
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("lea r9, [r9 + r11]");                                  // compute the destination cursor for the returned uname string
    emitter.instruction("mov rax, r9");                                         // remember the result start pointer for the string return value
    emitter.instruction("xor edx, edx");                                        // initialize the returned uname string length to zero bytes

    // -- dispatch by PHP mode --
    emitter.instruction(&format!("cmp r8b, {}", b'a'));                         // does the caller want the complete uname string?
    emitter.instruction("je __rt_php_uname_copy_all");                          // mode "a" returns sysname, nodename, release, version, and machine
    emitter.instruction(&format!("cmp r8b, {}", b's'));                         // does the caller want sysname?
    emitter.instruction("je __rt_php_uname_select_sysname");                    // mode "s" returns the operating-system name
    emitter.instruction(&format!("cmp r8b, {}", b'n'));                         // does the caller want nodename?
    emitter.instruction("je __rt_php_uname_select_nodename");                   // mode "n" returns the network node name
    emitter.instruction(&format!("cmp r8b, {}", b'r'));                         // does the caller want release?
    emitter.instruction("je __rt_php_uname_select_release");                    // mode "r" returns the operating-system release
    emitter.instruction(&format!("cmp r8b, {}", b'v'));                         // does the caller want version?
    emitter.instruction("je __rt_php_uname_select_version");                    // mode "v" returns the operating-system version
    emitter.instruction(&format!("cmp r8b, {}", b'm'));                         // does the caller want machine?
    emitter.instruction("je __rt_php_uname_select_machine");                    // mode "m" returns the machine hardware name
    emitter.instruction("jmp __rt_php_uname_mode_value_fail");                  // defensive fallback for any mode that escaped validation

    emitter.label("__rt_php_uname_select_sysname");
    emitter.instruction(&format!("lea rsi, {}", sysname));                      // select utsname.sysname as the C-string source
    emitter.instruction("jmp __rt_php_uname_copy_selected");                    // copy the selected field into concat-backed string storage
    emitter.label("__rt_php_uname_select_nodename");
    emitter.instruction(&format!("lea rsi, {}", nodename));                     // select utsname.nodename as the C-string source
    emitter.instruction("jmp __rt_php_uname_copy_selected");                    // copy the selected field into concat-backed string storage
    emitter.label("__rt_php_uname_select_release");
    emitter.instruction(&format!("lea rsi, {}", release));                      // select utsname.release as the C-string source
    emitter.instruction("jmp __rt_php_uname_copy_selected");                    // copy the selected field into concat-backed string storage
    emitter.label("__rt_php_uname_select_version");
    emitter.instruction(&format!("lea rsi, {}", version));                      // select utsname.version as the C-string source
    emitter.instruction("jmp __rt_php_uname_copy_selected");                    // copy the selected field into concat-backed string storage
    emitter.label("__rt_php_uname_select_machine");
    emitter.instruction(&format!("lea rsi, {}", machine));                      // select utsname.machine as the C-string source
    emitter.instruction("jmp __rt_php_uname_copy_selected");                    // copy the selected field into concat-backed string storage

    emit_x86_64_copy_selected(emitter);

    emitter.label("__rt_php_uname_copy_all");
    emit_x86_64_copy_field(emitter, "sysname_all", &sysname, true);
    emit_x86_64_copy_field(emitter, "nodename_all", &nodename, true);
    emit_x86_64_copy_field(emitter, "release_all", &release, true);
    emit_x86_64_copy_field(emitter, "version_all", &version, true);
    emit_x86_64_copy_field(emitter, "machine_all", &machine, false);
    emitter.instruction("jmp __rt_php_uname_done");                             // finish after assembling the complete uname string

    emit_x86_64_done(emitter);
    emit_x86_64_fail(emitter, "__rt_php_uname_mode_len_fail", "_php_uname_mode_len_msg", MODE_LEN_MSG.len());
    emit_x86_64_fail(
        emitter,
        "__rt_php_uname_mode_value_fail",
        "_php_uname_mode_value_msg",
        MODE_VALUE_MSG.len(),
    );
}

fn x86_field_addr(base: isize, offset: usize) -> String {
    let displacement = base + offset as isize;
    if displacement < 0 {
        format!("[rbp - {}]", -displacement)
    } else if displacement > 0 {
        format!("[rbp + {}]", displacement)
    } else {
        "[rbp]".to_string()
    }
}

fn emit_x86_64_validate_mode(emitter: &mut Emitter) {
    emitter.instruction(&format!("cmp r8b, {}", b'a'));                         // accept PHP mode "a" for the complete uname string
    emitter.instruction("je __rt_php_uname_mode_ok");                           // finish validation when mode "a" was requested
    emitter.instruction(&format!("cmp r8b, {}", b'm'));                         // accept PHP mode "m" for machine
    emitter.instruction("je __rt_php_uname_mode_ok");                           // finish validation when mode "m" was requested
    emitter.instruction(&format!("cmp r8b, {}", b'n'));                         // accept PHP mode "n" for nodename
    emitter.instruction("je __rt_php_uname_mode_ok");                           // finish validation when mode "n" was requested
    emitter.instruction(&format!("cmp r8b, {}", b'r'));                         // accept PHP mode "r" for release
    emitter.instruction("je __rt_php_uname_mode_ok");                           // finish validation when mode "r" was requested
    emitter.instruction(&format!("cmp r8b, {}", b's'));                         // accept PHP mode "s" for sysname
    emitter.instruction("je __rt_php_uname_mode_ok");                           // finish validation when mode "s" was requested
    emitter.instruction(&format!("cmp r8b, {}", b'v'));                         // accept PHP mode "v" for version
    emitter.instruction("je __rt_php_uname_mode_ok");                           // finish validation when mode "v" was requested
    emitter.instruction("jmp __rt_php_uname_mode_value_fail");                  // reject any single-byte mode outside PHP's accepted set
    emitter.label("__rt_php_uname_mode_ok");
}

fn emit_x86_64_copy_selected(emitter: &mut Emitter) {
    emitter.label("__rt_php_uname_copy_selected");
    emitter.instruction("mov r11b, BYTE PTR [rsi]");                            // read the next byte from the selected utsname C string
    emitter.instruction("test r11b, r11b");                                     // check whether the selected field's C null terminator was reached
    emitter.instruction("jz __rt_php_uname_done");                              // stop once the selected field has been fully copied
    emitter.instruction("mov BYTE PTR [r9], r11b");                             // append the selected field byte to the concat-backed result
    emitter.instruction("add rsi, 1");                                          // advance the selected field source pointer
    emitter.instruction("add r9, 1");                                           // advance the concat-backed destination cursor
    emitter.instruction("add rdx, 1");                                          // count the appended byte in the returned string length
    emitter.instruction("jmp __rt_php_uname_copy_selected");                    // continue copying the selected utsname field
}

fn emit_x86_64_copy_field(
    emitter: &mut Emitter,
    label: &str,
    addr: &str,
    append_space: bool,
) {
    let loop_label = format!("__rt_php_uname_copy_{}", label);
    let done_label = format!("__rt_php_uname_copy_{}_done", label);
    emitter.instruction(&format!("lea rsi, {}", addr));                         // select the next utsname field for the complete uname string
    emitter.label(&loop_label);
    emitter.instruction("mov r11b, BYTE PTR [rsi]");                            // read the next byte from the current utsname field
    emitter.instruction("test r11b, r11b");                                     // check whether the current field's C null terminator was reached
    emitter.instruction(&format!("jz {}", done_label));                         // stop copying this field at its C null terminator
    emitter.instruction("mov BYTE PTR [r9], r11b");                             // append the current field byte to the complete uname string
    emitter.instruction("add rsi, 1");                                          // advance the current field source pointer
    emitter.instruction("add r9, 1");                                           // advance the concat-backed destination cursor
    emitter.instruction("add rdx, 1");                                          // count the appended field byte in the complete uname length
    emitter.instruction(&format!("jmp {}", loop_label));                        // continue copying the current utsname field
    emitter.label(&done_label);
    if append_space {
        emitter.instruction("mov BYTE PTR [r9], 32");                           // append the space separator used by php_uname(\"a\")
        emitter.instruction("add r9, 1");                                       // advance past the appended separator byte
        emitter.instruction("add rdx, 1");                                      // count the separator in the complete uname length
    }
}

fn emit_x86_64_done(emitter: &mut Emitter) {
    emitter.label("__rt_php_uname_done");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the concat-buffer write offset from before this result
    emitter.instruction("add r11, rdx");                                        // advance the concat-buffer offset by the returned uname length
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the updated concat-buffer write offset
    emitter.instruction("add rsp, 640");                                        // release the stack-backed utsname storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the selected uname string in rax/rdx
}

fn emit_x86_64_fail(emitter: &mut Emitter, label: &str, msg_label: &str, msg_len: usize) {
    emitter.label(label);
    abi::emit_symbol_address(emitter, "rsi", msg_label);
    emitter.instruction(&format!("mov edx, {}", msg_len));                      // pass the fatal php_uname diagnostic length to write()
    emitter.instruction("mov edi, 2");                                          // write php_uname diagnostics to stderr
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 writes the diagnostic bytes
    emitter.instruction("syscall");                                             // emit the invalid php_uname mode diagnostic
    emitter.instruction("mov edi, 1");                                          // use a failing process exit status for invalid php_uname modes
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 exits the process
    emitter.instruction("syscall");                                             // terminate after the fatal php_uname diagnostic
}
