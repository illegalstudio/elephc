//! Purpose:
//! Emits pure runtime resolvers for compact callable-name metadata tables.
//! Supports deterministic linear scans and FNV-1a open-addressed lookup.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Inputs are borrowed and normalization removes at most one leading backslash.
//! - Resolvers allocate nothing and return descriptor plus flags in the custom two-word ABI.
//! - Table keys are already ASCII-lowercase; selectors fold only runtime uppercase bytes.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Fixed FNV-1a offset basis shared with the compile-time table builder.
const FNV1A_OFFSET: u64 = 14_695_981_039_346_656_037;
/// Fixed FNV-1a prime shared with the compile-time table builder.
const FNV1A_PRIME: u64 = 1_099_511_628_211;

/// Emits linear and hashed string-callable lookup helpers for the active target.
pub(crate) fn emit_callable_lookup(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 callable-name lookup helper bodies.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: callable lookup tables ---");
    emitter.label_global("__rt_callable_lookup_string_linear");
    emit_aarch64_normalize_selector(emitter, "__rt_callable_lookup_string_linear_ready");
    emitter.label("__rt_callable_lookup_string_linear_ready");
    emitter.instruction("cbz x3, __rt_callable_lookup_string_linear_miss");     // stop after scanning every canonical callable row
    emitter.label("__rt_callable_lookup_string_linear_loop");
    emitter.instruction("ldr x4, [x2]");                                        // x4 = canonical candidate-name pointer
    emitter.instruction("ldr x5, [x2, #8]");                                    // x5 = canonical candidate-name length
    emitter.instruction("cmp x1, x5");                                          // reject candidates with a different normalized length
    emitter.instruction("b.ne __rt_callable_lookup_string_linear_next");        // advance when the candidate length differs
    emitter.instruction("mov x6, #0");                                          // x6 = byte index within the candidate name
    emitter.label("__rt_callable_lookup_string_linear_bytes");
    emitter.instruction("cmp x6, x1");                                          // have all normalized selector bytes matched?
    emitter.instruction("b.hs __rt_callable_lookup_string_linear_match");       // return this descriptor after a complete match
    emitter.instruction("ldrb w7, [x0, x6]");                                   // load one runtime selector byte
    emit_aarch64_lower_ascii(emitter, "w7", "__rt_callable_lookup_string_linear_rhs");
    emitter.label("__rt_callable_lookup_string_linear_rhs");
    emitter.instruction("ldrb w8, [x4, x6]");                                   // load the canonical table byte
    emitter.instruction("cmp w7, w8");                                          // compare runtime and canonical bytes
    emitter.instruction("b.ne __rt_callable_lookup_string_linear_next");        // advance when this candidate byte differs
    emitter.instruction("add x6, x6, #1");                                      // advance within the current candidate name
    emitter.instruction("b __rt_callable_lookup_string_linear_bytes");          // compare the next selector byte
    emitter.label("__rt_callable_lookup_string_linear_next");
    emitter.instruction("add x2, x2, #32");                                     // advance to the next four-word linear entry
    emitter.instruction("subs x3, x3, #1");                                     // consume one remaining linear entry
    emitter.instruction("b.ne __rt_callable_lookup_string_linear_loop");        // continue while untested entries remain
    emitter.instruction("b __rt_callable_lookup_string_linear_miss");           // report a lookup miss after exhausting the table
    emitter.label("__rt_callable_lookup_string_linear_match");
    emitter.instruction("ldr x0, [x2, #16]");                                   // return the matched descriptor pointer
    emitter.instruction("ldr x1, [x2, #24]");                                   // return the matched lookup flags
    emitter.instruction("ret");                                                 // return one resolved callable descriptor
    // The hash helper below is a SEPARATE linker atom, and its miss epilogue is an internal
    // label, so branching into it from here is a reference `-dead_strip` cannot follow: the
    // atom is collectable and this branch lands wherever the linker put the next one. Three
    // instructions are cheaper to duplicate than to share.
    emitter.label("__rt_callable_lookup_string_linear_miss");
    emitter.instruction("mov x0, #0");                                          // return a null descriptor for lookup miss
    emitter.instruction("mov x1, #0");                                          // clear lookup flags on miss
    emitter.instruction("ret");                                                 // return without allocation or diagnostics

    emitter.blank();
    emitter.label_global("__rt_callable_lookup_string_hash");
    emit_aarch64_normalize_selector(emitter, "__rt_callable_lookup_string_hash_ready");
    emitter.label("__rt_callable_lookup_string_hash_ready");
    emitter.instruction("mov x8, x0");                                          // preserve the normalized selector pointer
    emitter.instruction("mov x9, x1");                                          // preserve the normalized selector length
    emitter.instruction("mov x10, x2");                                         // preserve the open-addressed table base
    emitter.instruction("mov x11, x3");                                         // preserve the table capacity mask
    abi::emit_load_int_immediate(emitter, "x12", FNV1A_OFFSET as i64);
    abi::emit_load_int_immediate(emitter, "x15", FNV1A_PRIME as i64);
    emitter.instruction("mov x13, #0");                                         // x13 = selector byte index for FNV-1a
    emitter.label("__rt_callable_lookup_string_hash_bytes");
    emitter.instruction("cmp x13, x9");                                         // have all selector bytes been hashed?
    emitter.instruction("b.hs __rt_callable_lookup_string_hash_probe_start");   // probe the table after hashing the full selector
    emitter.instruction("ldrb w14, [x8, x13]");                                 // load one runtime selector byte for hashing
    emit_aarch64_lower_ascii(emitter, "w14", "__rt_callable_lookup_string_hash_mix");
    emitter.label("__rt_callable_lookup_string_hash_mix");
    emitter.instruction("eor x12, x12, x14");                                   // xor the canonical selector byte into FNV state
    emitter.instruction("mul x12, x12, x15");                                   // multiply by the fixed FNV-1a prime
    emitter.instruction("add x13, x13, #1");                                    // advance to the next selector byte
    emitter.instruction("b __rt_callable_lookup_string_hash_bytes");            // continue hashing the normalized selector
    emitter.label("__rt_callable_lookup_string_hash_probe_start");
    emitter.instruction("and x13, x12, x11");                                   // select the initial open-address bucket
    emitter.label("__rt_callable_lookup_string_hash_probe");
    emitter.instruction("lsl x16, x13, #5");                                    // x16 = bucket index times 32
    emitter.instruction("add x16, x16, x13, lsl #3");                           // x16 = bucket index times 40
    emitter.instruction("add x17, x10, x16");                                   // x17 = current five-word bucket address
    emitter.instruction("ldr x0, [x17, #24]");                                  // load the bucket descriptor pointer
    emitter.instruction("cbz x0, __rt_callable_lookup_string_miss");            // an empty bucket terminates this probe chain
    emitter.instruction("ldr x1, [x17]");                                       // load the bucket hash before key comparison
    emitter.instruction("cmp x1, x12");                                         // compare the precomputed and runtime hashes
    emitter.instruction("b.ne __rt_callable_lookup_string_hash_next");          // probe the next bucket when hashes differ
    emitter.instruction("ldr x1, [x17, #16]");                                  // load the canonical candidate-name length
    emitter.instruction("cmp x1, x9");                                          // reject hash collisions with a different length
    emitter.instruction("b.ne __rt_callable_lookup_string_hash_next");          // probe onward after a length mismatch
    emitter.instruction("ldr x4, [x17, #8]");                                   // x4 = canonical candidate-name pointer
    emitter.instruction("mov x5, #0");                                          // x5 = byte index for collision-safe equality
    emitter.label("__rt_callable_lookup_string_hash_compare");
    emitter.instruction("cmp x5, x9");                                          // have all bytes in this hash collision matched?
    emitter.instruction("b.hs __rt_callable_lookup_string_hash_match");         // return the descriptor after full equality
    emitter.instruction("ldrb w6, [x8, x5]");                                   // load one runtime selector byte
    emit_aarch64_lower_ascii(emitter, "w6", "__rt_callable_lookup_string_hash_rhs");
    emitter.label("__rt_callable_lookup_string_hash_rhs");
    emitter.instruction("ldrb w7, [x4, x5]");                                   // load the canonical table byte
    emitter.instruction("cmp w6, w7");                                          // compare normalized bytes after the hash match
    emitter.instruction("b.ne __rt_callable_lookup_string_hash_next");          // continue probing after a true hash collision
    emitter.instruction("add x5, x5, #1");                                      // advance within the collision-safe key compare
    emitter.instruction("b __rt_callable_lookup_string_hash_compare");          // compare the next selector byte
    emitter.label("__rt_callable_lookup_string_hash_next");
    emitter.instruction("add x13, x13, #1");                                    // advance to the next open-address bucket
    emitter.instruction("and x13, x13, x11");                                   // wrap the bucket index by the power-of-two mask
    emitter.instruction("b __rt_callable_lookup_string_hash_probe");            // continue the read-only probe chain
    emitter.label("__rt_callable_lookup_string_hash_match");
    emitter.instruction("ldr x0, [x17, #24]");                                  // return the matched descriptor pointer
    emitter.instruction("ldr x1, [x17, #32]");                                  // return the matched lookup flags
    emitter.instruction("ret");                                                 // return one resolved callable descriptor

    emitter.label("__rt_callable_lookup_string_miss");
    emitter.instruction("mov x0, #0");                                          // return a null descriptor for lookup miss
    emitter.instruction("mov x1, #0");                                          // clear lookup flags on miss
    emitter.instruction("ret");                                                 // return without allocation or diagnostics

    emit_aarch64_composite_lookup(emitter);
}

