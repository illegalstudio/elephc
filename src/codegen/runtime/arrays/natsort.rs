use crate::codegen::emit::Emitter;

/// natsort / natcasesort: natural order sort.
/// For int arrays, natural order is the same as numeric order,
/// so these delegate to the existing sort routine.
pub fn emit_natsort(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: natsort (natural order sort, delegates to numeric sort) ---");
    emitter.label_global("__rt_natsort");

    // -- for integer arrays, natural sort is numeric sort --
    emitter.instruction("b __rt_sort_int");                                     // tail-call to sort_int (ascending)

    emitter.blank();
    emitter.comment("--- runtime: natcasesort (case-insensitive natural sort, delegates to numeric sort) ---");
    emitter.label_global("__rt_natcasesort");

    // -- for integer arrays, case is irrelevant; same as numeric sort --
    emitter.instruction("b __rt_sort_int");                                     // tail-call to sort_int (ascending)
}
