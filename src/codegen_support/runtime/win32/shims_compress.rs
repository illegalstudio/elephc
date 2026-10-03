//! Win32 shims for the statically-linked third-party compression/iconv
//! families: zlib (W3d), bzip2 (W3g), and iconv (W3f-A).

use crate::codegen::emit::Emitter;

/// Emits the W3d zlib family of `__rt_sys_*` shims (compressBound, deflateEnd,
/// inflateEnd, deflate, inflate, uncompress, inflateInit2_, compress2,
/// deflateInit2_). Unlike the msvcrt/ws2_32/kernel32 shims above, these wrap
/// symbols statically linked from the MinGW-sysroot `libz.a` (via
/// `ELEPHC_MINGW_SYSROOT`, see `src/linker.rs`). That archive is built by
/// MinGW gcc targeting Windows, so it is MSx64 ABI — NOT SysV — exactly like
/// every other Win32-side symbol this module shims; confirmed by
/// disassembling `compress2`/`deflateInit2_`/`deflate`/`inflate`/
/// `deflateEnd`/`inflateEnd`/`inflateInit2_`/`uncompress` out of a locally
/// cross-built `libzlibstatic.a` (zlib 1.3.1, the same version/build the CI
/// sysroot step produces): every one of them spills its register arguments
/// via `mov %rcx,0x10(%rbp)` / `mov %edx,0x18(%rbp)` / `mov %r8,0x20(%rbp)` /
/// `mov %r9d,0x28(%rbp)` — the standard MSx64 rcx/rdx/r8/r9 prologue, not
/// SysV rdi/rsi/rdx/rcx/r8/r9.
///
/// Return-value cdqe verdict (Class-3 sign-extension rule): NONE of these
/// nine shims sign-extend `eax`→`rax` after the call. Every runtime consumer
/// of a zlib int-status return tests for EQUALITY, not sign:
/// `uncompress` — `strings.rs` gzuncompress: `test rax,rax; jz ok` (zero =
/// success; a `cdqe`-less negative status zero-extends to a nonzero `rax`,
/// which the zero-test still correctly reports as failure);
/// `inflate` — `strings.rs` gzinflate: `cmp QWORD PTR [.], 1; jne fail`
/// (checks for `Z_STREAM_END`==1, unaffected by upper-bit sign-extension);
/// `deflate`/`deflateEnd`/`inflateEnd`/`inflateInit2_`/`deflateInit2_`/
/// `compress2` — no site tests their status at all (deflate/inflate read
/// `z_stream.total_out` instead; the Init/End calls are fire-and-forget in
/// every current call site). `compressBound` returns a `uLong` byte count,
/// never negative, so `cdqe` is moot there too. See `emit_shim_zlib_*` below
/// for the per-shim register-shuffle rationale.
pub(super) fn emit_shim_zlib(emitter: &mut Emitter) {
    emit_shim_zlib_trivial_1arg(emitter);
    emit_shim_zlib_2arg(emitter);
    emit_shim_zlib_4arg(emitter);
    emit_shim_zlib_compress2(emitter);
    emit_shim_zlib_deflate_init2(emitter);
}

/// Emits the trivial 1-arg zlib shims: `compressBound(srcLen)`,
/// `deflateEnd(strm)`, `inflateEnd(strm)`. SysV: rdi=arg1 → MSx64: rcx=arg1.
/// No cdqe: see [`emit_shim_zlib`] for the per-shim cdqe verdict.
fn emit_shim_zlib_trivial_1arg(emitter: &mut Emitter) {
    let shims: &[(&str, &str)] = &[
        ("__rt_sys_compressBound", "compressBound"),
        ("__rt_sys_deflateEnd", "deflateEnd"),
        ("__rt_sys_inflateEnd", "inflateEnd"),
    ];
    for (label, func) in shims {
        emitter.label_global(label);
        emitter.instruction("sub rsp, 40");                                     // shadow(32) + alignment(8)
        emitter.instruction("mov rcx, rdi");                                    // arg1 (strm/srcLen)
        emitter.instruction(&format!("call {}", func));                         // call libz function
        emitter.instruction("add rsp, 40");                                     // restore stack
        emitter.instruction("ret");                                             // return (no cdqe — see emit_shim_zlib)
        emitter.blank();
    }
}

