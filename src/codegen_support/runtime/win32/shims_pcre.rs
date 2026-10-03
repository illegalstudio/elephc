//! Win32 shims for the PCRE2-POSIX family (W3e-1) plus msvcrt malloc/free.

use crate::codegen::emit::Emitter;

/// Emits the W3e-1 PCRE2-POSIX family of `__rt_sys_*` shims: `pcre2_regcomp`,
/// `pcre2_regexec`, `pcre2_regfree`. Like the W3d zlib family (see
/// [`emit_shim_zlib`]), these wrap symbols statically linked from the
/// MinGW-sysroot `libpcre2-posix.a`/`libpcre2-8.a` (via `ELEPHC_MINGW_SYSROOT`,
/// `src/linker.rs:255`), which is MSx64 ABI — NOT SysV — because it is built
/// by MinGW gcc targeting Windows, exactly like every other Win32-side symbol
/// this module shims.
///
/// Return-value cdqe verdict (Class-3 sign-extension rule): `pcre2_regcomp`
/// and `pcre2_regexec` both return an `int` status where 0 means
/// success/match and nonzero means failure/no-match. EVERY current runtime
/// consumer tests this return with `test eax, eax` followed by `jnz`/`jz`
/// (equality against zero), never `js`/`jl` (sign test) — verified against
/// every x86_64 call site in `preg_match.rs` (`:365-366`, `:434-435`,
/// `:464-465`), `preg_split.rs` (regcomp/regexec status checks in
/// `emit_preg_split_linux_x86_64`), `preg_replace.rs` (`:337`/`:364-365`),
/// and `preg_replace_callback.rs` (regcomp/regexec status checks in
/// `emit_preg_replace_callback_linux_x86_64`) — so a `cdqe`-less negative
/// status (which zero-extends into a nonzero `rax`) is still correctly
/// reported as failure by every `test`/`jnz` consumer. No shim below performs
/// `cdqe`. `pcre2_regfree` returns `void`, so cdqe is moot there too.
pub(super) fn emit_shim_pcre2_posix(emitter: &mut Emitter) {
    emit_shim_pcre2_regcomp(emitter);
    emit_shim_pcre2_regexec(emitter);
    emit_shim_pcre2_regfree(emitter);
}

/// Emits the `__rt_sys_pcre2_regcomp` shim: converts SysV
/// `pcre2_regcomp(regex_t* preg, const char* pattern, int cflags)` (rdi, rsi,
/// edx) to MSx64 `pcre2_regcomp` (rcx=preg, rdx=pattern, r8d=cflags).
/// Register-shuffle hazard: SysV arg3 (cflags) is in `edx`, which is ALSO the
/// MSx64 arg2 target, so it is saved to `r8` BEFORE `rdx` is overwritten by
/// the arg2 shuffle (rsi→rdx); `rdi`→`rcx` moves last since it never
/// collides with an earlier MSx64 write. No cdqe: see
/// [`emit_shim_pcre2_posix`] for the family-wide cdqe verdict — the
/// `test eax,eax; jnz/jz` consumers only distinguish zero from nonzero.
fn emit_shim_pcre2_regcomp(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_pcre2_regcomp");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // SAVE cflags (SysV arg3) before rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // pattern → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // preg → arg1 (rcx)
    emitter.instruction("call pcre2_regcomp");                                  // libpcre2-posix pcre2_regcomp (MSx64 ABI, returns int in eax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see emit_shim_pcre2_posix)
    emitter.blank();
}

