//! Purpose:
//! Emits ownership transfer into inline Zend array slots and builds their table header.
//!
//! Called from:
//! - The native packed and associative zval packers.
//!
//! Key details:
//! - The caller selects the representation and supplies the existing native spill layout.
//! - Inline copies take ownership of the child contents before the envelope is freed.

use crate::hash_layout::NEXT_INDEX_OFFSET;
use crate::{Arch, Emitter};

/// Physical Zend array representation selected by the packer.
#[derive(Clone, Copy)]
pub enum ArrayStorage {
    /// Zend's packed flag and sentinel prefix.
    Packed,
    /// A Zend hash table with collision-index storage.
    Hash,
}

/// Transfers the two words of a child zval to its inline destination.
pub fn emit_adopt_child_words(emitter: &mut impl Emitter, dest: &str, source: &str, scratch: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {scratch}, [{source}]"));         // take the owned zval value word
            emitter.instruction(&format!("str {scratch}, [{dest}]"));           // publish the value inline
            emitter.instruction(&format!("ldr {scratch}, [{source}, #8]"));     // recover the type and auxiliary word
            emitter.instruction(&format!("str {scratch}, [{dest}, #8]"));       // publish the remaining zval word inline
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{source}]")); // take the owned zval value word
            emitter.instruction(&format!("mov QWORD PTR [{dest}], {scratch}")); // publish the value inline
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{source} + 8]")); // recover the type and auxiliary word
            emitter.instruction(&format!("mov QWORD PTR [{dest} + 8], {scratch}")); // publish the remaining word inline
        }
    }
}

/// Frees only the temporary zval envelope after its contents have moved inline.
pub fn emit_retire_child_envelope(emitter: &mut impl Emitter) {
    emitter.emit_call_label("__rt_heap_free");
}

/// Allocates a Zend HashTable and publishes ownership of the caller's packed or hash data.
pub fn emit_array_header(emitter: &mut impl Emitter, storage: ArrayStorage) {
    match emitter.arch() {
        Arch::AArch64 => emit_arm64_header(emitter, storage),
        Arch::X86_64 => emit_x86_64_header(emitter, storage),
    }
}

/// Emits the Zend HashTable header with the arm64 packers' native spill layout.
fn emit_arm64_header(emitter: &mut impl Emitter, storage: ArrayStorage) {
    let result_slot = match storage {
        ArrayStorage::Packed => 72,
        ArrayStorage::Hash => 80,
    };
    emitter.instruction("mov x0, #56");                                         // HashTable structures are 56 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = HashTable pointer
    emitter.instruction(&format!("str x0, [sp, #{result_slot}]"));              // save the new HashTable pointer
    emitter.instruction("mov x9, #1");                                          // refcount starts at one owner
    emitter.instruction("str w9, [x0]");                                        // store refcount at offset 0 (32-bit)
    emitter.instruction("mov x9, #7");                                          // gc type_info = IS_ARRAY (7)
    emitter.instruction("str w9, [x0, #4]");                                    // store gc type_info at offset 4 (32-bit)
    match storage {
        ArrayStorage::Packed => {
            emitter.instruction("mov x9, #0x60");                               // HASH_FLAG_PACKED | HASH_FLAG_STATIC_KEYS
            emitter.instruction("str w9, [x0, #8]");                            // store flags at offset 8 (32-bit)
            emitter.instruction("mov x9, #-2");                                 // HT_MIN_MASK = -2 marks a packed table
            emitter.instruction("str w9, [x0, #12]");                           // store nTableMask at offset 12 (32-bit)
            emitter.instruction("ldr x9, [sp, #32]");                           // reload the data block base
            emitter.instruction("add x9, x9, #8");                              // arData = data block base + 8
            emitter.instruction("str x9, [x0, #16]");                           // store arData at offset 16
        }
        ArrayStorage::Hash => {
            emitter.instruction("mov x9, #0x40");                               // HASH_FLAG_STATIC_KEYS (no packed flag)
            emitter.instruction("str w9, [x0, #8]");                            // store flags at offset 8 (32-bit)
            emitter.instruction("ldr x9, [sp, #24]");                           // reload nTableMask (64-bit)
            emitter.instruction("str w9, [x0, #12]");                           // store nTableMask at offset 12 (low 32 bits)
            emitter.instruction("ldr x9, [sp, #40]");                           // reload arData
            emitter.instruction("str x9, [x0, #16]");                           // store arData at offset 16
        }
    }
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the length
    emitter.instruction("str w9, [x0, #24]");                                   // store nNumUsed at offset 24 (32-bit)
    emitter.instruction("str w9, [x0, #28]");                                   // store nNumOfElements at offset 28 (32-bit)
    emitter.instruction("mov x9, #-1");                                         // HT_INVALID_IDX for the internal pointer
    emitter.instruction("str w9, [x0, #32]");                                   // store nInternalPointer at offset 32 (32-bit)
    match storage {
        ArrayStorage::Packed => {
            emitter.instruction("ldr x9, [sp, #8]");                            // reload the length for nNextFreeElement
            emitter.instruction("cbnz x9, __rt_zval_pack_array_packed_next_ready"); // nonempty packed arrays append after their final integer key
            emitter.instruction("movz x9, #0x8000, lsl #48");                   // an empty table has no integer key yet: ZEND_LONG_MIN
            emitter.label("__rt_zval_pack_array_packed_next_ready");
        }
        ArrayStorage::Hash => {
            emitter.instruction("ldr x9, [sp]");                                // recover the source hash after packing every live entry
            emitter.instruction(&format!("ldr x9, [x9, #{NEXT_INDEX_OFFSET}]")); // preserve automatic indexing, including deleted integer keys
        }
    }
    emitter.instruction("str x9, [x0, #40]");                                   // store nNextFreeElement at offset 40 (64-bit)
    emitter.instruction("str xzr, [x0, #48]");                                  // store pDestructor = NULL at offset 48
    emitter.instruction(&format!("ldr x0, [sp, #{result_slot}]"));              // restore the published HashTable pointer
}

