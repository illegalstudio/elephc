use crate::codegen::{emit::Emitter, platform::Arch};

/// ltrim: strip whitespace from left. Adjusts x1 and x2.
pub fn emit_ltrim(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ltrim_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ltrim ---");
    emitter.label_global("__rt_ltrim");
    emitter.label("__rt_ltrim_loop");
    emitter.instruction("cbz x2, __rt_ltrim_done");                             // if string is empty, nothing to trim
    emitter.instruction("ldrb w9, [x1]");                                       // peek at first byte without advancing
    emitter.instruction("cmp w9, #32");                                         // check for space (0x20)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if space, skip it
    emitter.instruction("cmp w9, #9");                                          // check for tab (0x09)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if tab, skip it
    emitter.instruction("cmp w9, #10");                                         // check for newline (0x0A)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if newline, skip it
    emitter.instruction("cmp w9, #13");                                         // check for carriage return (0x0D)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if CR, skip it
    emitter.instruction("b __rt_ltrim_done");                                   // non-whitespace found, stop trimming

    // -- advance past whitespace character --
    emitter.label("__rt_ltrim_skip");
    emitter.instruction("add x1, x1, #1");                                      // advance string pointer past whitespace
    emitter.instruction("sub x2, x2, #1");                                      // decrement string length
    emitter.instruction("b __rt_ltrim_loop");                                   // check next character

    emitter.label("__rt_ltrim_done");
    emitter.instruction("ret");                                                 // return with adjusted x1 and x2
}

fn emit_ltrim_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ltrim ---");
    emitter.label_global("__rt_ltrim");
    emitter.label("__rt_ltrim_loop_x86");
    emitter.instruction("test rdx, rdx");                                       // is the borrowed string slice already empty before trimming any leading whitespace?
    emitter.instruction("je __rt_ltrim_done_x86");                              // stop immediately when there are no bytes left to inspect
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // peek at the first byte of the borrowed string slice without advancing the pointer yet
    emitter.instruction("cmp cl, 32");                                          // is the first byte a space that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading space and continue trimming from the new front
    emitter.instruction("cmp cl, 9");                                           // is the first byte a tab that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading tab and continue trimming from the new front
    emitter.instruction("cmp cl, 10");                                          // is the first byte a newline that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading newline and continue trimming from the new front
    emitter.instruction("cmp cl, 13");                                          // is the first byte a carriage return that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading carriage return and continue trimming from the new front
    emitter.instruction("jmp __rt_ltrim_done_x86");                             // stop once the first byte is no longer classified as trim-worthy whitespace

    emitter.label("__rt_ltrim_skip_x86");
    emitter.instruction("add rax, 1");                                          // advance the borrowed string pointer past the stripped leading whitespace byte
    emitter.instruction("sub rdx, 1");                                          // shrink the borrowed string length to match the removed leading whitespace byte
    emitter.instruction("jmp __rt_ltrim_loop_x86");                             // continue trimming from the new front of the borrowed string slice

    emitter.label("__rt_ltrim_done_x86");
    emitter.instruction("ret");                                                 // return the adjusted borrowed string slice in rax/rdx
}