/// Emits the 2-arg zlib shims: `deflate(strm, flush)`, `inflate(strm, flush)`.
/// SysV: rdi=strm, esi=flush → MSx64: rcx=strm, rdx=flush. No cdqe: see
/// [`emit_shim_zlib`] for the per-shim cdqe verdict.
fn emit_shim_zlib_2arg(emitter: &mut Emitter) {
    let shims: &[(&str, &str)] = &[("__rt_sys_deflate", "deflate"), ("__rt_sys_inflate", "inflate")];
    for (label, func) in shims {
        emitter.label_global(label);
        emitter.instruction("sub rsp, 40");                                     // shadow(32) + alignment(8)
        emitter.instruction("mov rdx, rsi");                                    // flush → arg2 (rdx)
        emitter.instruction("mov rcx, rdi");                                    // strm → arg1 (rcx)
        emitter.instruction(&format!("call {}", func));                         // call libz function
        emitter.instruction("add rsp, 40");                                     // restore stack
        emitter.instruction("ret");                                             // return (no cdqe — see emit_shim_zlib)
        emitter.blank();
    }
}

/// Emits the 4-arg zlib shims: `uncompress(dest, &destLen, source, sourceLen)`,
/// `inflateInit2_(strm, windowBits, version, stream_size)`. SysV:
/// rdi,rsi,rdx,rcx → MSx64: rcx,rdx,r8,r9. Register-shuffle hazard: SysV arg4
/// is in `rcx`, which is ALSO MSx64 arg1, so it is saved to `r9` BEFORE
/// `rcx` is overwritten; SysV arg3 is in `rdx`, which is ALSO MSx64 arg2, so
/// it is saved to `r8` FIRST (per the `emit_shim_socket_shims` 4-arg idiom).
/// No cdqe: see [`emit_shim_zlib`] for the per-shim cdqe verdict.
fn emit_shim_zlib_4arg(emitter: &mut Emitter) {
    let shims: &[(&str, &str)] = &[
        ("__rt_sys_uncompress", "uncompress"),
        ("__rt_sys_inflateInit2_", "inflateInit2_"),
    ];
    for (label, func) in shims {
        emitter.label_global(label);
        emitter.instruction("sub rsp, 40");                                     // shadow(32) + alignment(8)
        emitter.instruction("mov r8, rdx");                                     // SAVE arg3 (SysV rdx) before rdx is overwritten
        emitter.instruction("mov r9, rcx");                                     // SAVE arg4 (SysV rcx) before rcx is overwritten
        emitter.instruction("mov rcx, rdi");                                    // arg1 → rcx
        emitter.instruction("mov rdx, rsi");                                    // arg2 → rdx
        emitter.instruction(&format!("call {}", func));                         // call libz function
        emitter.instruction("add rsp, 40");                                     // restore stack
        emitter.instruction("ret");                                             // return (no cdqe — see emit_shim_zlib)
        emitter.blank();
    }
}

