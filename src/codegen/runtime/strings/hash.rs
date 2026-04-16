use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash: compute hash of data using named algorithm.
/// Input: x1/x2=algorithm name, x3/x4=data ptr/len
/// Output: x1/x2=hex string in concat_buf
/// Supports: "md5" (CC_MD5, 16 bytes), "sha1" (CC_SHA1, 20 bytes), "sha256" (CC_SHA256, 32 bytes)
pub fn emit_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash ---");
    emitter.label_global("__rt_hash");
    emitter.instruction("sub sp, sp, #96");                                     // allocate stack frame (32 bytes hash + state)
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set frame pointer
    emitter.instruction("stp x3, x4, [sp, #64]");                               // save data ptr/len

    // -- check algorithm name --
    // Compare first char to dispatch quickly
    emitter.instruction("ldrb w9, [x1]");                                       // load first char of algo name

    // -- check for "md5" (len=3, starts with 'm') --
    emitter.instruction("cmp w9, #109");                                        // 'm'?
    emitter.instruction("b.ne __rt_hash_try_sha");                              // no → try sha*
    emitter.instruction("ldp x0, x1, [sp, #64]");                               // x0=data ptr, x1 unused
    emitter.instruction("ldr x0, [sp, #64]");                                   // x0 = data ptr
    emitter.instruction("ldr x1, [sp, #72]");                                   // w1 = data len
    emitter.instruction("mov w1, w1");                                          // truncate to 32-bit CC_LONG
    emitter.instruction("add x2, sp, #0");                                      // output buffer
    emitter.bl_c("CC_MD5");                                          // call CommonCrypto MD5
    emitter.instruction("mov x5, #16");                                         // hash size = 16 bytes
    emitter.instruction("b __rt_hash_hex");                                     // convert to hex

    // -- check for "sha1" or "sha256" --
    emitter.label("__rt_hash_try_sha");
    emitter.instruction("cmp w9, #115");                                        // 's'?
    emitter.instruction("b.ne __rt_hash_unknown");                              // no → unknown algo

    // Disambiguate sha1 vs sha256 by length
    emitter.instruction("cmp x2, #4");                                          // algo len == 4 → "sha1"
    emitter.instruction("b.eq __rt_hash_sha1");                                 // yes
    // Otherwise assume sha256
    emitter.instruction("ldr x0, [sp, #64]");                                   // x0 = data ptr
    emitter.instruction("ldr x1, [sp, #72]");                                   // data len
    emitter.instruction("mov w1, w1");                                          // truncate to CC_LONG
    emitter.instruction("add x2, sp, #0");                                      // output buffer (32 bytes)
    emitter.bl_c("CC_SHA256");                                       // call CommonCrypto SHA256
    emitter.instruction("mov x5, #32");                                         // hash size = 32 bytes
    emitter.instruction("b __rt_hash_hex");                                     // convert to hex

    emitter.label("__rt_hash_sha1");
    emitter.instruction("ldr x0, [sp, #64]");                                   // x0 = data ptr
    emitter.instruction("ldr x1, [sp, #72]");                                   // data len
    emitter.instruction("mov w1, w1");                                          // truncate to CC_LONG
    emitter.instruction("add x2, sp, #0");                                      // output buffer (20 bytes)
    emitter.bl_c("CC_SHA1");                                         // call CommonCrypto SHA1
    emitter.instruction("mov x5, #20");                                         // hash size = 20 bytes
    emitter.instruction("b __rt_hash_hex");                                     // convert to hex

    // -- unknown algorithm: return empty string --
    emitter.label("__rt_hash_unknown");
    emitter.instruction("mov x2, #0");                                          // empty result
    emitter.instruction("b __rt_hash_done");                                    // skip hex conversion

    // -- convert raw hash bytes to hex string --
    emitter.label("__rt_hash_hex");
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("add x11, sp, #0");                                     // source = raw hash bytes
    emitter.instruction("mov x12, x5");                                         // bytes to convert

    emitter.label("__rt_hash_hex_loop");
    emitter.instruction("cbz x12, __rt_hash_hex_done");                         // all bytes converted
    emitter.instruction("ldrb w13, [x11], #1");                                 // load byte, advance
    emitter.instruction("sub x12, x12, #1");                                    // decrement counter
    // -- high nibble --
    emitter.instruction("lsr w14, w13, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w14, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_hash_hi_af");                                // yes → a-f
    emitter.instruction("add w14, w14, #48");                                   // 0-9 → '0'-'9'
    emitter.instruction("b __rt_hash_hi_st");                                   // store
    emitter.label("__rt_hash_hi_af");
    emitter.instruction("add w14, w14, #87");                                   // 10-15 → 'a'-'f'
    emitter.label("__rt_hash_hi_st");
    emitter.instruction("strb w14, [x9], #1");                                  // write high hex char
    // -- low nibble --
    emitter.instruction("and w14, w13, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w14, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_hash_lo_af");                                // yes → a-f
    emitter.instruction("add w14, w14, #48");                                   // 0-9 → '0'-'9'
    emitter.instruction("b __rt_hash_lo_st");                                   // store
    emitter.label("__rt_hash_lo_af");
    emitter.instruction("add w14, w14, #87");                                   // 10-15 → 'a'-'f'
    emitter.label("__rt_hash_lo_st");
    emitter.instruction("strb w14, [x9], #1");                                  // write low hex char
    emitter.instruction("b __rt_hash_hex_loop");                                // next byte

    emitter.label("__rt_hash_hex_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset

    emitter.label("__rt_hash_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame
    emitter.instruction("add sp, sp, #96");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

fn emit_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash ---");
    emitter.label_global("__rt_hash");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving aligned digest scratch space for hash()
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base so hash() can preserve the data string and raw digest buffer across helper calls
    emitter.instruction("sub rsp, 96");                                         // reserve aligned stack space for a 32-byte raw digest buffer plus saved data ptr/len scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the data string pointer while hash() inspects the algorithm name and dispatches to the correct digest helper
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the data string length while hash() inspects the algorithm name and dispatches to the correct digest helper

    // -- check algorithm name --
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // load the first algorithm-name byte so hash() can dispatch quickly between md5/sha1/sha256
    emitter.instruction("cmp cl, 109");                                         // does the algorithm name begin with 'm', which this runtime treats as the md5() family?
    emitter.instruction("jne __rt_hash_try_sha_linux_x86_64");                  // continue to the sha* dispatch when the algorithm does not start with 'm'
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the data string pointer before invoking the MD5 C helper
    emitter.instruction("mov esi, DWORD PTR [rbp - 16]");                       // reload the data string length in the 32-bit size expected by the MD5 C helper
    emitter.instruction("lea rdx, [rbp - 64]");                                 // pass a stack-backed 16-byte output buffer as the MD5 digest destination
    emitter.bl_c("CC_MD5");                                                     // compute the raw MD5 digest bytes for hash(\"md5\", ...)
    emitter.instruction("mov rsi, 16");                                         // record the raw digest size so the shared hexadecimal conversion path formats 16 bytes for md5
    emitter.instruction("jmp __rt_hash_hex_linux_x86_64");                      // convert the raw md5 digest bytes to lowercase hexadecimal through the shared formatting path

    emitter.label("__rt_hash_try_sha_linux_x86_64");
    emitter.instruction("cmp cl, 115");                                         // does the algorithm name begin with 's', which this runtime treats as the sha* family?
    emitter.instruction("jne __rt_hash_unknown_linux_x86_64");                  // return an empty string when the requested algorithm is outside the currently supported md5/sha1/sha256 set
    emitter.instruction("cmp rdx, 4");                                          // is the algorithm-name length exactly 4, which this runtime treats as sha1?
    emitter.instruction("je __rt_hash_sha1_linux_x86_64");                      // dispatch to the SHA1 helper when the algorithm name is four bytes long
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the data string pointer before invoking the SHA256 C helper
    emitter.instruction("mov esi, DWORD PTR [rbp - 16]");                       // reload the data string length in the 32-bit size expected by the SHA256 C helper
    emitter.instruction("lea rdx, [rbp - 64]");                                 // pass a stack-backed 32-byte output buffer as the SHA256 digest destination
    emitter.bl_c("CC_SHA256");                                                  // compute the raw SHA256 digest bytes for hash(\"sha256\", ...)
    emitter.instruction("mov rsi, 32");                                         // record the raw digest size so the shared hexadecimal conversion path formats 32 bytes for sha256
    emitter.instruction("jmp __rt_hash_hex_linux_x86_64");                      // convert the raw sha256 digest bytes to lowercase hexadecimal through the shared formatting path

    emitter.label("__rt_hash_sha1_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the data string pointer before invoking the SHA1 C helper
    emitter.instruction("mov esi, DWORD PTR [rbp - 16]");                       // reload the data string length in the 32-bit size expected by the SHA1 C helper
    emitter.instruction("lea rdx, [rbp - 64]");                                 // pass a stack-backed 20-byte output buffer as the SHA1 digest destination
    emitter.bl_c("CC_SHA1");                                                    // compute the raw SHA1 digest bytes for hash(\"sha1\", ...)
    emitter.instruction("mov rsi, 20");                                         // record the raw digest size so the shared hexadecimal conversion path formats 20 bytes for sha1
    emitter.instruction("jmp __rt_hash_hex_linux_x86_64");                      // convert the raw sha1 digest bytes to lowercase hexadecimal through the shared formatting path

    emitter.label("__rt_hash_unknown_linux_x86_64");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset so an unsupported algorithm can still return a valid empty string slice
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea rax, [r10 + r9]");                                 // return the current concat-buffer cursor as the start pointer of the empty result string for unsupported algorithms
    emitter.instruction("xor rdx, rdx");                                        // return a zero-length string when the requested hash algorithm is unsupported
    emitter.instruction("add rsp, 96");                                         // release the stack-backed digest scratch space before returning the empty string result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty hash() result
    emitter.instruction("ret");                                                 // return the empty string result for unsupported algorithms in the standard x86_64 string result registers

    emitter.label("__rt_hash_hex_linux_x86_64");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before formatting the raw digest bytes as lowercase hexadecimal
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the lowercase hexadecimal digest begins
    emitter.instruction("mov r8, r11");                                         // preserve the concat-backed digest start pointer for the returned string value after the hex loop mutates the destination cursor
    emitter.instruction("lea rcx, [rbp - 64]");                                 // seed the raw-digest source cursor with the local digest output buffer filled by the dispatched C helper

    emitter.label("__rt_hash_hex_loop_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // stop once every raw digest byte selected by the algorithm dispatch has been formatted into lowercase hexadecimal
    emitter.instruction("jz __rt_hash_hex_done_linux_x86_64");                  // finish once the full raw digest has been converted to lowercase hex characters
    emitter.instruction("movzx edx, BYTE PTR [rcx]");                           // load one raw digest byte before splitting it into high and low hexadecimal nibbles
    emitter.instruction("add rcx, 1");                                          // advance the raw-digest source cursor after consuming one digest byte
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining raw-digest byte count after consuming one digest byte
    emitter.instruction("mov eax, edx");                                        // copy the raw digest byte before extracting its high hexadecimal nibble for lowercase formatting
    emitter.instruction("shr al, 4");                                           // isolate the high nibble of the raw digest byte so it can be rendered as lowercase hexadecimal
    emitter.instruction("cmp al, 10");                                          // does the high nibble require an alphabetic lowercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_hash_hi_af_linux_x86_64");                    // map nibble values 10-15 to 'a'-'f' for the high hexadecimal digit
    emitter.instruction("add al, 48");                                          // map nibble values 0-9 to '0'-'9' for the high hexadecimal digit
    emitter.instruction("jmp __rt_hash_hi_store_linux_x86_64");                 // skip the alphabetic-nibble mapping once the high digit has been converted

    emitter.label("__rt_hash_hi_af_linux_x86_64");
    emitter.instruction("add al, 87");                                          // map nibble values 10-15 to 'a'-'f' for the high hexadecimal digit

    emitter.label("__rt_hash_hi_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the high lowercase hexadecimal digit of the current digest byte into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the high lowercase hexadecimal digit
    emitter.instruction("mov eax, edx");                                        // reload the raw digest byte before extracting its low hexadecimal nibble for lowercase formatting
    emitter.instruction("and al, 15");                                          // isolate the low nibble of the raw digest byte so it can be rendered as lowercase hexadecimal
    emitter.instruction("cmp al, 10");                                          // does the low nibble require an alphabetic lowercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_hash_lo_af_linux_x86_64");                    // map nibble values 10-15 to 'a'-'f' for the low hexadecimal digit
    emitter.instruction("add al, 48");                                          // map nibble values 0-9 to '0'-'9' for the low hexadecimal digit
    emitter.instruction("jmp __rt_hash_lo_store_linux_x86_64");                 // skip the alphabetic-nibble mapping once the low digit has been converted

    emitter.label("__rt_hash_lo_af_linux_x86_64");
    emitter.instruction("add al, 87");                                          // map nibble values 10-15 to 'a'-'f' for the low hexadecimal digit

    emitter.label("__rt_hash_lo_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the low lowercase hexadecimal digit of the current digest byte into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the low lowercase hexadecimal digit
    emitter.instruction("jmp __rt_hash_hex_loop_linux_x86_64");                 // continue formatting the remaining raw digest bytes into lowercase hexadecimal

    emitter.label("__rt_hash_hex_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the concat-backed start pointer of the lowercase hexadecimal digest string produced by hash()
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer destination cursor before computing the lowercase hexadecimal digest length
    emitter.instruction("sub rdx, r8");                                         // compute the lowercase hexadecimal digest length as dest_end - dest_start for the returned x86_64 string value
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset before publishing the lowercase hexadecimal digest bytes that hash() appended
    emitter.instruction("add rcx, rdx");                                        // advance the concat-buffer write offset by the produced lowercase hexadecimal digest length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset after formatting the dispatched digest
    emitter.instruction("add rsp, 96");                                         // release the stack-backed digest scratch space before returning the lowercase hexadecimal hash() result string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the lowercase hexadecimal hash() result string
    emitter.instruction("ret");                                                 // return the lowercase hexadecimal hash() result in the standard x86_64 string result registers
}
