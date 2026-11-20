//! Purpose:
//! Owns fixed native stream tables and their indexed storage operations.
//!
//! Called from:
//! - Stream builtins, runtime stream adapters and web request reset emission.
//!
//! Key details:
//! - Callers select descriptors and retain control over PHP callbacks.
//! - Table declarations and reset byte counts share the same geometry.

use crate::{Arch, Emitter, data::StaticStorage};

/// Number of ordinary fd entries in the stream side tables.
pub const FD_CAPACITY: usize = 256;
/// Number of wrapper registrations.
pub const WRAPPER_REGISTRATION_CAPACITY: usize = 64;
/// Number of live synthetic wrapper handles.
pub const WRAPPER_HANDLE_CAPACITY: usize = 256;
/// Number of filter registrations.
pub const FILTER_REGISTRATION_CAPACITY: usize = 128;
/// Shift from a wrapper or filter registration index to a two-pair row.
pub const REGISTRATION_SHIFT: usize = 5;
/// Width of one pair of independently owned registration strings.
pub const REGISTRATION_BYTES: usize = 1 << REGISTRATION_SHIFT;

const WORD: usize = 8;
const HOST_PAIR_SHIFT: usize = 4;

/// Defines the bytes reserved for one table from its number of rows and stride.
const fn table(symbol: &'static str, count: usize, stride: usize) -> StaticStorage {
    StaticStorage { symbol, bytes: count * stride }
}

pub const USER_WRAPPERS: StaticStorage = table("_user_wrappers", WRAPPER_REGISTRATION_CAPACITY, REGISTRATION_BYTES);
pub const USER_WRAPPER_HANDLES: StaticStorage = table("_user_wrapper_handles", WRAPPER_HANDLE_CAPACITY, WORD);
pub const USER_FILTER_REGISTRY: StaticStorage = table("_user_filter_registry", FILTER_REGISTRATION_CAPACITY, REGISTRATION_BYTES);
pub const USER_FILTER_INSTANCES: StaticStorage = table("_user_filter_instances", FD_CAPACITY, 2 * WORD);
pub const DIR_HANDLES: StaticStorage = table("_dir_handles", FD_CAPACITY, WORD);
pub const GLOB_HANDLES: StaticStorage = table("_glob_handles", FD_CAPACITY, WORD);
pub const POPEN_FILES: StaticStorage = table("_popen_files", FD_CAPACITY, WORD);
pub const ICONV_HANDLES: StaticStorage = table("_iconv_handles", FD_CAPACITY, WORD);
pub const BZIP2_HANDLES: StaticStorage = table("_bzstream_handles", FD_CAPACITY, WORD);
pub const ZLIB_HANDLES: StaticStorage = table("_zstream_handles", FD_CAPACITY, WORD);
pub const READ_FILTERS: StaticStorage = table("_stream_read_filters", FD_CAPACITY, 1);
pub const WRITE_FILTERS: StaticStorage = table("_stream_write_filters", FD_CAPACITY, 1);
pub const EOF_FLAGS: StaticStorage = table("_eof_flags", FD_CAPACITY, 1);
pub const CHUNK_SIZES: StaticStorage = table("_stream_chunk_size", FD_CAPACITY, WORD);
pub const CONNECT_HOSTS: StaticStorage = table("_stream_connect_host", FD_CAPACITY, 1 << HOST_PAIR_SHIFT);

/// Clears the request's stream side tables after request-owned resources have been closed.
pub fn emit_request_clear(emitter: &mut impl Emitter) {
    for storage in [
        USER_WRAPPERS, USER_WRAPPER_HANDLES, USER_FILTER_REGISTRY, USER_FILTER_INSTANCES,
        DIR_HANDLES, GLOB_HANDLES, POPEN_FILES, ICONV_HANDLES, BZIP2_HANDLES, ZLIB_HANDLES,
        READ_FILTERS, WRITE_FILTERS, EOF_FLAGS, CHUNK_SIZES, CONNECT_HOSTS,
    ] {
        storage.emit_clear(emitter);
    }
}

/// Addresses the host pair at a checked fd. x86_64 consumes the index register.
pub fn emit_connect_host_slot(emitter: &mut impl Emitter, base: &str, index: &str) {
    CONNECT_HOSTS.emit_address(emitter, base);
    emit_connect_host_offset(emitter, base, index);
}

/// Adds the fd offset after the caller materializes the base and reloads the index.
fn emit_connect_host_offset(emitter: &mut impl Emitter, base: &str, index: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {base}, {base}, {index}, lsl #{HOST_PAIR_SHIFT}")); // address the checked descriptor's host pair
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("shl {index}, {HOST_PAIR_SHIFT}"));    // scale the index by the host pair stride
            emitter.instruction(&format!("add {base}, {index}"));               // address the checked descriptor's host pair
        }
    }
}

