//! Purpose:
//! Exercises shared mbstring exception-chain materialization with real native heap objects.
//!
//! Called from:
//! - The codegen harness on Linux x86_64, Linux ARM64, and macOS ARM64.
//!
//! Key details:
//! - A synthetic wire result isolates class, binary-message, previous, and ownership semantics.
//! - The same harness rejects malformed trailing records without publishing partial chains.

use crate::support::*;
use elephc::codegen_support::platform::{Arch, Target};
use elephc_builtin_contract::mbstring_abi::{exception::{encode, Exception}, RESULT_ERROR, RESULT_VALUE_ERROR,
    RESULT_EXCEPTION_CHAIN};

/// Checks native class/message/previous fields and releases the entire chain under heap debugging.
#[test]
fn test_mbstring_exception_chain_native_materialization() {
    let mut emitter = Harness::new(target());
    let arm = target().arch == Arch::AArch64;
    let bytes = encode(&[Exception { kind: RESULT_VALUE_ERROR, message: b"older\0\xff" },
        Exception { kind: RESULT_ERROR, message: b"new\0\xfe" }]).unwrap();
    if arm {
        emitter.instruction("sub sp, sp, #16");                                 // retain the owned chain across field validation and cleanup
    } else {
        emitter.instruction("sub rsp, 16");                                     // reserve aligned storage for the complete Throwable chain
    }
    load_symbol(&mut emitter, if arm { "x9" } else { "r10" }, "_gc_live", 0);
    if arm {
        emitter.instruction("str x9, [sp, #8]");                                // record live native bytes before constructing any exception
    } else {
        emitter.instruction("mov QWORD PTR [rsp + 8], r10");                    // preserve the native ownership baseline across both materializer calls
    }
    symbol_address(&mut emitter, if arm { "x0" } else { "rdi" }, "__mb_exception_test_wire");
    call_label(&mut emitter, "__rt_mbstring_exception_chain");
    if arm {
        emitter.instruction("cmp x0, #2");                                      // require a published native PendingThrowable result
        emitter.instruction("b.ne __mb_exception_test_fail");                   // reject a fabricated success or transport failure
    } else {
        emitter.instruction("cmp eax, 2");                                      // require a real pending Throwable chain
        emitter.instruction("jne __mb_exception_test_fail");                    // fail if native materialization did not publish the error
    }
    load_symbol(&mut emitter, if arm { "x11" } else { "r11" }, "_exc_value", 0);
    if arm {
        emitter.instruction("str x11, [sp]");                                   // retain the complete chain's sole owner for later release
    } else {
        emitter.instruction("mov QWORD PTR [rsp], r11");                        // retain the latest Throwable owning all earlier errors
    }
    assert_record(&mut emitter, "_spl_error_class_id", b"new\0\xfe");
    if arm {
        emitter.instruction("ldr x11, [x11, #40]");                             // follow the latest error's raw previous pointer
    } else {
        emitter.instruction("mov r11, QWORD PTR [r11 + 40]");                   // follow the previous owner without retaining another reference
    }
    assert_record(&mut emitter, "_spl_value_error_class_id", b"older\0\xff");
    if arm {
        emitter.instruction("ldr x9, [x11, #40]");                              // inspect the oldest error's previous slot
        emitter.instruction("cbnz x9, __mb_exception_test_fail");               // reject reversed, cyclic, or extra exception links
        emitter.instruction("ldr x0, [sp]");                                    // consume the complete chain through its single root owner
    } else {
        emitter.instruction("cmp QWORD PTR [r11 + 40], 0");                     // require an exact two-error chain
        emitter.instruction("jne __mb_exception_test_fail");                    // reject an extra or cyclic previous link
        emitter.instruction("mov rax, QWORD PTR [rsp]");                        // consume the complete chain's single root owner
    }
    zero_symbol(&mut emitter, "_exc_value", 0);
    call_label(&mut emitter, "__rt_decref_any");
    assert_heap_baseline(&mut emitter);
    symbol_address(&mut emitter, if arm { "x0" } else { "rdi" }, "__mb_exception_test_invalid");
    call_label(&mut emitter, "__rt_mbstring_exception_chain");
    if arm {
        emitter.instruction("cmp x0, #1");                                      // malformed trailing records must fail before allocating visible errors
        emitter.instruction("b.ne __mb_exception_test_fail");                   // reject partial success on inconsistent record counts
    } else {
        emitter.instruction("cmp eax, 1");                                      // require a fatal transport status for malformed framing
        emitter.instruction("jne __mb_exception_test_fail");                    // reject partial materialization or a fabricated pending error
    }
    assert_heap_baseline(&mut emitter);
    load_symbol(&mut emitter, if arm { "x9" } else { "r10" }, "_exc_value", 0);
    if arm {
        emitter.instruction("cbnz x9, __mb_exception_test_fail");               // malformed framing must leave the native pending slot empty
        emitter.instruction("add sp, sp, #16");                                 // release fixture storage before normal program teardown
        emitter.instruction("b __mb_exception_test_done");                      // skip the failure exit and static borrowed wire buffers
    } else {
        emitter.instruction("test r10, r10");                                   // verify no partial native chain was published
        emitter.instruction("jnz __mb_exception_test_fail");                    // reject leaked pending exception ownership
        emitter.instruction("add rsp, 16");                                     // restore the program's aligned main frame
        emitter.instruction("jmp __mb_exception_test_done");                    // resume normal teardown after fixture data
    }
    emitter.label("__mb_exception_test_fail");
    if arm {
        emitter.instruction("mov x0, #91");                                     // identify a native exception-field or ownership regression
    } else {
        emitter.instruction("mov edi, 91");                                     // identify a failed native exception-chain assertion
    }
    emitter.bl_c("exit");
    emitter.directive(".data");
    emitter.directive(".p2align 3");
    emitter.label("__mb_exception_test_wire");
    emitter.directive(&format!(".quad {RESULT_EXCEPTION_CHAIN}, 2, __mb_exception_test_payload, {}, 0, 0", bytes.len()));
    emitter.label("__mb_exception_test_invalid");
    emitter.directive(&format!(".quad {RESULT_EXCEPTION_CHAIN}, 3, __mb_exception_test_payload, {}, 0, 0", bytes.len()));
    emitter.label("__mb_exception_test_payload");
    emitter.directive(&format!(".byte {}", bytes.iter().map(u8::to_string).collect::<Vec<_>>().join(",")));
    emitter.directive(".text");
    emitter.label("__mb_exception_test_done");
    assert_eq!(compile_harness_and_run_with_heap_debug(
        "<?php echo mb_regex_set_options(), \"\\n\";", 65_536, &emitter.output()), "pr\n");
}