/// Emits the `__rt_sys_compress2` shim: converts SysV
/// `compress2(dest, &destLen, source, sourceLen, level)` (rdi, rsi, rdx, rcx,
/// r8) to MSx64 `compress2` (rcx=dest, rdx=&destLen, r8=source, r9=sourceLen,
/// [rsp+32]=level — the 5th arg, on the stack above the 32-byte shadow).
///
/// Register-shuffle hazard, in the ORDER the shim executes it: SysV arg5
/// (level) is in `r8`, which is ALSO the MSx64 arg3 target, so it is saved
/// to `r10` and spilled to its stack slot BEFORE `r8` is overwritten by the
/// arg3 shuffle. SysV arg3 (source) is in `rdx`, which is ALSO MSx64 arg2,
/// so it is saved to `r8` (now free) BEFORE `rdx` is overwritten by the arg2
/// shuffle. SysV arg4 (sourceLen) is in `rcx`, which is ALSO MSx64 arg1, so
/// it is saved to `r9` BEFORE `rcx` is overwritten by the arg1 shuffle.
/// `sub rsp, 56` reserves shadow(32) + the 5th-arg slot(8) + 16 bytes of
/// padding to keep the frame 16-byte aligned (56 ≡ 8 mod 16, matching the
/// mandatory `rsp ≡ 8 (mod 16)` shim-entry convention, so `rsp ≡ 0 (mod 16)`
/// at `call compress2`). No cdqe: see [`emit_shim_zlib`] for the per-shim
/// cdqe verdict — the `test rax,rax; jz` consumer in `gzcompress()` only
/// distinguishes zero from nonzero.
fn emit_shim_zlib_compress2(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_compress2");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + 5th-arg slot(8) + pad(16), 16-byte aligned
    emitter.instruction("mov r10, r8");                                         // SAVE arg5 (SysV r8/level) before r8 is overwritten
    emitter.instruction("mov QWORD PTR [rsp + 32], r10");                       // level → 5th arg (stack slot above shadow)
    emitter.instruction("mov r8, rdx");                                         // SAVE arg3 (SysV rdx/source) before rdx is overwritten
    emitter.instruction("mov r9, rcx");                                         // SAVE arg4 (SysV rcx/sourceLen) before rcx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // dest → arg1 (rcx)
    emitter.instruction("mov rdx, rsi");                                        // &destLen → arg2 (rdx)
    emitter.instruction("call compress2");                                      // zlib compress2 (MSx64 ABI)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see emit_shim_zlib)
    emitter.blank();
}

