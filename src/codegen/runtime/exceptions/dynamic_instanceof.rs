use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub fn emit_dynamic_instanceof(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_dynamic_instanceof_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: dynamic_instanceof ---");
    emitter.label_global("__rt_instanceof_lookup");

    // -- scan class/interface target-name metadata --
    emitter.instruction("cbz x1, __rt_instanceof_lookup_no");                   // null string pointers cannot name a class or interface
    emitter.adrp("x9", "_instanceof_target_count");                            // load the page containing the dynamic instanceof target count
    emitter.add_lo12("x9", "x9", "_instanceof_target_count");                 // resolve the dynamic instanceof target-count address
    emitter.instruction("ldr x9, [x9]");                                        // x9 = number of class/interface names available for lookup
    emitter.adrp("x10", "_instanceof_target_entries");                         // load the page containing the dynamic instanceof target table
    emitter.add_lo12("x10", "x10", "_instanceof_target_entries");             // resolve the dynamic instanceof target table address
    emitter.instruction("mov x11, #0");                                         // x11 = current target-table index

    emitter.label("__rt_instanceof_lookup_entry_loop");
    emitter.instruction("cmp x11, x9");                                         // have all target-name metadata entries been scanned?
    emitter.instruction("b.hs __rt_instanceof_lookup_no");                      // no class/interface name matched the dynamic string target
    emitter.instruction("ldr x12, [x10]");                                      // x12 = candidate class/interface name pointer
    emitter.instruction("ldr x13, [x10, #8]");                                  // x13 = candidate class/interface name length
    emitter.instruction("cmp x2, x13");                                         // compare dynamic target length with this metadata name length
    emitter.instruction("b.ne __rt_instanceof_lookup_next");                    // names with different lengths cannot match
    emitter.instruction("mov x14, #0");                                         // x14 = byte index within the candidate name

    emitter.label("__rt_instanceof_lookup_byte_loop");
    emitter.instruction("cmp x14, x2");                                         // have all bytes in this equal-length name been checked?
    emitter.instruction("b.hs __rt_instanceof_lookup_match");                   // every byte matched case-insensitively
    emitter.instruction("ldrb w15, [x1, x14]");                                 // load a byte from the dynamic target string
    emitter.instruction("ldrb w16, [x12, x14]");                                // load the corresponding metadata-name byte
    emitter.instruction("cmp w15, #65");                                        // test whether the dynamic byte is below uppercase ASCII A
    emitter.instruction("b.lt __rt_instanceof_lookup_rhs");                     // leave non-uppercase dynamic bytes unchanged
    emitter.instruction("cmp w15, #90");                                        // test whether the dynamic byte is above uppercase ASCII Z
    emitter.instruction("b.gt __rt_instanceof_lookup_rhs");                     // leave non-uppercase dynamic bytes unchanged
    emitter.instruction("add w15, w15, #32");                                   // lowercase the dynamic target byte for PHP class-name lookup

    emitter.label("__rt_instanceof_lookup_rhs");
    emitter.instruction("cmp w16, #65");                                        // test whether the metadata byte is below uppercase ASCII A
    emitter.instruction("b.lt __rt_instanceof_lookup_cmp");                     // leave non-uppercase metadata bytes unchanged
    emitter.instruction("cmp w16, #90");                                        // test whether the metadata byte is above uppercase ASCII Z
    emitter.instruction("b.gt __rt_instanceof_lookup_cmp");                     // leave non-uppercase metadata bytes unchanged
    emitter.instruction("add w16, w16, #32");                                   // lowercase the metadata byte for case-insensitive comparison

    emitter.label("__rt_instanceof_lookup_cmp");
    emitter.instruction("cmp w15, w16");                                        // compare the lowercased dynamic and metadata bytes
    emitter.instruction("b.ne __rt_instanceof_lookup_next");                    // this metadata name is not the requested target
    emitter.instruction("add x14, x14, #1");                                    // advance to the next byte in this target-name candidate
    emitter.instruction("b __rt_instanceof_lookup_byte_loop");                  // continue comparing the current target-name candidate

    emitter.label("__rt_instanceof_lookup_match");
    emitter.instruction("ldr x1, [x10, #16]");                                  // return the matched class/interface id
    emitter.instruction("ldr x2, [x10, #24]");                                  // return 0 for class targets or 1 for interface targets
    emitter.instruction("mov x0, #1");                                          // signal that the dynamic target string resolved successfully
    emitter.instruction("ret");                                                 // return lookup success plus target metadata

    emitter.label("__rt_instanceof_lookup_next");
    emitter.instruction("add x10, x10, #32");                                   // advance to the next four-word target metadata entry
    emitter.instruction("add x11, x11, #1");                                    // increment the target metadata index
    emitter.instruction("b __rt_instanceof_lookup_entry_loop");                 // continue scanning dynamic instanceof target metadata

    emitter.label("__rt_instanceof_lookup_no");
    emitter.instruction("mov x0, #0");                                          // signal that no class/interface target matched the string
    emitter.instruction("mov x1, #0");                                          // clear the target id result on failed lookup
    emitter.instruction("mov x2, #0");                                          // clear the target kind result on failed lookup
    emitter.instruction("ret");                                                 // return lookup failure

    emitter.blank();
    emitter.label_global("__rt_instanceof_invalid_target");
    emitter.adrp("x1", "_instanceof_target_type_msg");                         // load the page containing the dynamic-target TypeError message
    emitter.add_lo12("x1", "x1", "_instanceof_target_type_msg");               // resolve the dynamic-target TypeError message address
    emitter.instruction("mov x2, #59");                                         // pass the dynamic-target TypeError message length to write()
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the dynamic instanceof TypeError
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit status 1 indicates abnormal termination
    emitter.syscall(1);
}