/// Requires complete message and previous ownership cleanup rather than only intact heap headers.
fn assert_heap_baseline(emitter: &mut Harness) {
    if emitter.target.arch == Arch::AArch64 {
        load_symbol(emitter, "x9", "_gc_live", 0);
        emitter.instruction("ldr x10, [sp, #8]");                               // restore live bytes measured before constructing the chain
        emitter.instruction("cmp x9, x10");                                     // require all native messages and previous Throwables to have been freed
        emitter.instruction("b.ne __mb_exception_test_fail");                   // reject leaked storage on success or malformed transport
    } else {
        load_symbol(emitter, "r10", "_gc_live", 0);
        emitter.instruction("cmp r10, QWORD PTR [rsp + 8]");                    // require exact restoration of native live-byte ownership
        emitter.instruction("jne __mb_exception_test_fail");                    // reject leaked messages, partial chains, or previous owners
    }
}

/// Verifies one borrowed compact Throwable without changing the current x11/r11 chain cursor.
fn assert_record(emitter: &mut Harness, class: &str, message: &[u8]) {
    let arm = emitter.target.arch == Arch::AArch64;
    load_symbol(emitter, if arm { "x9" } else { "r10" }, class, 0);
    if arm {
        emitter.instruction("cbz x11, __mb_exception_test_fail");               // require this exact chain element to exist
        emitter.instruction("ldr x10, [x11]");                                  // read the native exception's resolved class identity
        emitter.instruction("cmp x10, x9");                                     // compare class identity rather than merely message text
        emitter.instruction("b.ne __mb_exception_test_fail");                   // reject reversed or collapsed exception classes
        emitter.instruction("ldr x10, [x11, #16]");                             // read the binary message length
        emitter.instruction(&format!("cmp x10, #{}", message.len()));           // preserve embedded NUL and non-UTF-8 bytes
        emitter.instruction("b.ne __mb_exception_test_fail");                   // reject truncated or substituted exception messages
        emitter.instruction("ldr x12, [x11, #8]");                              // borrow the native-owned message for byte comparisons
    } else {
        emitter.instruction("test r11, r11");                                   // require a nonnull native Throwable at this chain position
        emitter.instruction("jz __mb_exception_test_fail");                     // reject a dropped previous exception
        emitter.instruction("cmp QWORD PTR [r11], r10");                        // compare the exact program-specific class identity
        emitter.instruction("jne __mb_exception_test_fail");                    // reject class flattening or a reversed chain
        emitter.instruction(&format!("cmp QWORD PTR [r11 + 16], {}", message.len())); // validate the full binary message length
        emitter.instruction("jne __mb_exception_test_fail");                    // reject truncated or repaired error bytes
        emitter.instruction("mov r10, QWORD PTR [r11 + 8]");                    // borrow the message without acquiring a second owner
    }
    for (offset, byte) in message.iter().enumerate() {
        if arm {
            emitter.instruction(&format!("ldrb w9, [x12, #{offset}]"));         // load one exact native-owned message byte
            emitter.instruction(&format!("cmp w9, #{byte}"));                   // include embedded NULs and malformed UTF-8 in the comparison
            emitter.instruction("b.ne __mb_exception_test_fail");               // reject any changed binary exception byte
        } else {
            emitter.instruction(&format!("cmp BYTE PTR [r10 + {offset}], {byte}")); // compare exact native-owned message bytes
            emitter.instruction("jne __mb_exception_test_fail");                // reject binary message corruption
        }
    }
}


