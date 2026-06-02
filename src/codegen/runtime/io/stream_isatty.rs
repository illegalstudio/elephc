//! Purpose:
//! Emits the `__rt_stream_isatty` runtime helper assembly for stream_isatty.
//! Probes whether a file descriptor refers to a terminal via an `ioctl` request.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - Uses the platform terminal-attributes `ioctl` request (`TIOCGETA` on macOS,
//!   `TCGETS` on Linux); a successful call means the descriptor is a terminal.

use crate::codegen::{emit::Emitter, platform::Arch};

/// stream_isatty: report whether a file descriptor is connected to a terminal.
/// Input:  x0=fd
/// Output: x0=1 if the descriptor is a terminal, 0 otherwise
pub fn emit_stream_isatty(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_isatty_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_isatty ---");
    emitter.label_global("__rt_stream_isatty");

    let request = emitter.platform.tty_get_request();
    let low = request & 0xFFFF;
    let high = (request >> 16) & 0xFFFF;

    // -- probe the descriptor with the terminal-attributes ioctl --
    emitter.instruction("sub sp, sp, #144");                                    // reserve scratch for the struct termios the ioctl fills in
    emitter.instruction(&format!("movz x1, #0x{:X}", low));                     // load the low half of the terminal-attributes ioctl request
    if high != 0 {
        emitter.instruction(&format!("movk x1, #0x{:X}, lsl #16", high));       // load the high half of the request
    }
    emitter.instruction("mov x2, sp");                                          // pass the scratch struct termios buffer as the ioctl output argument
    emitter.syscall(54);
    emitter.instruction("add sp, sp, #144");                                    // release the struct termios scratch buffer

    // -- turn the ioctl result into a boolean --
    emitter.instruction("cmp x0, #0");                                          // ioctl returns 0 for a terminal and a negative errno otherwise
    emitter.instruction("cset x0, eq");                                         // x0 = 1 when the descriptor is a terminal, else 0
    emitter.instruction("ret");                                                 // return the terminal flag to the caller
}

fn emit_stream_isatty_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_isatty ---");
    emitter.label_global("__rt_stream_isatty");

    let request = emitter.platform.tty_get_request();

    // -- probe the descriptor with the terminal-attributes ioctl --
    emitter.instruction("sub rsp, 144");                                        // reserve scratch for the struct termios the ioctl fills in
    emitter.instruction("mov rdx, rsp");                                        // pass the scratch struct termios buffer as the third ioctl argument
    emitter.instruction(&format!("mov esi, 0x{:X}", request));                  // second ioctl argument: terminal-attributes request
    emitter.instruction("mov eax, 16");                                         // Linux x86_64 syscall 16 = ioctl
    emitter.instruction("syscall");                                             // probe the descriptor in rdi for terminal attributes
    emitter.instruction("add rsp, 144");                                        // release the struct termios scratch buffer

    // -- turn the ioctl result into a boolean --
    emitter.instruction("cmp rax, 0");                                          // ioctl returns 0 for a terminal and a negative errno otherwise
    emitter.instruction("sete al");                                             // al = 1 when the descriptor is a terminal
    emitter.instruction("movzx rax, al");                                       // zero-extend the terminal flag into the full result register
    emitter.instruction("ret");                                                 // return the terminal flag to the caller
}