/// Emits the `__rt_sys_deflateInit2_` shim — the bespoke W3d shim: 8 SysV
/// args (rdi, rsi, edx, ecx, r8d, r9d, plus 2 CALLER-STACK args) to MSx64 (6
/// register args + 2 stack args). This is the only Class-1 zlib shim with
/// more than 4 arguments in ANY ABI, so both the SysV caller-stack layout
/// AND the MSx64 callee-stack layout matter.
///
/// SysV caller side (every call site — `strings.rs` gzcompress/gzdeflate
/// and `stream_filters/zlib.rs` — follows this identical pattern): the
/// caller does `sub rsp, 16` then `mov QWORD PTR [rsp+0], version` and
/// `mov QWORD PTR [rsp+8], stream_size` immediately before `call
/// deflateInit2_`. Verified directly against `strings.rs:1129-1133`
/// (`sub rsp, 16` / `[rsp+0]=zlib version address` / `[rsp+8]=Z_STREAM_SIZE`
/// / `call deflateInit2_`) and `stream_filters/zlib.rs:356-360` (identical
/// shape). Since `call` then pushes an 8-byte return address, at shim ENTRY
/// (before this shim allocates its own frame) those two caller-stack args
/// sit at `[rsp+8]` (version) and `[rsp+16]` (stream_size), relative to the
/// shim's entry `rsp` — NOT to any later stack pointer. This is the single
/// most failure-prone offset in the whole family, so both values are read
/// into scratch registers (`rax`, `r10`) BEFORE `sub rsp, 72` shifts what
/// `[rsp+N]` means.
///
/// MSx64 callee side wants: rcx=strm, rdx=level, r8=method, r9=windowBits,
/// `[rsp+32]`=memLevel, `[rsp+40]`=strategy, `[rsp+48]`=version,
/// `[rsp+56]`=stream_size (4 register args + 4 stack args, the stack args
/// starting immediately above the 32-byte shadow space). Confirmed against a
/// disassembly of `deflateInit2_` cross-built from zlib 1.3.1 for
/// `x86_64-w64-mingw32`: the prologue spills `rcx`→`[rbp+0x10]`,
/// `edx`→`[rbp+0x18]`, `r8d`→`[rbp+0x20]`, `r9d`→`[rbp+0x28]`, then reads its
/// 7th/8th args (version/stream_size) at `[rbp+0x40]`/`[rbp+0x48]` — i.e.
/// caller-stack slots 5–8 sit at `[rsp+32]`/`[rsp+40]`/`[rsp+48]`/`[rsp+56]`
/// from the caller's perspective at `call` time, exactly as this shim lays
/// them out.
///
/// Register-shuffle order (each SysV register is read into its MSx64 target
/// or a scratch register BEFORE being overwritten by an earlier-numbered
/// MSx64 argument): the two register args furthest from a collision
/// (`r8`→memLevel, `r9`→strategy) are spilled to their stack slots first;
/// then SysV arg4 (`rcx`/windowBits, which collides with MSx64 arg1) is
/// saved to `r9`; SysV arg3 (`rdx`/method, which collides with MSx64 arg2)
/// is saved to `r8`; then `rdx`←`rsi` (level) and finally `rcx`←`rdi` (strm)
/// — `rdi`/`rsi` never collide with an earlier MSx64 write, so they move
/// last.
///
/// Alignment: shim entry `rsp ≡ 8 (mod 16)` (the universal post-`call`
/// convention every shim in this module assumes). `sub rsp, 72` — shadow
/// (32) + 4 stack-arg slots (32) + 8 bytes of padding — is itself `≡ 8 (mod
/// 16)`, so `rsp` lands exactly on a 16-byte boundary at `call
/// deflateInit2_`. ✅
///
/// No cdqe: `deflateInit2_`'s int-status return is never sign-tested by any
/// current call site (`gzcompress`/`gzdeflate`/the zlib stream filter all
/// ignore its return value outright) — see [`emit_shim_zlib`] for the
/// family-wide cdqe verdict.
fn emit_shim_zlib_deflate_init2(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_deflateInit2_");
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // version (caller stack arg7) — read BEFORE sub rsp,72 shifts offsets
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // stream_size (caller stack arg8) — read BEFORE sub rsp,72 shifts offsets
    emitter.instruction("sub rsp, 72");                                         // shadow(32) + 4 stack args(32) + pad(8), 16-byte aligned at the call
    emitter.instruction("mov QWORD PTR [rsp + 32], r8");                        // memLevel (SysV arg5) → MSx64 5th arg (stack)
    emitter.instruction("mov QWORD PTR [rsp + 40], r9");                        // strategy (SysV arg6) → MSx64 6th arg (stack)
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // version → MSx64 7th arg (stack)
    emitter.instruction("mov QWORD PTR [rsp + 56], r10");                       // stream_size → MSx64 8th arg (stack)
    emitter.instruction("mov r9, rcx");                                         // windowBits (SysV arg4) → MSx64 arg4 (r9) BEFORE rcx is overwritten
    emitter.instruction("mov r8, rdx");                                         // method (SysV arg3) → MSx64 arg3 (r8) BEFORE rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // level (SysV arg2) → MSx64 arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // strm (SysV arg1) → MSx64 arg1 (rcx)
    emitter.instruction("call deflateInit2_");                                  // zlib deflateInit2_ (MSx64 ABI, 4 reg + 4 stack args)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see emit_shim_zlib)
    emitter.blank();
}

