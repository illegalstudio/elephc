//! Purpose:
//! Emits the `__rt_stream_isatty` runtime helper assembly for stream_isatty.
//! Probes whether a file descriptor refers to a terminal through the native
//! platform terminal API.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Uses `GetConsoleMode` on Windows after `_get_osfhandle`; Unix targets use
//!   the platform terminal-attributes `ioctl` request.

use crate::codegen_support::{
    emit::Emitter,
    platform::{Arch, Platform},
};

/// stream_isatty: report whether a file descriptor is connected to a terminal.
/// Input:  x0=fd
/// Output: x0=1 if the descriptor is a terminal, 0 otherwise
pub fn emit_stream_isatty(emitter: &mut Emitter) {
    if emitter.platform == Platform::Windows {
        emit_stream_isatty_windows_x86_64(emitter);
        return;
    }

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

/// Emits the Windows x86_64 `stream_isatty` helper through the CRT descriptor
/// table and the Win32 console-mode probe.
///
/// The generated runtime enters with its normal SysV-style descriptor in `rdi`,
/// while `_get_osfhandle` and `GetConsoleMode` use the MSx64 ABI. A CRT lookup
/// failure or a redirected handle makes the helper return false without ever
/// treating the descriptor as a Winsock socket.
fn emit_stream_isatty_windows_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_isatty ---");
    emitter.label_global("__rt_stream_isatty");

    // -- resolve the CRT descriptor, then ask the console itself for its mode --
    emitter.instruction("sub rsp, 56");                                         // reserve MSx64 shadow space and an aligned console-mode DWORD
    emitter.instruction("mov ecx, edi");                                        // CRT descriptor for _get_osfhandle's MSx64 first argument
    emitter.instruction("call _get_osfhandle");                                 // recover the native HANDLE or INVALID_HANDLE_VALUE
    emitter.instruction("cmp rax, -1");                                         // invalid CRT descriptor or raw socket?
    emitter.instruction("je .Lstream_isatty_windows_false");                    // neither is a terminal stream
    emitter.instruction("mov rcx, rax");                                        // hConsoleHandle = converted CRT descriptor
    emitter.instruction("lea rdx, [rsp + 40]");                                 // lpMode = stack-local DWORD outside shadow space
    emitter.instruction("call GetConsoleMode");                                 // only console handles expose a readable mode
    emitter.instruction("test eax, eax");                                       // Win32 BOOL success means an attached console
    emitter.instruction("setne al");                                            // al = 1 for a terminal and 0 for a redirected handle
    emitter.instruction("movzx eax, al");                                       // zero-extend the PHP boolean result
    emitter.instruction("add rsp, 56");                                         // release shadow space and mode local
    emitter.instruction("ret");                                                 // return the terminal flag
    emitter.label(".Lstream_isatty_windows_false");
    emitter.instruction("xor eax, eax");                                        // invalid descriptors and raw sockets are not terminals
    emitter.instruction("add rsp, 56");                                         // release shadow space and mode local
    emitter.instruction("ret");                                                 // return false
}

/// Emits the Linux x86_64 stream runtime helper for stream isatty.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Verifies Windows probes console handles instead of issuing a socket ioctl.
    #[test]
    fn windows_uses_get_console_mode() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_stream_isatty(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("call _get_osfhandle"));
        assert!(asm.contains("call GetConsoleMode"));
        assert!(!asm.contains("ioctl"));
        assert!(!asm.contains("ioctlsocket"));
    }
}