/// Emits the Zend HashTable header with the x86_64 packers' native spill layout.
fn emit_x86_64_header(emitter: &mut impl Emitter, storage: ArrayStorage) {
    let result_slot = match storage {
        ArrayStorage::Packed => 72,
        ArrayStorage::Hash => 88,
    };
    emitter.instruction("mov rax, 56");                                         // HashTable structures are 56 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = HashTable pointer
    emitter.instruction(&format!("mov QWORD PTR [rbp - {result_slot}], rax"));  // save the new HashTable pointer
    emitter.instruction("mov DWORD PTR [rax], 1");                              // refcount = 1 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 4], 7");                          // gc type_info = IS_ARRAY (7)
    match storage {
        ArrayStorage::Packed => {
            emitter.instruction("mov DWORD PTR [rax + 8], 96");                 // HASH_FLAG_PACKED | HASH_FLAG_STATIC_KEYS = 0x60
            emitter.instruction("mov DWORD PTR [rax + 12], -2");                // nTableMask = HT_MIN_MASK = -2 (packed)
            emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");               // reload the data block base
            emitter.instruction("lea rcx, [rcx + 8]");                          // arData = data block base + 8
            emitter.instruction("mov QWORD PTR [rax + 16], rcx");               // store arData at offset 16
        }
        ArrayStorage::Hash => {
            emitter.instruction("mov DWORD PTR [rax + 8], 64");                 // HASH_FLAG_STATIC_KEYS = 0x40 (no packed flag)
            emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");               // reload nTableMask (64-bit)
            emitter.instruction("mov DWORD PTR [rax + 12], ecx");               // store nTableMask at offset 12 (low 32 bits)
            emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");               // reload arData
            emitter.instruction("mov QWORD PTR [rax + 16], rcx");               // store arData at offset 16
        }
    }
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the length
    emitter.instruction("mov DWORD PTR [rax + 24], ecx");                       // store nNumUsed at offset 24 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 28], ecx");                       // store nNumOfElements at offset 28 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 32], -1");                        // nInternalPointer = HT_INVALID_IDX (32-bit)
    match storage {
        ArrayStorage::Packed => {
            emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");               // reload the length for nNextFreeElement
            emitter.instruction("test rcx, rcx");                               // distinguish an empty table from one ending at key length - 1
            emitter.instruction("jnz __rt_zval_pack_array_packed_next_ready");  // nonempty packed arrays append at their element count
            emitter.instruction("mov rcx, 1");                                  // materialize ZEND_LONG_MIN without an out-of-range immediate
            emitter.instruction("shl rcx, 63");                                 // an empty table has no integer key yet
            emitter.label("__rt_zval_pack_array_packed_next_ready");
        }
        ArrayStorage::Hash => {
            emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                // recover the source hash after packing every live entry
            emitter.instruction(&format!("mov rcx, QWORD PTR [rcx + {NEXT_INDEX_OFFSET}]")); // preserve automatic indexing and deleted-key history
        }
    }
    emitter.instruction("mov QWORD PTR [rax + 40], rcx");                       // store nNextFreeElement at offset 40 (64-bit)
    emitter.instruction("mov QWORD PTR [rax + 48], 0");                         // pDestructor = NULL
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {result_slot}]"));  // restore the published HashTable pointer
}
