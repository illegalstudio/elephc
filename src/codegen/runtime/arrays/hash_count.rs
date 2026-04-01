use crate::codegen::emit::Emitter;

/// hash_count: get the number of entries in a hash table.
/// Input:  x0=hash_table_ptr
/// Output: x0=count
pub fn emit_hash_count(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_count ---");
    emitter.label_global("__rt_hash_count");

    // -- load count from header offset 0 --
    emitter.instruction("ldr x0, [x0]");                                        // x0 = count from hash table header
    emitter.instruction("ret");                                                 // return to caller
}