/// Emits the W3g bzip2 family of `__rt_sys_BZ2_*` shims: `BZ2_bzCompress`,
/// `BZ2_bzCompressInit`, `BZ2_bzCompressEnd` (`stream_filters/bzip2.rs`
/// `emit_compress_x86_64`), and `BZ2_bzBuffToBuffDecompress`
/// (`stream_filters/compress_bzip2_stream.rs` `emit_x86_64`). Like the W3d
/// zlib family (see [`emit_shim_zlib`]), these wrap symbols statically
/// linked from the MinGW-sysroot `libbz2.a` (via `ELEPHC_MINGW_SYSROOT`,
/// `src/linker.rs:406`, `-lbz2`), MSx64 ABI (built by MinGW gcc), NOT SysV.
/// No cdqe on any of the four: every current call site either ignores the
/// libbz2 int-status return outright (`BZ2_bzCompress` in the fwrite loop,
/// `BZ2_bzCompressInit`, `BZ2_bzCompressEnd`) or only equality/zero-tests it
/// (`BZ2_bzCompress` in the close loop: `cmp ..., 4` for `BZ_STREAM_END`;
/// `BZ2_bzBuffToBuffDecompress`: `test eax, eax` / `jnz`) — none sign-tests
/// (`js`/`jl`) the result, so no shim needs a sign-extending `cdqe`.
pub(super) fn emit_shim_bzip2(emitter: &mut Emitter) {
    // BZ2_bzCompressEnd(strm) — 1 arg. SysV: rdi=strm → MSx64: rcx=strm.
    emitter.label_global("__rt_sys_BZ2_bzCompressEnd");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // strm → arg1 (rcx)
    emitter.instruction("call BZ2_bzCompressEnd");                              // libbz2 BZ2_bzCompressEnd (MSx64 ABI)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see emit_shim_bzip2)
    emitter.blank();

    // BZ2_bzCompress(strm, action) — 2 args. SysV: rdi=strm, esi=action →
    // MSx64: rcx=strm, edx=action.
    emitter.label_global("__rt_sys_BZ2_bzCompress");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov edx, esi");                                        // action → arg2 (edx)
    emitter.instruction("mov rcx, rdi");                                        // strm → arg1 (rcx)
    emitter.instruction("call BZ2_bzCompress");                                 // libbz2 BZ2_bzCompress (MSx64 ABI)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see emit_shim_bzip2)
    emitter.blank();

    // BZ2_bzCompressInit(strm, blockSize100k, verbosity, workFactor) — 4
    // args. SysV: rdi=strm, esi=blockSize100k, edx=verbosity, ecx=workFactor
    // → MSx64: rcx=strm, rdx=blockSize100k, r8=verbosity, r9=workFactor.
    // Register-shuffle hazard (the `emit_shim_zlib_4arg` idiom): SysV arg4 is
    // in `rcx`, ALSO the MSx64 arg1 target, so it is saved to `r9` BEFORE
    // `rcx` is overwritten; SysV arg3 is in `rdx`, ALSO MSx64 arg2, so it is
    // saved to `r8` FIRST.
    emitter.label_global("__rt_sys_BZ2_bzCompressInit");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // SAVE arg3 (SysV rdx/verbosity) before rdx is overwritten
    emitter.instruction("mov r9, rcx");                                         // SAVE arg4 (SysV rcx/workFactor) before rcx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // strm → arg1 (rcx)
    emitter.instruction("mov rdx, rsi");                                        // blockSize100k → arg2 (rdx)
    emitter.instruction("call BZ2_bzCompressInit");                             // libbz2 BZ2_bzCompressInit (MSx64 ABI)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see emit_shim_bzip2)
    emitter.blank();

    // BZ2_bzBuffToBuffDecompress(dest, &destLen, source, sourceLen, small,
    // verbosity) — 6 args. SysV (regular C ABI, NOT the syscall r10-for-arg4
    // convention `sendto`/`recvfrom` use): rdi=dest, rsi=&destLen, rdx=source,
    // ecx=sourceLen, r8d=small, r9d=verbosity (confirmed against the call
    // site, `compress_bzip2_stream.rs` `emit_x86_64`@190-196) → MSx64:
    // rcx=dest, rdx=&destLen, r8=source, r9d=sourceLen, `[rsp+32]`=small,
    // `[rsp+40]`=verbosity (4 reg args + 2 stack args, immediately above the
    // 32-byte shadow — the `sendto`/`recvfrom` stack-arg layout, but SysV
    // arg4 arrives in `rcx` here, not `r10`).
    //
    // Register-shuffle order (each SysV register is read BEFORE being
    // overwritten by an earlier-numbered MSx64 write): `r9d`/`r8d`
    // (verbosity/small) are spilled to their stack slots first (nothing else
    // targets them); then SysV arg3 (`rdx`/source, which collides with MSx64
    // arg2) is saved to `r8`; then SysV arg4 (`ecx`/sourceLen, which collides
    // with MSx64 arg1) is saved to `r9d`; then `rdx`←`rsi` (&destLen) and
    // finally `rcx`←`rdi` (dest) — `rdi`/`rsi` never collide with an earlier
    // MSx64 write, so they move last.
    //
    // `sub rsp, 56` — shadow(32) + 2 stack-arg slots(16) + pad(8) — is `≡ 8
    // (mod 16)`, matching the universal `rsp ≡ 8 (mod 16)` shim-entry
    // convention, so `rsp ≡ 0 (mod 16)` at `call BZ2_bzBuffToBuffDecompress`.
    emitter.label_global("__rt_sys_BZ2_bzBuffToBuffDecompress");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + 2 stack args(16) + pad(8), 16-byte aligned
    emitter.instruction("mov DWORD PTR [rsp + 40], r9d");                       // verbosity → 6th arg (stack)
    emitter.instruction("mov DWORD PTR [rsp + 32], r8d");                       // small → 5th arg (stack)
    emitter.instruction("mov r8, rdx");                                         // SAVE arg3 (SysV rdx/source) before rdx is overwritten
    emitter.instruction("mov r9d, ecx");                                        // SAVE arg4 (SysV ecx/sourceLen) before rcx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // &destLen → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // dest → arg1 (rcx)
    emitter.instruction("call BZ2_bzBuffToBuffDecompress");                     // libbz2 BZ2_bzBuffToBuffDecompress (MSx64 ABI)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see emit_shim_bzip2)
    emitter.blank();
}