/// Emits the AArch64 resolver shared by instance, static, and invokable table shapes.
fn emit_aarch64_composite_lookup(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_callable_lookup_composite_linear");
    emitter.instruction("mov x8, x0");                                          // preserve the callable shape kind
    emitter.instruction("mov x9, x1");                                          // preserve class id or class-name pointer
    emitter.instruction("mov x10, x2");                                         // preserve the class-name length
    emitter.instruction("mov x11, x3");                                         // preserve the method-name pointer
    emitter.instruction("mov x12, x4");                                         // preserve the method-name length
    emitter.instruction("ldr x14, [x5]");                                       // load the number of linear entries
    emitter.instruction("add x13, x5, #8");                                     // skip the table header to the first row
    emit_aarch64_normalize_composite(emitter, "__rt_callable_lookup_composite_linear_ready");
    emitter.label("__rt_callable_lookup_composite_linear_ready");
    emit_aarch64_validate_composite(
        emitter,
        "__rt_callable_lookup_composite_linear",
        "__rt_callable_lookup_composite_linear_miss",
    );
    emitter.label("__rt_callable_lookup_composite_linear_loop");
    emitter.instruction("cbz x14, __rt_callable_lookup_composite_linear_miss"); // stop after scanning every composite row
    emitter.instruction("cmp x8, #2");                                          // does this shape use a string class selector?
    emitter.instruction("b.eq __rt_callable_lookup_composite_linear_class_string"); // compare the static class-name component
    emitter.instruction("ldr x0, [x13]");                                       // load the numeric receiver class id
    emitter.instruction("cmp x9, x0");                                          // compare the runtime and table class ids
    emitter.instruction("b.ne __rt_callable_lookup_composite_linear_next");     // advance when numeric class ids differ
    emitter.instruction("cmp x8, #3");                                          // is this an invokable-object lookup?
    emitter.instruction("b.eq __rt_callable_lookup_composite_linear_match");    // class id alone identifies an invokable template
    emitter.instruction("b __rt_callable_lookup_composite_linear_method");      // compare the instance method component
    emitter.label("__rt_callable_lookup_composite_linear_class_string");
    emitter.instruction("ldr x0, [x13, #8]");                                   // load the canonical class-name length
    emitter.instruction("cmp x10, x0");                                         // reject static rows with a different class length
    emitter.instruction("b.ne __rt_callable_lookup_composite_linear_next");     // advance after a static class-length mismatch
    emitter.instruction("ldr x1, [x13]");                                       // load the canonical class-name pointer
    emitter.instruction("mov x2, #0");                                          // start comparing the static class name
    emitter.label("__rt_callable_lookup_composite_linear_class_bytes");
    emitter.instruction("cmp x2, x10");                                         // have all static class bytes matched?
    emitter.instruction("b.hs __rt_callable_lookup_composite_linear_method");   // continue with the method component
    emitter.instruction("ldrb w3, [x9, x2]");                                   // load one runtime class-name byte
    emit_aarch64_lower_ascii(emitter, "w3", "__rt_callable_lookup_composite_linear_class_rhs");
    emitter.label("__rt_callable_lookup_composite_linear_class_rhs");
    emitter.instruction("ldrb w4, [x1, x2]");                                   // load one canonical class-name byte
    emitter.instruction("cmp w3, w4");                                          // compare normalized class-name bytes
    emitter.instruction("b.ne __rt_callable_lookup_composite_linear_next");     // advance after a class-name mismatch
    emitter.instruction("add x2, x2, #1");                                      // advance within the class-name component
    emitter.instruction("b __rt_callable_lookup_composite_linear_class_bytes"); // compare the next class-name byte
    emitter.label("__rt_callable_lookup_composite_linear_method");
    emitter.instruction("ldr x0, [x13, #24]");                                  // load the canonical method-name length
    emitter.instruction("cmp x12, x0");                                         // reject rows with a different method length
    emitter.instruction("b.ne __rt_callable_lookup_composite_linear_next");     // advance after a method-length mismatch
    emitter.instruction("ldr x1, [x13, #16]");                                  // load the canonical method-name pointer
    emitter.instruction("mov x2, #0");                                          // start comparing the method name
    emitter.label("__rt_callable_lookup_composite_linear_method_bytes");
    emitter.instruction("cmp x2, x12");                                         // have all method bytes matched?
    emitter.instruction("b.hs __rt_callable_lookup_composite_linear_match");    // return the resolved descriptor/template
    emitter.instruction("ldrb w3, [x11, x2]");                                  // load one runtime method-name byte
    emit_aarch64_lower_ascii(emitter, "w3", "__rt_callable_lookup_composite_linear_method_rhs");
    emitter.label("__rt_callable_lookup_composite_linear_method_rhs");
    emitter.instruction("ldrb w4, [x1, x2]");                                   // load one canonical method-name byte
    emitter.instruction("cmp w3, w4");                                          // compare normalized method-name bytes
    emitter.instruction("b.ne __rt_callable_lookup_composite_linear_next");     // advance after a method-name mismatch
    emitter.instruction("add x2, x2, #1");                                      // advance within the method-name component
    emitter.instruction("b __rt_callable_lookup_composite_linear_method_bytes"); // compare the next method-name byte
    emitter.label("__rt_callable_lookup_composite_linear_next");
    emitter.instruction("add x13, x13, #48");                                   // advance to the next six-word row
    emitter.instruction("sub x14, x14, #1");                                    // consume one remaining linear row
    emitter.instruction("b __rt_callable_lookup_composite_linear_loop");        // continue the compact linear scan
    emitter.label("__rt_callable_lookup_composite_linear_match");
    emitter.instruction("ldr x0, [x13, #32]");                                  // return the matched descriptor/template pointer
    emitter.instruction("ldr x1, [x13, #40]");                                  // return the matched capture flag
    emitter.instruction("ret");                                                 // return one composite callable result
    // Own miss epilogue: the hash helper below is a separate atom (see the string pair above).
    emitter.label("__rt_callable_lookup_composite_linear_miss");
    emitter.instruction("mov x0, #0");                                          // return a null composite descriptor on miss
    emitter.instruction("mov x1, #0");                                          // clear composite lookup flags on miss
    emitter.instruction("ret");                                                 // return without allocation or diagnostics

    emitter.blank();
    emitter.label_global("__rt_callable_lookup_composite_hash");
    emitter.instruction("mov x8, x0");                                          // preserve the callable shape kind
    emitter.instruction("mov x9, x1");                                          // preserve class id or class-name pointer
    emitter.instruction("mov x10, x2");                                         // preserve the class-name length
    emitter.instruction("mov x11, x3");                                         // preserve the method-name pointer
    emitter.instruction("mov x12, x4");                                         // preserve the method-name length
    emitter.instruction("ldr x14, [x5]");                                       // load the open-addressed capacity mask
    emitter.instruction("add x13, x5, #8");                                     // skip the table header to the first bucket
    emit_aarch64_normalize_composite(emitter, "__rt_callable_lookup_composite_hash_ready");
    emitter.label("__rt_callable_lookup_composite_hash_ready");
    emit_aarch64_validate_composite(
        emitter,
        "__rt_callable_lookup_composite_hash",
        "__rt_callable_lookup_composite_miss",
    );
    abi::emit_load_int_immediate(emitter, "x0", FNV1A_OFFSET as i64);
    abi::emit_load_int_immediate(emitter, "x1", FNV1A_PRIME as i64);
    emitter.instruction("cmp x8, #2");                                          // does the hash begin with class-name bytes?
    emitter.instruction("b.eq __rt_callable_lookup_composite_hash_class_string"); // hash the static class-name component
    emitter.instruction("mov x2, #0");                                          // start hashing the little-endian class id
    emitter.label("__rt_callable_lookup_composite_hash_class_id");
    emitter.instruction("cmp x2, #8");                                          // have all eight class-id bytes been hashed?
    emitter.instruction("b.hs __rt_callable_lookup_composite_hash_after_first"); // continue after the numeric class component
    emitter.instruction("lsl x4, x2, #3");                                      // convert the class-id byte index to a bit shift
    emitter.instruction("lsrv x3, x9, x4");                                     // select one little-endian class-id byte
    emitter.instruction("and x3, x3, #255");                                    // isolate the selected class-id byte
    emitter.instruction("eor x0, x0, x3");                                      // xor the class-id byte into FNV state
    emitter.instruction("mul x0, x0, x1");                                      // multiply by the fixed FNV-1a prime
    emitter.instruction("add x2, x2, #1");                                      // advance to the next class-id byte
    emitter.instruction("b __rt_callable_lookup_composite_hash_class_id");      // hash the remaining class-id bytes
    emitter.label("__rt_callable_lookup_composite_hash_class_string");
    emitter.instruction("mov x2, #0");                                          // start hashing the normalized class name
    emitter.label("__rt_callable_lookup_composite_hash_class_bytes");
    emitter.instruction("cmp x2, x10");                                         // have all class-name bytes been hashed?
    emitter.instruction("b.hs __rt_callable_lookup_composite_hash_after_first"); // continue after the string class component
    emitter.instruction("ldrb w3, [x9, x2]");                                   // load one runtime class-name byte
    emit_aarch64_lower_ascii(emitter, "w3", "__rt_callable_lookup_composite_hash_class_mix");
    emitter.label("__rt_callable_lookup_composite_hash_class_mix");
    emitter.instruction("eor x0, x0, x3");                                      // xor the canonical class byte into FNV state
    emitter.instruction("mul x0, x0, x1");                                      // multiply by the fixed FNV-1a prime
    emitter.instruction("add x2, x2, #1");                                      // advance to the next class-name byte
    emitter.instruction("b __rt_callable_lookup_composite_hash_class_bytes");   // hash the remaining class-name bytes
    emitter.label("__rt_callable_lookup_composite_hash_after_first");
    emitter.instruction("cmp x8, #3");                                          // does an invokable key stop after the class id?
    emitter.instruction("b.eq __rt_callable_lookup_composite_hash_probe_start"); // probe immediately for invokable objects
    emitter.instruction("eor x0, x0, xzr");                                     // xor the zero component separator into FNV state
    emitter.instruction("mul x0, x0, x1");                                      // multiply after the zero component separator
    emitter.instruction("mov x2, #0");                                          // start hashing the normalized method name
    emitter.label("__rt_callable_lookup_composite_hash_method_bytes");
    emitter.instruction("cmp x2, x12");                                         // have all method-name bytes been hashed?
    emitter.instruction("b.hs __rt_callable_lookup_composite_hash_probe_start"); // probe after hashing the complete composite key
    emitter.instruction("ldrb w3, [x11, x2]");                                  // load one runtime method-name byte
    emit_aarch64_lower_ascii(emitter, "w3", "__rt_callable_lookup_composite_hash_method_mix");
    emitter.label("__rt_callable_lookup_composite_hash_method_mix");
    emitter.instruction("eor x0, x0, x3");                                      // xor the canonical method byte into FNV state
    emitter.instruction("mul x0, x0, x1");                                      // multiply by the fixed FNV-1a prime
    emitter.instruction("add x2, x2, #1");                                      // advance to the next method-name byte
    emitter.instruction("b __rt_callable_lookup_composite_hash_method_bytes");  // hash the remaining method-name bytes
    emitter.label("__rt_callable_lookup_composite_hash_probe_start");
    emitter.instruction("mov x7, x0");                                          // preserve the completed composite hash
    emitter.instruction("and x2, x0, x14");                                     // select the initial open-address bucket
    emitter.label("__rt_callable_lookup_composite_hash_probe");
    emitter.instruction("lsl x4, x2, #6");                                      // compute the bucket index times 64
    emitter.instruction("sub x4, x4, x2, lsl #3");                              // convert the bucket offset to seven words
    emitter.instruction("add x3, x13, x4");                                     // address the current open-addressed bucket
    emitter.instruction("ldr x0, [x3, #40]");                                   // load the bucket descriptor/template pointer
    emitter.instruction("cbz x0, __rt_callable_lookup_composite_miss");         // an empty bucket terminates the probe chain
    emitter.instruction("ldr x1, [x3]");                                        // load the bucket hash
    emitter.instruction("cmp x1, x7");                                          // compare the precomputed and runtime hashes
    emitter.instruction("b.ne __rt_callable_lookup_composite_hash_next");       // probe onward when hashes differ
    emitter.instruction("cmp x8, #2");                                          // does collision equality use a class string?
    emitter.instruction("b.eq __rt_callable_lookup_composite_hash_class_equal"); // compare the static class-name component
    emitter.instruction("ldr x1, [x3, #8]");                                    // load the bucket receiver class id
    emitter.instruction("cmp x9, x1");                                          // compare numeric receiver class ids
    emitter.instruction("b.ne __rt_callable_lookup_composite_hash_next");       // probe after a numeric class mismatch
    emitter.instruction("cmp x8, #3");                                          // is class id the complete invokable key?
    emitter.instruction("b.eq __rt_callable_lookup_composite_hash_match");      // return the invokable template after equality
    emitter.instruction("b __rt_callable_lookup_composite_hash_method_equal");  // compare the instance method component
    emitter.label("__rt_callable_lookup_composite_hash_class_equal");
    emitter.instruction("ldr x1, [x3, #16]");                                   // load the canonical class-name length
    emitter.instruction("cmp x10, x1");                                         // reject collisions with a different class length
    emitter.instruction("b.ne __rt_callable_lookup_composite_hash_next");       // probe after a class-length mismatch
    emitter.instruction("ldr x4, [x3, #8]");                                    // load the canonical class-name pointer
    emitter.instruction("mov x5, #0");                                          // start collision-safe class equality
    emitter.label("__rt_callable_lookup_composite_hash_class_compare");
    emitter.instruction("cmp x5, x10");                                         // have all class-name bytes matched?
    emitter.instruction("b.hs __rt_callable_lookup_composite_hash_method_equal"); // continue with method equality
    emitter.instruction("ldrb w6, [x9, x5]");                                   // load one runtime class-name byte
    emit_aarch64_lower_ascii(emitter, "w6", "__rt_callable_lookup_composite_hash_class_rhs");
    emitter.label("__rt_callable_lookup_composite_hash_class_rhs");
    emitter.instruction("ldrb w0, [x4, x5]");                                   // load one canonical class-name byte
    emitter.instruction("cmp w6, w0");                                          // compare normalized class-name bytes
    emitter.instruction("b.ne __rt_callable_lookup_composite_hash_next");       // probe after a true class hash collision
    emitter.instruction("add x5, x5, #1");                                      // advance within class-name equality
    emitter.instruction("b __rt_callable_lookup_composite_hash_class_compare"); // compare the next class-name byte
    emitter.label("__rt_callable_lookup_composite_hash_method_equal");
    emitter.instruction("ldr x1, [x3, #32]");                                   // load the canonical method-name length
    emitter.instruction("cmp x12, x1");                                         // reject collisions with a different method length
    emitter.instruction("b.ne __rt_callable_lookup_composite_hash_next");       // probe after a method-length mismatch
    emitter.instruction("ldr x4, [x3, #24]");                                   // load the canonical method-name pointer
    emitter.instruction("mov x5, #0");                                          // start collision-safe method equality
    emitter.label("__rt_callable_lookup_composite_hash_method_compare");
    emitter.instruction("cmp x5, x12");                                         // have all method-name bytes matched?
    emitter.instruction("b.hs __rt_callable_lookup_composite_hash_match");      // return after complete composite equality
    emitter.instruction("ldrb w6, [x11, x5]");                                  // load one runtime method-name byte
    emit_aarch64_lower_ascii(emitter, "w6", "__rt_callable_lookup_composite_hash_method_rhs");
    emitter.label("__rt_callable_lookup_composite_hash_method_rhs");
    emitter.instruction("ldrb w0, [x4, x5]");                                   // load one canonical method-name byte
    emitter.instruction("cmp w6, w0");                                          // compare normalized method-name bytes
    emitter.instruction("b.ne __rt_callable_lookup_composite_hash_next");       // probe after a true method hash collision
    emitter.instruction("add x5, x5, #1");                                      // advance within method-name equality
    emitter.instruction("b __rt_callable_lookup_composite_hash_method_compare"); // compare the next method-name byte
    emitter.label("__rt_callable_lookup_composite_hash_next");
    emitter.instruction("add x2, x2, #1");                                      // advance to the next open-address bucket
    emitter.instruction("and x2, x2, x14");                                     // wrap the bucket index by the capacity mask
    emitter.instruction("b __rt_callable_lookup_composite_hash_probe");         // continue the collision-safe probe chain
    emitter.label("__rt_callable_lookup_composite_hash_match");
    emitter.instruction("ldr x0, [x3, #40]");                                   // return the matched descriptor/template pointer
    emitter.instruction("ldr x1, [x3, #48]");                                   // return the matched capture flag
    emitter.instruction("ret");                                                 // return one hashed composite callable result

    emitter.label("__rt_callable_lookup_composite_miss");
    emitter.instruction("mov x0, #0");                                          // return a null composite descriptor on miss
    emitter.instruction("mov x1, #0");                                          // clear composite lookup flags on miss
    emitter.instruction("ret");                                                 // return without allocation or diagnostics
}