/// Emits the `__rt_sys_pcre2_regexec` shim: converts SysV `pcre2_regexec(const
/// regex_t* preg, const char* string, size_t nmatch, regmatch_t* pmatch, int
/// eflags)` (rdi, rsi, rdx, rcx, r8d) to MSx64 `pcre2_regexec` (rcx=preg,
/// rdx=string, r8=nmatch, r9=pmatch, `[rsp+32]`=eflags — the 5th arg, on the
/// stack above the 32-byte shadow space). This is the only 5-argument shim in
/// the W3e-1 family, so both register AND stack-arg placement matter.
///
/// Register-shuffle order, in the ORDER the shim executes it (each SysV
/// register is read into its MSx64 target or a scratch register BEFORE being
/// overwritten by an earlier-numbered MSx64 argument): SysV arg5 (eflags) is
/// in `r8`, which is ALSO the MSx64 arg3 (nmatch) target, so it is saved to
/// `r10` and spilled to its `[rsp+32]` stack slot FIRST, before `r8` is
/// overwritten by the arg3 shuffle. SysV arg3 (nmatch) is in `rdx`, which is
/// ALSO MSx64 arg2 (string), so it is saved to `r8` (now free) BEFORE `rdx`
/// is overwritten by the arg2 shuffle. SysV arg4 (pmatch) is in `rcx`, which
/// is ALSO MSx64 arg1 (preg), so it is saved to `r9` BEFORE `rcx` is
/// overwritten by the arg1 shuffle. `rdx`←`rsi` (string) and `rcx`←`rdi`
/// (preg) move last since `rdi`/`rsi` never collide with an earlier MSx64
/// write.
///
/// Alignment: shim entry `rsp ≡ 8 (mod 16)` (the universal post-`call`
/// convention every shim in this module assumes). `sub rsp, 56` reserves
/// shadow(32) + the 5th-arg slot(8) + 16 bytes of padding — `56 ≡ 8 (mod
/// 16)`, so `rsp` lands exactly on a 16-byte boundary at `call
/// pcre2_regexec`. No cdqe: see [`emit_shim_pcre2_posix`] for the family-wide
/// cdqe verdict.
fn emit_shim_pcre2_regexec(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_pcre2_regexec");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + 5th-arg slot(8) + pad(16), 16-byte aligned
    emitter.instruction("mov r10, r8");                                         // SAVE eflags (SysV arg5/r8) before r8 is overwritten
    emitter.instruction("mov QWORD PTR [rsp + 32], r10");                       // eflags → MSx64 5th arg (stack slot above shadow)
    emitter.instruction("mov r8, rdx");                                         // SAVE nmatch (SysV arg3/rdx) before rdx is overwritten
    emitter.instruction("mov r9, rcx");                                         // SAVE pmatch (SysV arg4/rcx) before rcx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // preg → arg1 (rcx)
    emitter.instruction("mov rdx, rsi");                                        // string → arg2 (rdx)
    emitter.instruction("call pcre2_regexec");                                  // libpcre2-posix pcre2_regexec (MSx64 ABI, returns int in eax)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see emit_shim_pcre2_posix)
    emitter.blank();
}

/// Emits the `__rt_sys_pcre2_regfree` shim: converts SysV
/// `pcre2_regfree(regex_t* preg)` (rdi) to MSx64 `pcre2_regfree` (rcx=preg).
/// Mirrors `emit_shim_zlib_trivial_1arg` (1-arg case). `pcre2_regfree`
/// returns `void`, so no cdqe is possible or needed — see
/// [`emit_shim_pcre2_posix`] for the family-wide cdqe verdict.
fn emit_shim_pcre2_regfree(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_pcre2_regfree");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // preg → arg1 (rcx)
    emitter.instruction("call pcre2_regfree");                                  // libpcre2-posix pcre2_regfree (MSx64 ABI, void return)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (void — no cdqe)
    emitter.blank();
}

/// Emits the `__rt_sys_malloc` shim: converts SysV `malloc(size_t size)`
/// (rdi) to MSx64 `malloc` (rcx=size), calling the standard msvcrt `malloc`
/// import. Mirrors `emit_shim_zlib_trivial_1arg` (1-arg case). The runtime
/// uses this to allocate the dynamic `regmatch_t` capture vector for PCRE2
/// regexec calls (NOT the PHP heap — `__rt_heap_alloc` is a separate,
/// unrelated allocator). The pointer return stays in `rax` (never
/// sign-tested), so no cdqe.
pub(super) fn emit_shim_malloc(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_malloc");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // size → arg1 (rcx)
    emitter.instruction("call malloc");                                         // msvcrt malloc (returns void* in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return pointer (no cdqe: never sign-tested)
    emitter.blank();
}

/// Emits the `__rt_sys_free` shim: converts SysV `free(void* ptr)` (rdi) to
/// MSx64 `free` (rcx=ptr), calling the standard msvcrt `free` import. Mirrors
/// `emit_shim_zlib_trivial_1arg` (1-arg case). Frees the dynamic
/// `regmatch_t` capture vector allocated by `__rt_sys_malloc` (see
/// [`emit_shim_malloc`]). `free` returns `void`, so no cdqe.
pub(super) fn emit_shim_free(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_free");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // ptr → arg1 (rcx)
    emitter.instruction("call free");                                           // msvcrt free (void return)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (void — no cdqe)
    emitter.blank();
}