/// Persists the selected host and publishes its pair using the existing stash frame.
pub fn emit_stash_connect_host(emitter: &mut impl Emitter) {
    emitter.emit_call_label("__rt_str_persist");
    let (base, index) = match emitter.arch() {
        Arch::AArch64 => ("x10", "x9"),
        Arch::X86_64 => ("r10", "r9"),
    };
    CONNECT_HOSTS.emit_address(emitter, base);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("ldr x9, [sp, #0]"),               // reload the descriptor index
        Arch::X86_64 => emitter.instruction("mov r9, QWORD PTR [rbp - 8]"),     // reload the descriptor index
    }
    emit_connect_host_offset(emitter, base, index);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x1, [x10, #0]");                           // publish the persisted host pointer
            emitter.instruction("str x2, [x10, #8]");                           // publish the persisted host length
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [r10 + 0], rax");                // publish the persisted host pointer
            emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                // publish the persisted host length
        }
    }
}

/// Detaches a closed fd's string owner and clears the scalar state associated with the fd.
pub fn emit_retire_fd_metadata(emitter: &mut impl Emitter) {
    let fd = emitter.int_result_reg();
    let base = match emitter.arch() {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r9",
    };
    emitter.emit_push_reg(fd);
    emit_connect_host_slot(emitter, base, fd);
    emitter.emit_load_from_address(fd, base, 0);
    emitter.emit_store_zero_to_address(base, 0);
    emitter.emit_store_zero_to_address(base, WORD);
    emitter.emit_call_label("__rt_heap_free_safe");
    emitter.emit_pop_reg(fd);

    for table in [READ_FILTERS, WRITE_FILTERS, EOF_FLAGS] {
        table.emit_address(emitter, base);
        emitter.instruction(match emitter.arch() {
            Arch::AArch64 => "strb wzr, [x9, x0]",
            Arch::X86_64 => "mov BYTE PTR [r9 + rax], 0",
        });                                                                     // clear one byte of descriptor state
    }
    CHUNK_SIZES.emit_address(emitter, base);
    emitter.instruction(match emitter.arch() {
        Arch::AArch64 => "str xzr, [x9, x0, lsl #3]",
        Arch::X86_64 => "mov QWORD PTR [r9 + rax * 8], 0",
    });                                                                         // clear the configured chunk size
}

/// Scans for a vacant wrapper slot, leaving the table base and index live.
pub fn emit_find_wrapper_slot(emitter: &mut impl Emitter, scan: &str, found: &str, full: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            USER_WRAPPER_HANDLES.emit_address(emitter, "x10");
            emitter.instruction("mov x12, #0");                                 // start at the first wrapper slot
            emitter.label(scan);
            emitter.instruction(&format!("cmp x12, #{WRAPPER_HANDLE_CAPACITY}")); // check whether the table is full
            emitter.instruction(&format!("b.ge {full}"));                       // let the caller retire the unpublished owner
            emitter.instruction("ldr x13, [x10, x12, lsl #3]");                 // load the wrapper owner
            emitter.instruction(&format!("cbz x13, {found}"));                  // select this vacant slot
            emitter.instruction("add x12, x12, #1");                            // advance one slot
            emitter.instruction(&format!("b {scan}"));                          // examine the next slot
        }
        Arch::X86_64 => {
            USER_WRAPPER_HANDLES.emit_address(emitter, "r10");
            emitter.instruction("xor r12, r12");                                // start at the first wrapper slot
            emitter.label(scan);
            emitter.instruction(&format!("cmp r12, {WRAPPER_HANDLE_CAPACITY}")); // check whether the table is full
            emitter.instruction(&format!("jge {full}"));                        // let the caller retire the unpublished owner
            emitter.instruction("mov r13, QWORD PTR [r10 + r12 * 8]");          // load the wrapper owner
            emitter.instruction("test r13, r13");                               // identify a vacant slot
            emitter.instruction(&format!("jz {found}"));                        // select this vacant slot
            emitter.instruction("inc r12");                                     // advance one slot
            emitter.instruction(&format!("jmp {scan}"));                        // examine the next slot
        }
    }
    emitter.label(found);
}

/// Transfers the wrapper object in the scan's owner register into the selected slot.
pub fn emit_publish_wrapper_owner(emitter: &mut impl Emitter) {
    emitter.instruction(match emitter.arch() {
        Arch::AArch64 => "str x13, [x10, x12, lsl #3]",
        Arch::X86_64 => "mov QWORD PTR [r10 + r12 * 8], r13",
    });                                                                         // publish the selected wrapper owner
}

/// Borrows a wrapper owner and leaves the table address live for an optional detach.
pub fn emit_load_wrapper_owner(
    emitter: &mut impl Emitter, base: &str, index: &str, destination: &str,
) {
    USER_WRAPPER_HANDLES.emit_address(emitter, base);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("ldr {destination}, [{base}, {index}, lsl #3]")), // borrow the selected wrapper object
        Arch::X86_64 => emitter.instruction(&format!("mov {destination}, QWORD PTR [{base} + {index} * 8]")), // borrow the selected wrapper object
    }
}

/// Clears the slot after its owner is moved to the caller's local storage.
pub fn emit_clear_wrapper_slot(emitter: &mut impl Emitter, base: &str, index: &str) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("str xzr, [{base}, {index}, lsl #3]")), // retire the wrapper's published slot
        Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [{base} + {index} * 8], 0")), // retire the wrapper's published slot
    }
}