/// Normalizes pointer components for the AArch64 composite callable ABI.
fn emit_aarch64_normalize_composite(emitter: &mut Emitter, ready_label: &str) {
    let first_ready = format!("{}_first", ready_label);
    emitter.instruction("cmp x8, #2");                                          // only static callables have a string first component
    emitter.instruction(&format!("b.ne {}", first_ready));                      // skip numeric class-id normalization
    emitter.instruction(&format!("cbz x10, {}", first_ready));                  // keep an empty class selector unchanged
    emitter.instruction("ldrb w0, [x9]");                                       // inspect the first class selector byte
    emitter.instruction("cmp w0, #92");                                         // is the class selector prefixed by a separator?
    emitter.instruction(&format!("b.ne {}", first_ready));                      // keep an unprefixed class selector unchanged
    emitter.instruction("add x9, x9, #1");                                      // remove exactly one leading class separator
    emitter.instruction("sub x10, x10, #1");                                    // shorten the normalized class selector
    emitter.label(&first_ready);
    emitter.instruction("cmp x8, #3");                                          // invokable keys have no method component
    emitter.instruction(&format!("b.eq {}", ready_label));                      // finish normalization for invokable objects
    emitter.instruction(&format!("cbz x12, {}", ready_label));                  // keep an empty method selector unchanged
    emitter.instruction("ldrb w0, [x11]");                                      // inspect the first method selector byte
    emitter.instruction("cmp w0, #92");                                         // is the method selector prefixed by a separator?
    emitter.instruction(&format!("b.ne {}", ready_label));                      // keep an unprefixed method selector unchanged
    emitter.instruction("add x11, x11, #1");                                    // remove exactly one leading method separator
    emitter.instruction("sub x12, x12, #1");                                    // shorten the normalized method selector
}