fn emit_dynamic_instanceof_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dynamic_instanceof ---");
    emitter.label_global("__rt_instanceof_lookup");

    emitter.instruction("test rax, rax");                                       // null string pointers cannot name a class or interface
    emitter.instruction("je __rt_instanceof_lookup_no");                        // report lookup failure for null dynamic target strings
    emitter.instruction("mov r8, rax");                                         // preserve the dynamic target string pointer
    emitter.instruction("mov r9, rdx");                                         // preserve the dynamic target string length
    emitter.instruction("mov r10, QWORD PTR [rip + _instanceof_target_count]"); // r10 = number of class/interface names available for lookup
    emitter.instruction("lea r11, [rip + _instanceof_target_entries]");         // r11 = current dynamic-target metadata entry

    emitter.label("__rt_instanceof_lookup_entry_loop");
    emitter.instruction("test r10, r10");                                       // have all target-name metadata entries been scanned?
    emitter.instruction("je __rt_instanceof_lookup_no");                        // no class/interface name matched the dynamic string target
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // rdi = candidate class/interface name pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // rsi = candidate class/interface name length
    emitter.instruction("cmp r9, rsi");                                         // compare dynamic target length with this metadata name length
    emitter.instruction("jne __rt_instanceof_lookup_next");                     // names with different lengths cannot match
    emitter.instruction("xor ecx, ecx");                                        // rcx = byte index within the candidate name

    emitter.label("__rt_instanceof_lookup_byte_loop");
    emitter.instruction("cmp rcx, r9");                                         // have all bytes in this equal-length name been checked?
    emitter.instruction("jae __rt_instanceof_lookup_match");                    // every byte matched case-insensitively
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load a byte from the dynamic target string
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load the corresponding metadata-name byte
    emitter.instruction("cmp al, 65");                                          // test whether the dynamic byte is below uppercase ASCII A
    emitter.instruction("jb __rt_instanceof_lookup_rhs");                       // leave non-uppercase dynamic bytes unchanged
    emitter.instruction("cmp al, 90");                                          // test whether the dynamic byte is above uppercase ASCII Z
    emitter.instruction("ja __rt_instanceof_lookup_rhs");                       // leave non-uppercase dynamic bytes unchanged
    emitter.instruction("add al, 32");                                          // lowercase the dynamic target byte for PHP class-name lookup

    emitter.label("__rt_instanceof_lookup_rhs");
    emitter.instruction("cmp dl, 65");                                          // test whether the metadata byte is below uppercase ASCII A
    emitter.instruction("jb __rt_instanceof_lookup_cmp");                       // leave non-uppercase metadata bytes unchanged
    emitter.instruction("cmp dl, 90");                                          // test whether the metadata byte is above uppercase ASCII Z
    emitter.instruction("ja __rt_instanceof_lookup_cmp");                       // leave non-uppercase metadata bytes unchanged
    emitter.instruction("add dl, 32");                                          // lowercase the metadata byte for case-insensitive comparison

    emitter.label("__rt_instanceof_lookup_cmp");
    emitter.instruction("cmp al, dl");                                          // compare the lowercased dynamic and metadata bytes
    emitter.instruction("jne __rt_instanceof_lookup_next");                     // this metadata name is not the requested target
    emitter.instruction("add rcx, 1");                                          // advance to the next byte in this target-name candidate
    emitter.instruction("jmp __rt_instanceof_lookup_byte_loop");                // continue comparing the current target-name candidate

    emitter.label("__rt_instanceof_lookup_match");
    emitter.instruction("mov rdi, QWORD PTR [r11 + 16]");                       // return the matched class/interface id
    emitter.instruction("mov rdx, QWORD PTR [r11 + 24]");                       // return 0 for class targets or 1 for interface targets
    emitter.instruction("mov eax, 1");                                          // signal that the dynamic target string resolved successfully
    emitter.instruction("ret");                                                 // return lookup success plus target metadata

    emitter.label("__rt_instanceof_lookup_next");
    emitter.instruction("add r11, 32");                                         // advance to the next four-word target metadata entry
    emitter.instruction("sub r10, 1");                                          // consume one target metadata entry
    emitter.instruction("jmp __rt_instanceof_lookup_entry_loop");               // continue scanning dynamic instanceof target metadata

    emitter.label("__rt_instanceof_lookup_no");
    emitter.instruction("xor eax, eax");                                        // signal that no class/interface target matched the string
    emitter.instruction("xor edi, edi");                                        // clear the target id result on failed lookup
    emitter.instruction("xor edx, edx");                                        // clear the target kind result on failed lookup
    emitter.instruction("ret");                                                 // return lookup failure

    emitter.blank();
    emitter.label_global("__rt_instanceof_invalid_target");
    emitter.instruction("lea rsi, [rip + _instanceof_target_type_msg]");        // point write() at the dynamic-target TypeError message
    emitter.instruction("mov edx, 59");                                         // pass the dynamic-target TypeError message length to write()
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the dynamic instanceof TypeError
    emitter.instruction("mov eax, 1");                                          // Linux syscall 1 = write
    emitter.instruction("syscall");                                             // emit the dynamic instanceof TypeError diagnostic
    emitter.instruction("mov edi, 1");                                          // exit status 1 indicates abnormal termination
    emitter.instruction("mov eax, 60");                                         // Linux syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate after the dynamic instanceof TypeError
}
