use crate::codegen::emit::Emitter;

/// asort / arsort: sort array by values (for indexed int arrays, same as sort/rsort).
/// Input:  x0=array_ptr
/// Output: array sorted in-place
/// For indexed int arrays, sorting by value is identical to sort/rsort since
/// there are no string keys to preserve — just delegate to existing sort routines.
pub fn emit_asort(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: asort (sort by values ascending) ---");
    emitter.label_global("__rt_asort");

    // -- delegate to existing ascending integer sort --
    emitter.instruction("b __rt_sort_int");                                     // tail-call to sort_int (ascending)

    emitter.blank();
    emitter.comment("--- runtime: arsort (sort by values descending) ---");
    emitter.label_global("__rt_arsort");

    // -- delegate to existing descending integer sort --
    emitter.instruction("b __rt_rsort_int");                                    // tail-call to rsort_int (descending)
}