/// Rejects NUL bytes in AArch64 composite selector strings before lookup.
fn emit_aarch64_validate_composite(emitter: &mut Emitter, scope: &str, miss_label: &str) {
    let first_loop = format!("{}_validate_first", scope);
    let first_done = format!("{}_validate_first_done", scope);
    let second_loop = format!("{}_validate_second", scope);
    let second_done = format!("{}_validate_second_done", scope);
    emitter.instruction("cmp x8, #2");                                          // does the first component contain class-name bytes?
    emitter.instruction(&format!("b.ne {}", first_done));                       // skip byte validation for numeric class ids
    emitter.instruction("mov x0, #0");                                          // start validating the class-name bytes
    emitter.label(&first_loop);
    emitter.instruction("cmp x0, x10");                                         // have all class-name bytes been checked?
    emitter.instruction(&format!("b.hs {}", first_done));                       // continue after validating the class name
    emitter.instruction("ldrb w1, [x9, x0]");                                   // load one class-name byte for NUL validation
    emitter.instruction(&format!("cbz w1, {}", miss_label));                    // embedded NUL makes the selector a miss
    emitter.instruction("add x0, x0, #1");                                      // advance to the next class-name byte
    emitter.instruction(&format!("b {}", first_loop));                          // validate the remaining class-name bytes
    emitter.label(&first_done);
    emitter.instruction("cmp x8, #3");                                          // does this shape omit a method component?
    emitter.instruction(&format!("b.eq {}", second_done));                      // invokable keys need no second validation
    emitter.instruction("mov x0, #0");                                          // start validating the method-name bytes
    emitter.label(&second_loop);
    emitter.instruction("cmp x0, x12");                                         // have all method-name bytes been checked?
    emitter.instruction(&format!("b.hs {}", second_done));                      // continue after validating the method name
    emitter.instruction("ldrb w1, [x11, x0]");                                  // load one method-name byte for NUL validation
    emitter.instruction(&format!("cbz w1, {}", miss_label));                    // embedded NUL makes the selector a miss
    emitter.instruction("add x0, x0, #1");                                      // advance to the next method-name byte
    emitter.instruction(&format!("b {}", second_loop));                         // validate the remaining method-name bytes
    emitter.label(&second_done);
}

/// Normalizes the AArch64 selector pointer and length by removing at most one leading backslash.
fn emit_aarch64_normalize_selector(emitter: &mut Emitter, ready_label: &str) {
    emitter.instruction(&format!("cbz x1, {}", ready_label));                   // empty selectors have no leading separator to remove
    emitter.instruction("ldrb w4, [x0]");                                       // inspect the first selector byte
    emitter.instruction("cmp w4, #92");                                         // is the first byte a namespace separator?
    emitter.instruction(&format!("b.ne {}", ready_label));                      // keep selectors without a leading separator unchanged
    emitter.instruction("add x0, x0, #1");                                      // remove exactly one leading namespace separator
    emitter.instruction("sub x1, x1, #1");                                      // shorten the normalized selector by one byte
}

/// Emits an AArch64 ASCII-uppercase fold and branches to `done_label` when no fold is needed.
fn emit_aarch64_lower_ascii(emitter: &mut Emitter, byte_reg: &str, done_label: &str) {
    emitter.instruction(&format!("cmp {}, #65", byte_reg));                     // test whether the byte is below uppercase ASCII A
    emitter.instruction(&format!("b.lt {}", done_label));                       // preserve bytes below the uppercase ASCII range
    emitter.instruction(&format!("cmp {}, #90", byte_reg));                     // test whether the byte is above uppercase ASCII Z
    emitter.instruction(&format!("b.gt {}", done_label));                       // preserve bytes above the uppercase ASCII range
    emitter.instruction(&format!("add {}, {}, #32", byte_reg, byte_reg));       // fold uppercase ASCII to its lowercase byte
}