/// Emits the W3f-A `__rt_sys_iconv_open`/`__rt_sys_iconv`/`__rt_sys_iconv_close`
/// family: real ABI shims for `stream_filters/iconv.rs` and
/// `stream_filters/iconv_write.rs`'s x86_64 call sites. Unlike the W3f-B
/// rewrite family below, these are NOT loud-fail stubs: libiconv IS
/// statically linked on windows-x86_64 (`src/linker.rs:256/406`, `-liconv`,
/// MinGW sysroot), MSx64 ABI — the exact same "statically-linked MinGW
/// archive" situation as the W3d zlib family (`emit_shim_zlib`) and the
/// W3e-1 PCRE2-POSIX family (`emit_shim_pcre2_posix`).
/// GNU libiconv exports these entry points with the `libiconv_*` prefix;
/// `<iconv.h>` normally supplies the source-level macro aliases, which hand
/// written assembly must apply explicitly.
pub(crate) fn emit_shim_iconv(emitter: &mut Emitter) {
    emit_shim_iconv_open(emitter);
    emit_shim_iconv_call(emitter);
    emit_shim_iconv_close(emitter);
}

/// Emits the `__rt_sys_iconv_open` shim: converts SysV `iconv_open(const
/// char* tocode, const char* fromcode)` (rdi, rsi) to MSx64 `iconv_open`
/// (rcx=tocode, rdx=fromcode). No register collision (rdi/rsi never overlap
/// rcx/rdx), so the two moves may run in either order.
///
/// Return-value cdqe verdict: `iconv_open` returns `iconv_t` — a full 64-bit
/// value where failure is `(iconv_t)-1`. `call iconv_open` leaves that
/// already-64-bit value in `rax` untouched by this shim (no truncating
/// 32-bit write occurs anywhere in the shim body), so `(iconv_t)-1` already
/// reads back as all-ones across the full 64-bit `rax` — exactly what the
/// consumer's `cmp rax, -1` / `cmn x0, #1` sign-tests expect. No cdqe needed
/// or possible (cdqe would incorrectly re-sign-extend from a 32-bit `eax`
/// that was never separately written).
fn emit_shim_iconv_open(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_iconv_open");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rdx, rsi");                                        // fromcode → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // tocode → arg1 (rcx)
    emitter.instruction("call libiconv_open");                                  // GNU libiconv open export (MSx64 ABI, returns iconv_t in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see doc above)
    emitter.blank();
}

