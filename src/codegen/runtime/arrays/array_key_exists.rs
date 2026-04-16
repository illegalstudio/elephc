use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_key_exists: check if an integer key exists in an indexed array.
/// Input: x0 = array pointer, x1 = key (integer index)
/// Output: x0 = 1 if key exists, 0 if not
pub fn emit_array_key_exists(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_key_exists_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_key_exists ---");
    emitter.label_global("__rt_array_key_exists");

    // -- check if key is in bounds [0, length) --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length from header
    emitter.instruction("cmp x1, #0");                                          // check if key is negative
    emitter.instruction("b.lt __rt_array_key_exists_no");                       // negative keys don't exist
    emitter.instruction("cmp x1, x9");                                          // compare key with array length
    emitter.instruction("b.ge __rt_array_key_exists_no");                       // if key >= length, does not exist

    // -- key exists --
    emitter.instruction("mov x0, #1");                                          // return true
    emitter.instruction("ret");                                                 // return to caller

    // -- key does not exist --
    emitter.label("__rt_array_key_exists_no");
    emitter.instruction("mov x0, #0");                                          // return false
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_array_key_exists_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_key_exists ---");
    emitter.label_global("__rt_array_key_exists");

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the current array length from the indexed-array header
    emitter.instruction("cmp rsi, 0");                                          // negative integer keys never exist in indexed arrays
    emitter.instruction("jl __rt_array_key_exists_no");                         // reject negative keys before the upper-bound comparison
    emitter.instruction("cmp rsi, r10");                                        // compare the candidate key against the current array length
    emitter.instruction("jge __rt_array_key_exists_no");                        // keys at or beyond length do not exist in the indexed array
    emitter.instruction("mov rax, 1");                                          // return true once the key is proven to be in bounds
    emitter.instruction("ret");                                                 // return the success flag to the caller

    emitter.label("__rt_array_key_exists_no");
    emitter.instruction("xor eax, eax");                                        // return false when the integer key is out of bounds
    emitter.instruction("ret");                                                 // return the failure flag to the caller
}