/// Emits the x86-64 callable-name lookup helper bodies.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: callable lookup tables ---");
    emitter.label_global("__rt_callable_lookup_string_linear");
    emit_x86_64_normalize_selector(emitter, "__rt_callable_lookup_string_linear_ready");
    emitter.label("__rt_callable_lookup_string_linear_ready");
    emitter.instruction("mov r8, rdi");                                         // preserve the normalized selector pointer
    emitter.instruction("mov r9, rsi");                                         // preserve the normalized selector length
    emitter.instruction("mov r10, rdx");                                        // preserve the linear table base
    emitter.instruction("mov r11, rcx");                                        // preserve the number of linear entries
    emitter.label("__rt_callable_lookup_string_linear_loop");
    emitter.instruction("test r11, r11");                                       // have all canonical callable rows been scanned?
    emitter.instruction("je __rt_callable_lookup_string_linear_miss");          // return a miss after exhausting the linear table
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // rdi = canonical candidate-name pointer
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // rax = canonical candidate-name length
    emitter.instruction("cmp r9, rax");                                         // reject candidates with a different normalized length
    emitter.instruction("jne __rt_callable_lookup_string_linear_next");         // advance when the candidate length differs
    emitter.instruction("xor ecx, ecx");                                        // rcx = byte index within the candidate name
    emitter.label("__rt_callable_lookup_string_linear_bytes");
    emitter.instruction("cmp rcx, r9");                                         // have all normalized selector bytes matched?
    emitter.instruction("jae __rt_callable_lookup_string_linear_match");        // return this descriptor after a complete match
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load one runtime selector byte
    emit_x86_64_lower_ascii(emitter, "al", "__rt_callable_lookup_string_linear_rhs");
    emitter.label("__rt_callable_lookup_string_linear_rhs");
    emitter.instruction("cmp al, BYTE PTR [rdi + rcx]");                        // compare runtime and canonical bytes
    emitter.instruction("jne __rt_callable_lookup_string_linear_next");         // advance when this candidate byte differs
    emitter.instruction("add rcx, 1");                                          // advance within the current candidate name
    emitter.instruction("jmp __rt_callable_lookup_string_linear_bytes");        // compare the next selector byte
    emitter.label("__rt_callable_lookup_string_linear_next");
    emitter.instruction("add r10, 32");                                         // advance to the next four-word linear entry
    emitter.instruction("sub r11, 1");                                          // consume one remaining linear entry
    emitter.instruction("jmp __rt_callable_lookup_string_linear_loop");         // continue scanning canonical entries
    emitter.label("__rt_callable_lookup_string_linear_match");
    emitter.instruction("mov rax, QWORD PTR [r10 + 16]");                       // return the matched descriptor pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 24]");                       // return the matched lookup flags
    emitter.instruction("ret");                                                 // return one resolved callable descriptor
    // Own miss epilogue: `label_global` opens a fresh `.text.<name>` section here, so the hash
    // helper's internal miss label is in a different `--gc-sections` unit.
    emitter.label("__rt_callable_lookup_string_linear_miss");
    emitter.instruction("xor eax, eax");                                        // return a null descriptor for lookup miss
    emitter.instruction("xor edx, edx");                                        // clear lookup flags on miss
    emitter.instruction("ret");                                                 // return without allocation or diagnostics

    emitter.blank();
    emitter.label_global("__rt_callable_lookup_string_hash");
    emit_x86_64_normalize_selector(emitter, "__rt_callable_lookup_string_hash_ready");
    emitter.label("__rt_callable_lookup_string_hash_ready");
    emitter.instruction("push rbx");                                            // preserve the callee-saved byte-index register
    emitter.instruction("mov r8, rdi");                                         // preserve the normalized selector pointer
    emitter.instruction("mov r9, rsi");                                         // preserve the normalized selector length
    emitter.instruction("mov r10, rdx");                                        // preserve the open-addressed table base
    emitter.instruction("mov r11, rcx");                                        // preserve the table capacity mask
    emitter.instruction(&format!("mov rax, 0x{:016x}", FNV1A_OFFSET));          // initialize the fixed FNV-1a hash state
    emitter.instruction("xor ebx, ebx");                                        // rbx = selector byte index for FNV-1a
    emitter.label("__rt_callable_lookup_string_hash_bytes");
    emitter.instruction("cmp rbx, r9");                                         // have all selector bytes been hashed?
    emitter.instruction("jae __rt_callable_lookup_string_hash_probe_start");    // probe the table after hashing the full selector
    emitter.instruction("movzx ecx, BYTE PTR [r8 + rbx]");                      // load one runtime selector byte for hashing
    emit_x86_64_lower_ascii(emitter, "cl", "__rt_callable_lookup_string_hash_mix");
    emitter.label("__rt_callable_lookup_string_hash_mix");
    emitter.instruction("xor rax, rcx");                                        // xor the canonical selector byte into FNV state
    emitter.instruction(&format!("mov rcx, 0x{:016x}", FNV1A_PRIME));           // load the fixed FNV-1a prime
    emitter.instruction("imul rax, rcx");                                       // multiply the FNV state with 64-bit wrapping
    emitter.instruction("add rbx, 1");                                          // advance to the next selector byte
    emitter.instruction("jmp __rt_callable_lookup_string_hash_bytes");          // continue hashing the normalized selector
    emitter.label("__rt_callable_lookup_string_hash_probe_start");
    emitter.instruction("mov rsi, rax");                                        // preserve the completed selector hash
    emitter.instruction("and rax, r11");                                        // select the initial open-address bucket
    emitter.instruction("mov rdi, rax");                                        // rdi = current bucket index
    emitter.label("__rt_callable_lookup_string_hash_probe");
    emitter.instruction("imul rax, rdi, 40");                                   // compute the five-word bucket byte offset
    emitter.instruction("add rax, r10");                                        // rax = current bucket address
    emitter.instruction("mov rdx, QWORD PTR [rax + 24]");                       // load the bucket descriptor pointer
    emitter.instruction("test rdx, rdx");                                       // is this open-address bucket empty?
    emitter.instruction("je __rt_callable_lookup_string_hash_miss");            // an empty bucket terminates this probe chain
    emitter.instruction("cmp rsi, QWORD PTR [rax]");                            // compare the precomputed and runtime hashes
    emitter.instruction("jne __rt_callable_lookup_string_hash_next");           // probe the next bucket when hashes differ
    emitter.instruction("cmp r9, QWORD PTR [rax + 16]");                        // reject hash collisions with a different length
    emitter.instruction("jne __rt_callable_lookup_string_hash_next");           // probe onward after a length mismatch
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // rdx = canonical candidate-name pointer
    emitter.instruction("xor ebx, ebx");                                        // rbx = byte index for collision-safe equality
    emitter.label("__rt_callable_lookup_string_hash_compare");
    emitter.instruction("cmp rbx, r9");                                         // have all bytes in this hash collision matched?
    emitter.instruction("jae __rt_callable_lookup_string_hash_match");          // return the descriptor after full equality
    emitter.instruction("movzx ecx, BYTE PTR [r8 + rbx]");                      // load one runtime selector byte
    emit_x86_64_lower_ascii(emitter, "cl", "__rt_callable_lookup_string_hash_rhs");
    emitter.label("__rt_callable_lookup_string_hash_rhs");
    emitter.instruction("cmp cl, BYTE PTR [rdx + rbx]");                        // compare normalized bytes after the hash match
    emitter.instruction("jne __rt_callable_lookup_string_hash_next");           // continue probing after a true hash collision
    emitter.instruction("add rbx, 1");                                          // advance within the collision-safe key compare
    emitter.instruction("jmp __rt_callable_lookup_string_hash_compare");        // compare the next selector byte
    emitter.label("__rt_callable_lookup_string_hash_next");
    emitter.instruction("add rdi, 1");                                          // advance to the next open-address bucket
    emitter.instruction("and rdi, r11");                                        // wrap the bucket index by the power-of-two mask
    emitter.instruction("jmp __rt_callable_lookup_string_hash_probe");          // continue the read-only probe chain
    emitter.label("__rt_callable_lookup_string_hash_match");
    emitter.instruction("mov rdx, QWORD PTR [rax + 32]");                       // return the matched lookup flags
    emitter.instruction("mov rax, QWORD PTR [rax + 24]");                       // return the matched descriptor pointer
    emitter.instruction("pop rbx");                                             // restore the callee-saved byte-index register
    emitter.instruction("ret");                                                 // return one resolved callable descriptor
    emitter.label("__rt_callable_lookup_string_hash_miss");
    emitter.instruction("pop rbx");                                             // restore the callee-saved byte-index register on miss
    emitter.instruction("jmp __rt_callable_lookup_string_miss");                // return the shared null result

    emitter.label("__rt_callable_lookup_string_miss");
    emitter.instruction("xor eax, eax");                                        // return a null descriptor for lookup miss
    emitter.instruction("xor edx, edx");                                        // clear lookup flags on miss
    emitter.instruction("ret");                                                 // return without allocation or diagnostics

    emit_x86_64_composite_lookup(emitter);
}

