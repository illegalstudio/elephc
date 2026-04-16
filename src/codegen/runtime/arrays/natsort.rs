use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// natsort / natcasesort: natural order sort.
/// For int arrays, natural order is the same as numeric order,
/// so these delegate to the existing sort routine.
pub fn emit_natsort(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_natsort_linux_x86_64(emitter);
        return;
    }

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

fn emit_natsort_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: natsort (natural order sort, delegates to numeric sort) ---");
    emitter.label_global("__rt_natsort");
    emitter.instruction("jmp __rt_sort_int");                                   // tail-jump to the x86_64 ascending integer sort helper because indexed integers already satisfy natural-order semantics

    emitter.blank();
    emitter.comment("--- runtime: natcasesort (case-insensitive natural sort, delegates to numeric sort) ---");
    emitter.label_global("__rt_natcasesort");
    emitter.instruction("jmp __rt_sort_int");                                   // tail-jump to the same x86_64 ascending integer sort helper because case-folding is irrelevant for integer payloads
}