/// Emits the `__rt_sys_iconv` shim: converts SysV `iconv(iconv_t cd, char**
/// inbuf, size_t* inbytesleft, char** outbuf, size_t* outbytesleft)` (rdi,
/// rsi, rdx, rcx, r8) to MSx64 `iconv` (rcx=cd, rdx=inbuf, r8=inbytesleft,
/// r9=outbuf, `[rsp+32]`=outbytesleft — the 5th arg, on the stack above the
/// 32-byte shadow space). Identical register shape to
/// `emit_shim_pcre2_regexec` (also rdi/rsi/rdx/rcx/r8 → rcx/rdx/r8/r9/stack),
/// so this shim mirrors its register-shuffle order exactly.
///
/// Register-shuffle order, in the ORDER the shim executes it: SysV arg5
/// (outbytesleft) is in `r8`, which is ALSO the MSx64 arg3 (inbytesleft)
/// target, so it is saved to `r10` and spilled to its `[rsp+32]` stack slot
/// FIRST, before `r8` is overwritten by the arg3 shuffle. SysV arg3
/// (inbytesleft) is in `rdx`, which is ALSO MSx64 arg2 (inbuf), so it is
/// saved to `r8` (now free) BEFORE `rdx` is overwritten by the arg2 shuffle.
/// SysV arg4 (outbuf) is in `rcx`, which is ALSO MSx64 arg1 (cd), so it is
/// saved to `r9` BEFORE `rcx` is overwritten by the arg1 shuffle. `rdx`←`rsi`
/// (inbuf) and `rcx`←`rdi` (cd) move last since `rdi`/`rsi` never collide
/// with an earlier MSx64 write.
///
/// Alignment: shim entry `rsp ≡ 8 (mod 16)`. `sub rsp, 56` reserves shadow
/// (32) + the 5th-arg slot(8) + 16 bytes of padding — `56 ≡ 8 (mod 16)`, so
/// `rsp` lands exactly on a 16-byte boundary at `call iconv`.
///
/// No cdqe: `iconv` returns a `size_t` conversion count (or `(size_t)-1` on
/// error), but NEITHER x86_64 call site (`stream_filters/iconv.rs`'s read
/// filter, `stream_filters/iconv_write.rs`'s write-filter loop) reads `rax`
/// after the call at all — both recompute the converted/produced byte count
/// from the before/after `outbytesleft` cursor instead (`iconv.rs`: `mov r9,
/// [rsp+24]; sub r9, [rsp+72]`; `iconv_write.rs`: `mov rax, ICONV_SCRATCH;
/// sub rax, [rbp-48]`). The `iconv` return value is write-only dead output
/// at every current call site, so no cdqe verdict is even reachable here.
fn emit_shim_iconv_call(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_iconv");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + 5th-arg slot(8) + pad(16), 16-byte aligned
    emitter.instruction("mov r10, r8");                                         // SAVE outbytesleft (SysV arg5/r8) before r8 is overwritten
    emitter.instruction("mov QWORD PTR [rsp + 32], r10");                       // outbytesleft → MSx64 5th arg (stack slot above shadow)
    emitter.instruction("mov r8, rdx");                                         // SAVE inbytesleft (SysV arg3/rdx) before rdx is overwritten
    emitter.instruction("mov r9, rcx");                                         // SAVE outbuf (SysV arg4/rcx) before rcx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // cd → arg1 (rcx)
    emitter.instruction("mov rdx, rsi");                                        // inbuf → arg2 (rdx)
    emitter.instruction("call libiconv");                                       // GNU libiconv conversion export (MSx64 ABI, returns size_t in rax)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see doc above: rax is dead at every call site)
    emitter.blank();
}

/// Emits the `__rt_sys_iconv_close` shim: converts SysV
/// `iconv_close(iconv_t cd)` (rdi) to MSx64 `iconv_close` (rcx=cd). Mirrors
/// `emit_shim_zlib_trivial_1arg` (1-arg case). `iconv_close` returns an `int`
/// status, but neither x86_64 call site (`iconv.rs`, `iconv_write.rs`) reads
/// `rax` after the call — both immediately move on to unrelated work
/// (`__rt_tmpfile`, reloading the descriptor) — so no cdqe verdict is
/// reachable here either.
fn emit_shim_iconv_close(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_iconv_close");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // cd → arg1 (rcx)
    emitter.instruction("call libiconv_close");                                 // GNU libiconv close export (MSx64 ABI, returns int in eax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — see doc above: rax is dead at every call site)
    emitter.blank();
}