/// Emits the x86-64 resolver shared by instance, static, and invokable table shapes.
fn emit_x86_64_composite_lookup(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_callable_lookup_composite_linear");
    emit_x86_64_save_composite_inputs(emitter);
    emit_x86_64_normalize_composite(emitter, "__rt_callable_lookup_composite_linear_ready");
    emitter.label("__rt_callable_lookup_composite_linear_ready");
    emit_x86_64_validate_composite(
        emitter,
        "__rt_callable_lookup_composite_linear",
        "__rt_callable_lookup_composite_linear_miss",
    );
    emitter.label("__rt_callable_lookup_composite_linear_loop");
    emitter.instruction("test r15, r15");                                       // have all composite rows been scanned?
    emitter.instruction("je __rt_callable_lookup_composite_linear_miss");       // return a miss after exhausting the linear table
    emitter.instruction("cmp rbx, 2");                                          // does this shape use a string class selector?
    emitter.instruction("je __rt_callable_lookup_composite_linear_class_string"); // compare the static class-name component
    emitter.instruction("cmp r10, QWORD PTR [r14]");                            // compare the runtime and table class ids
    emitter.instruction("jne __rt_callable_lookup_composite_linear_next");      // advance when numeric class ids differ
    emitter.instruction("cmp rbx, 3");                                          // is this an invokable-object lookup?
    emitter.instruction("je __rt_callable_lookup_composite_linear_match");      // class id alone identifies an invokable template
    emitter.instruction("jmp __rt_callable_lookup_composite_linear_method");    // compare the instance method component
    emitter.label("__rt_callable_lookup_composite_linear_class_string");
    emitter.instruction("cmp r11, QWORD PTR [r14 + 8]");                        // reject static rows with a different class length
    emitter.instruction("jne __rt_callable_lookup_composite_linear_next");      // advance after a static class-length mismatch
    emitter.instruction("mov rsi, QWORD PTR [r14]");                            // load the canonical class-name pointer
    emitter.instruction("xor eax, eax");                                        // start comparing the static class name
    emitter.label("__rt_callable_lookup_composite_linear_class_bytes");
    emitter.instruction("cmp rax, r11");                                        // have all static class bytes matched?
    emitter.instruction("jae __rt_callable_lookup_composite_linear_method");    // continue with the method component
    emitter.instruction("movzx edx, BYTE PTR [r10 + rax]");                     // load one runtime class-name byte
    emit_x86_64_lower_ascii(emitter, "dl", "__rt_callable_lookup_composite_linear_class_rhs");
    emitter.label("__rt_callable_lookup_composite_linear_class_rhs");
    emitter.instruction("cmp dl, BYTE PTR [rsi + rax]");                        // compare normalized class-name bytes
    emitter.instruction("jne __rt_callable_lookup_composite_linear_next");      // advance after a class-name mismatch
    emitter.instruction("add rax, 1");                                          // advance within the class-name component
    emitter.instruction("jmp __rt_callable_lookup_composite_linear_class_bytes"); // compare the next class-name byte
    emitter.label("__rt_callable_lookup_composite_linear_method");
    emitter.instruction("cmp r13, QWORD PTR [r14 + 24]");                       // reject rows with a different method length
    emitter.instruction("jne __rt_callable_lookup_composite_linear_next");      // advance after a method-length mismatch
    emitter.instruction("mov rsi, QWORD PTR [r14 + 16]");                       // load the canonical method-name pointer
    emitter.instruction("xor eax, eax");                                        // start comparing the method name
    emitter.label("__rt_callable_lookup_composite_linear_method_bytes");
    emitter.instruction("cmp rax, r13");                                        // have all method bytes matched?
    emitter.instruction("jae __rt_callable_lookup_composite_linear_match");     // return the resolved descriptor/template
    emitter.instruction("movzx edx, BYTE PTR [r12 + rax]");                     // load one runtime method-name byte
    emit_x86_64_lower_ascii(emitter, "dl", "__rt_callable_lookup_composite_linear_method_rhs");
    emitter.label("__rt_callable_lookup_composite_linear_method_rhs");
    emitter.instruction("cmp dl, BYTE PTR [rsi + rax]");                        // compare normalized method-name bytes
    emitter.instruction("jne __rt_callable_lookup_composite_linear_next");      // advance after a method-name mismatch
    emitter.instruction("add rax, 1");                                          // advance within the method-name component
    emitter.instruction("jmp __rt_callable_lookup_composite_linear_method_bytes"); // compare the next method-name byte
    emitter.label("__rt_callable_lookup_composite_linear_next");
    emitter.instruction("add r14, 48");                                         // advance to the next six-word row
    emitter.instruction("sub r15, 1");                                          // consume one remaining linear row
    emitter.instruction("jmp __rt_callable_lookup_composite_linear_loop");      // continue the compact linear scan
    emitter.label("__rt_callable_lookup_composite_linear_match");
    emitter.instruction("mov rax, QWORD PTR [r14 + 32]");                       // return the matched descriptor/template pointer
    emitter.instruction("mov rdx, QWORD PTR [r14 + 40]");                       // return the matched capture flag
    emitter.instruction("jmp __rt_callable_lookup_composite_linear_return");    // restore preserved registers and return
    // Own miss AND return epilogues: the hash helper below opens its own `.text.<name>` section,
    // so both of its internal labels are in a different `--gc-sections` unit.
    emitter.label("__rt_callable_lookup_composite_linear_miss");
    emitter.instruction("xor eax, eax");                                        // return a null composite descriptor on miss
    emitter.instruction("xor edx, edx");                                        // clear composite lookup flags on miss
    emitter.label("__rt_callable_lookup_composite_linear_return");
    emitter.instruction("pop r15");                                             // restore the fifth preserved lookup register
    emitter.instruction("pop r14");                                             // restore the fourth preserved lookup register
    emitter.instruction("pop r13");                                             // restore the third preserved lookup register
    emitter.instruction("pop r12");                                             // restore the second preserved lookup register
    emitter.instruction("pop rbx");                                             // restore the first preserved lookup register
    emitter.instruction("ret");                                                 // return without allocation or diagnostics

    emitter.blank();
    emitter.label_global("__rt_callable_lookup_composite_hash");
    emit_x86_64_save_composite_inputs(emitter);
    emit_x86_64_normalize_composite(emitter, "__rt_callable_lookup_composite_hash_ready");
    emitter.label("__rt_callable_lookup_composite_hash_ready");
    emit_x86_64_validate_composite(
        emitter,
        "__rt_callable_lookup_composite_hash",
        "__rt_callable_lookup_composite_miss",
    );
    emitter.instruction(&format!("mov rax, 0x{:016x}", FNV1A_OFFSET));          // initialize the fixed FNV-1a hash state
    emitter.instruction(&format!("mov rsi, 0x{:016x}", FNV1A_PRIME));           // load the fixed FNV-1a prime
    emitter.instruction("cmp rbx, 2");                                          // does the hash begin with class-name bytes?
    emitter.instruction("je __rt_callable_lookup_composite_hash_class_string"); // hash the static class-name component
    for shift in (0..64).step_by(8) {
        emitter.instruction("mov rdi, r10");                                    // copy the runtime class id for byte extraction
        if shift != 0 {
            emitter.instruction(&format!("shr rdi, {}", shift));                // select the next little-endian class-id byte
        }
        emitter.instruction("and rdi, 255");                                    // isolate the selected class-id byte
        emitter.instruction("xor rax, rdi");                                    // xor the class-id byte into FNV state
        emitter.instruction("imul rax, rsi");                                   // multiply by the fixed FNV-1a prime
    }
    emitter.instruction("jmp __rt_callable_lookup_composite_hash_after_first"); // continue after the numeric class component
    emitter.label("__rt_callable_lookup_composite_hash_class_string");
    emitter.instruction("xor ecx, ecx");                                        // start hashing the normalized class name
    emitter.label("__rt_callable_lookup_composite_hash_class_bytes");
    emitter.instruction("cmp rcx, r11");                                        // have all class-name bytes been hashed?
    emitter.instruction("jae __rt_callable_lookup_composite_hash_after_first"); // continue after the string class component
    emitter.instruction("movzx edi, BYTE PTR [r10 + rcx]");                     // load one runtime class-name byte
    emit_x86_64_lower_ascii(emitter, "dil", "__rt_callable_lookup_composite_hash_class_mix");
    emitter.label("__rt_callable_lookup_composite_hash_class_mix");
    emitter.instruction("xor rax, rdi");                                        // xor the canonical class byte into FNV state
    emitter.instruction("imul rax, rsi");                                       // multiply by the fixed FNV-1a prime
    emitter.instruction("add rcx, 1");                                          // advance to the next class-name byte
    emitter.instruction("jmp __rt_callable_lookup_composite_hash_class_bytes"); // hash the remaining class-name bytes
    emitter.label("__rt_callable_lookup_composite_hash_after_first");
    emitter.instruction("cmp rbx, 3");                                          // does an invokable key stop after the class id?
    emitter.instruction("je __rt_callable_lookup_composite_hash_probe_start");  // probe immediately for invokable objects
    emitter.instruction("imul rax, rsi");                                       // hash the zero component separator
    emitter.instruction("xor ecx, ecx");                                        // start hashing the normalized method name
    emitter.label("__rt_callable_lookup_composite_hash_method_bytes");
    emitter.instruction("cmp rcx, r13");                                        // have all method-name bytes been hashed?
    emitter.instruction("jae __rt_callable_lookup_composite_hash_probe_start"); // probe after hashing the complete composite key
    emitter.instruction("movzx edi, BYTE PTR [r12 + rcx]");                     // load one runtime method-name byte
    emit_x86_64_lower_ascii(emitter, "dil", "__rt_callable_lookup_composite_hash_method_mix");
    emitter.label("__rt_callable_lookup_composite_hash_method_mix");
    emitter.instruction("xor rax, rdi");                                        // xor the canonical method byte into FNV state
    emitter.instruction("imul rax, rsi");                                       // multiply by the fixed FNV-1a prime
    emitter.instruction("add rcx, 1");                                          // advance to the next method-name byte
    emitter.instruction("jmp __rt_callable_lookup_composite_hash_method_bytes"); // hash the remaining method-name bytes
    emitter.label("__rt_callable_lookup_composite_hash_probe_start");
    emitter.instruction("mov r9, rax");                                         // preserve the completed composite hash
    emitter.instruction("and rax, r15");                                        // select the initial open-address bucket
    emitter.instruction("mov rcx, rax");                                        // preserve the current bucket index
    emitter.label("__rt_callable_lookup_composite_hash_probe");
    emitter.instruction("imul rax, rcx, 56");                                   // compute the seven-word bucket byte offset
    emitter.instruction("add rax, r14");                                        // address the current open-addressed bucket
    emitter.instruction("mov rdx, QWORD PTR [rax + 40]");                       // load the bucket descriptor/template pointer
    emitter.instruction("test rdx, rdx");                                       // is this open-addressed bucket empty?
    emitter.instruction("je __rt_callable_lookup_composite_miss");              // an empty bucket terminates the probe chain
    emitter.instruction("cmp r9, QWORD PTR [rax]");                             // compare the precomputed and runtime hashes
    emitter.instruction("jne __rt_callable_lookup_composite_hash_next");        // probe onward when hashes differ
    emitter.instruction("cmp rbx, 2");                                          // does collision equality use a class string?
    emitter.instruction("je __rt_callable_lookup_composite_hash_class_equal");  // compare the static class-name component
    emitter.instruction("cmp r10, QWORD PTR [rax + 8]");                        // compare numeric receiver class ids
    emitter.instruction("jne __rt_callable_lookup_composite_hash_next");        // probe after a numeric class mismatch
    emitter.instruction("cmp rbx, 3");                                          // is class id the complete invokable key?
    emitter.instruction("je __rt_callable_lookup_composite_hash_match");        // return the invokable template after equality
    emitter.instruction("jmp __rt_callable_lookup_composite_hash_method_equal"); // compare the instance method component
    emitter.label("__rt_callable_lookup_composite_hash_class_equal");
    emitter.instruction("cmp r11, QWORD PTR [rax + 16]");                       // reject collisions with a different class length
    emitter.instruction("jne __rt_callable_lookup_composite_hash_next");        // probe after a class-length mismatch
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // load the canonical class-name pointer
    emitter.instruction("xor edx, edx");                                        // start collision-safe class equality
    emitter.label("__rt_callable_lookup_composite_hash_class_compare");
    emitter.instruction("cmp rdx, r11");                                        // have all class-name bytes matched?
    emitter.instruction("jae __rt_callable_lookup_composite_hash_method_equal"); // continue with method equality
    emitter.instruction("movzx edi, BYTE PTR [r10 + rdx]");                     // load one runtime class-name byte
    emit_x86_64_lower_ascii(emitter, "dil", "__rt_callable_lookup_composite_hash_class_rhs");
    emitter.label("__rt_callable_lookup_composite_hash_class_rhs");
    emitter.instruction("cmp dil, BYTE PTR [rsi + rdx]");                       // compare normalized class-name bytes
    emitter.instruction("jne __rt_callable_lookup_composite_hash_next");        // probe after a true class hash collision
    emitter.instruction("add rdx, 1");                                          // advance within class-name equality
    emitter.instruction("jmp __rt_callable_lookup_composite_hash_class_compare"); // compare the next class-name byte
    emitter.label("__rt_callable_lookup_composite_hash_method_equal");
    emitter.instruction("cmp r13, QWORD PTR [rax + 32]");                       // reject collisions with a different method length
    emitter.instruction("jne __rt_callable_lookup_composite_hash_next");        // probe after a method-length mismatch
    emitter.instruction("mov rsi, QWORD PTR [rax + 24]");                       // load the canonical method-name pointer
    emitter.instruction("xor edx, edx");                                        // start collision-safe method equality
    emitter.label("__rt_callable_lookup_composite_hash_method_compare");
    emitter.instruction("cmp rdx, r13");                                        // have all method-name bytes matched?
    emitter.instruction("jae __rt_callable_lookup_composite_hash_match");       // return after complete composite equality
    emitter.instruction("movzx edi, BYTE PTR [r12 + rdx]");                     // load one runtime method-name byte
    emit_x86_64_lower_ascii(emitter, "dil", "__rt_callable_lookup_composite_hash_method_rhs");
    emitter.label("__rt_callable_lookup_composite_hash_method_rhs");
    emitter.instruction("cmp dil, BYTE PTR [rsi + rdx]");                       // compare normalized method-name bytes
    emitter.instruction("jne __rt_callable_lookup_composite_hash_next");        // probe after a true method hash collision
    emitter.instruction("add rdx, 1");                                          // advance within method-name equality
    emitter.instruction("jmp __rt_callable_lookup_composite_hash_method_compare"); // compare the next method-name byte
    emitter.label("__rt_callable_lookup_composite_hash_next");
    emitter.instruction("add rcx, 1");                                          // advance to the next open-address bucket
    emitter.instruction("and rcx, r15");                                        // wrap the bucket index by the capacity mask
    emitter.instruction("jmp __rt_callable_lookup_composite_hash_probe");       // continue the collision-safe probe chain
    emitter.label("__rt_callable_lookup_composite_hash_match");
    emitter.instruction("mov rdx, QWORD PTR [rax + 48]");                       // return the matched capture flag
    emitter.instruction("mov rax, QWORD PTR [rax + 40]");                       // return the matched descriptor/template pointer
    emitter.instruction("jmp __rt_callable_lookup_composite_return");           // restore preserved registers and return

    emitter.label("__rt_callable_lookup_composite_miss");
    emitter.instruction("xor eax, eax");                                        // return a null composite descriptor on miss
    emitter.instruction("xor edx, edx");                                        // clear composite lookup flags on miss
    emitter.label("__rt_callable_lookup_composite_return");
    emitter.instruction("pop r15");                                             // restore the fifth preserved lookup register
    emitter.instruction("pop r14");                                             // restore the fourth preserved lookup register
    emitter.instruction("pop r13");                                             // restore the third preserved lookup register
    emitter.instruction("pop r12");                                             // restore the second preserved lookup register
    emitter.instruction("pop rbx");                                             // restore the first preserved lookup register
    emitter.instruction("ret");                                                 // return without allocation or diagnostics
}

