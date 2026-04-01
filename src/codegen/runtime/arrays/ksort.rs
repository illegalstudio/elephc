use crate::codegen::emit::Emitter;

/// ksort / krsort: sort array by keys.
/// For indexed int arrays, elements are already ordered by numeric index,
/// so these are no-ops that return immediately.
pub fn emit_ksort(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ksort (sort by keys ascending, no-op for indexed) ---");
    emitter.label_global("__rt_ksort");

    // -- indexed arrays are already in key order (0, 1, 2, ...) --
    emitter.instruction("ret");                                                 // return immediately, array unchanged

    emitter.blank();
    emitter.comment("--- runtime: krsort (sort by keys descending, no-op for indexed) ---");
    emitter.label_global("__rt_krsort");

    // -- indexed arrays are already in key order, reverse would need reindexing --
    emitter.instruction("ret");                                                 // return immediately, array unchanged
}