/// Target-specific assertion assembly injected into the existing codegen fixture runner.
struct Harness { target: Target, text: String }

impl Harness {
    /// Starts a fixture fragment without emitting another program header or entry point.
    fn new(target: Target) -> Self { Self { target, text: String::new() } }

    /// Appends one instruction using the enclosing program's selected assembly syntax.
    fn instruction(&mut self, text: &str) { self.text.push_str(&format!("    {text}\n")); }

    /// Adds a fixture-local control-flow or data label.
    fn label(&mut self, text: &str) { self.text.push_str(&format!("{text}:\n")); }

    /// Appends static fixture data or a section switch.
    fn directive(&mut self, text: &str) { self.text.push_str(&format!("{text}\n")); }

    /// Calls a C function with the selected target's external symbol spelling.
    fn bl_c(&mut self, name: &str) {
        let opcode = if self.target.arch == Arch::AArch64 { "bl" } else { "call" };
        self.instruction(&format!("{opcode} {}", self.target.extern_symbol(name)));
    }

    /// Transfers the complete fixture fragment into the existing injection helper.
    fn output(self) -> String { self.text }
}

/// Addresses a fixture or runtime symbol in the runner's portable macOS assembly dialect.
fn symbol_address(emitter: &mut Harness, register: &str, symbol: &str) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("adrp {register}, {symbol}@PAGE"));        // locate the native symbol's page through the fixture dialect
        emitter.instruction(&format!("add {register}, {register}, {symbol}@PAGEOFF")); // include the exact in-page symbol offset
    } else {
        emitter.instruction(&format!("lea {register}, [rip + {symbol}]"));      // address the exact native fixture or runtime symbol
    }
}

/// Loads a runtime metadata word while preserving the other fixture assertion registers.
fn load_symbol(emitter: &mut Harness, register: &str, symbol: &str, offset: usize) {
    symbol_address(emitter, register, symbol);
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("ldr {register}, [{register}, #{offset}]")); // borrow the selected runtime metadata word
    } else {
        emitter.instruction(&format!("mov {register}, QWORD PTR [{register} + {offset}]")); // read one exact native metadata word
    }
}

/// Clears a runtime ownership slot after transferring its retained owner to fixture cleanup.
fn zero_symbol(emitter: &mut Harness, symbol: &str, offset: usize) {
    if emitter.target.arch == Arch::AArch64 {
        symbol_address(emitter, "x9", symbol);
        emitter.instruction(&format!("str xzr, [x9, #{offset}]"));              // clear published ownership before releasing the saved root
    } else {
        symbol_address(emitter, "r10", symbol);
        emitter.instruction(&format!("mov QWORD PTR [r10 + {offset}], 0"));     // prevent a stale native pending pointer after chain release
    }
}

/// Calls an internal runtime helper using its existing native symbol spelling.
fn call_label(emitter: &mut Harness, label: &str) {
    let opcode = if emitter.target.arch == Arch::AArch64 { "bl" } else { "call" };
    emitter.instruction(&format!("{opcode} {label}"));                          // execute the production runtime helper from the native fixture
}