/// Saves x86-64 composite ABI inputs and loads the table header.
fn emit_x86_64_save_composite_inputs(emitter: &mut Emitter) {
    emitter.instruction("push rbx");                                            // preserve the callable shape register
    emitter.instruction("push r12");                                            // preserve the method pointer register
    emitter.instruction("push r13");                                            // preserve the method length register
    emitter.instruction("push r14");                                            // preserve the table cursor register
    emitter.instruction("push r15");                                            // preserve the table bound register
    emitter.instruction("mov rbx, rdi");                                        // preserve the callable shape kind
    emitter.instruction("mov r10, rsi");                                        // preserve class id or class-name pointer
    emitter.instruction("mov r11, rdx");                                        // preserve the class-name length
    emitter.instruction("mov r12, rcx");                                        // preserve the method-name pointer
    emitter.instruction("mov r13, r8");                                         // preserve the method-name length
    emitter.instruction("mov r14, r9");                                         // preserve the metadata table header
    emitter.instruction("mov r15, QWORD PTR [r14]");                            // load the row count or capacity mask
    emitter.instruction("add r14, 8");                                          // skip the table header to its rows
}

/// Normalizes pointer components for the x86-64 composite callable ABI.
fn emit_x86_64_normalize_composite(emitter: &mut Emitter, ready_label: &str) {
    let first_ready = format!("{}_first", ready_label);
    emitter.instruction("cmp rbx, 2");                                          // only static callables have a string first component
    emitter.instruction(&format!("jne {}", first_ready));                       // skip numeric class-id normalization
    emitter.instruction("test r11, r11");                                       // is the class selector empty?
    emitter.instruction(&format!("je {}", first_ready));                        // keep an empty class selector unchanged
    emitter.instruction("cmp BYTE PTR [r10], 92");                              // is the class selector prefixed by a separator?
    emitter.instruction(&format!("jne {}", first_ready));                       // keep an unprefixed class selector unchanged
    emitter.instruction("add r10, 1");                                          // remove exactly one leading class separator
    emitter.instruction("sub r11, 1");                                          // shorten the normalized class selector
    emitter.label(&first_ready);
    emitter.instruction("cmp rbx, 3");                                          // invokable keys have no method component
    emitter.instruction(&format!("je {}", ready_label));                        // finish normalization for invokable objects
    emitter.instruction("test r13, r13");                                       // is the method selector empty?
    emitter.instruction(&format!("je {}", ready_label));                        // keep an empty method selector unchanged
    emitter.instruction("cmp BYTE PTR [r12], 92");                              // is the method selector prefixed by a separator?
    emitter.instruction(&format!("jne {}", ready_label));                       // keep an unprefixed method selector unchanged
    emitter.instruction("add r12, 1");                                          // remove exactly one leading method separator
    emitter.instruction("sub r13, 1");                                          // shorten the normalized method selector
}

/// Rejects NUL bytes in x86-64 composite selector strings before lookup.
fn emit_x86_64_validate_composite(emitter: &mut Emitter, scope: &str, miss_label: &str) {
    let first_loop = format!("{}_validate_first", scope);
    let first_done = format!("{}_validate_first_done", scope);
    let second_loop = format!("{}_validate_second", scope);
    let second_done = format!("{}_validate_second_done", scope);
    emitter.instruction("cmp rbx, 2");                                          // does the first component contain class-name bytes?
    emitter.instruction(&format!("jne {}", first_done));                        // skip byte validation for numeric class ids
    emitter.instruction("xor eax, eax");                                        // start validating the class-name bytes
    emitter.label(&first_loop);
    emitter.instruction("cmp rax, r11");                                        // have all class-name bytes been checked?
    emitter.instruction(&format!("jae {}", first_done));                        // continue after validating the class name
    emitter.instruction("cmp BYTE PTR [r10 + rax], 0");                         // inspect one class-name byte for NUL
    emitter.instruction(&format!("je {}", miss_label));                         // embedded NUL makes the selector a miss
    emitter.instruction("add rax, 1");                                          // advance to the next class-name byte
    emitter.instruction(&format!("jmp {}", first_loop));                        // validate the remaining class-name bytes
    emitter.label(&first_done);
    emitter.instruction("cmp rbx, 3");                                          // does this shape omit a method component?
    emitter.instruction(&format!("je {}", second_done));                        // invokable keys need no second validation
    emitter.instruction("xor eax, eax");                                        // start validating the method-name bytes
    emitter.label(&second_loop);
    emitter.instruction("cmp rax, r13");                                        // have all method-name bytes been checked?
    emitter.instruction(&format!("jae {}", second_done));                       // continue after validating the method name
    emitter.instruction("cmp BYTE PTR [r12 + rax], 0");                         // inspect one method-name byte for NUL
    emitter.instruction(&format!("je {}", miss_label));                         // embedded NUL makes the selector a miss
    emitter.instruction("add rax, 1");                                          // advance to the next method-name byte
    emitter.instruction(&format!("jmp {}", second_loop));                       // validate the remaining method-name bytes
    emitter.label(&second_done);
}

/// Normalizes the x86-64 selector pointer and length by removing at most one leading backslash.
fn emit_x86_64_normalize_selector(emitter: &mut Emitter, ready_label: &str) {
    emitter.instruction("test rsi, rsi");                                       // empty selectors have no leading separator to remove
    emitter.instruction(&format!("je {}", ready_label));                        // keep an empty selector unchanged
    emitter.instruction("cmp BYTE PTR [rdi], 92");                              // is the first byte a namespace separator?
    emitter.instruction(&format!("jne {}", ready_label));                       // keep selectors without a leading separator unchanged
    emitter.instruction("add rdi, 1");                                          // remove exactly one leading namespace separator
    emitter.instruction("sub rsi, 1");                                          // shorten the normalized selector by one byte
}

/// Emits an x86-64 ASCII-uppercase fold and branches to `done_label` when no fold is needed.
fn emit_x86_64_lower_ascii(emitter: &mut Emitter, byte_reg: &str, done_label: &str) {
    emitter.instruction(&format!("cmp {}, 65", byte_reg));                      // test whether the byte is below uppercase ASCII A
    emitter.instruction(&format!("jb {}", done_label));                         // preserve bytes below the uppercase ASCII range
    emitter.instruction(&format!("cmp {}, 90", byte_reg));                      // test whether the byte is above uppercase ASCII Z
    emitter.instruction(&format!("ja {}", done_label));                         // preserve bytes above the uppercase ASCII range
    emitter.instruction(&format!("add {}, 32", byte_reg));                      // fold uppercase ASCII to its lowercase byte
}
