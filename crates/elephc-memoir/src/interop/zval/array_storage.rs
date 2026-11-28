//! Purpose:
//! Builds Zend array storage and transfers ownership into its inline slots.
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

/// Allocates and initializes the packed or hash backing area for the active packer frame.
pub fn emit_array_backing(emitter: &mut impl Emitter, storage: ArrayStorage) {
    emit_table_capacity(emitter, storage);
    match storage {
        ArrayStorage::Packed => emit_packed_backing(emitter),
        ArrayStorage::Hash => emit_hash_backing(emitter),
    }
}

/// Finds the smallest native Zend capacity covering the input, with a minimum of eight.
fn emit_table_capacity(emitter: &mut impl Emitter, storage: ArrayStorage) {
    let (search, ready) = match storage {
        ArrayStorage::Packed => ("__rt_zval_pack_array_packed_pow2", "__rt_zval_pack_array_packed_pow2_done"),
        ArrayStorage::Hash => ("__rt_zval_pack_array_hash_pow2", "__rt_zval_pack_array_hash_pow2_done"),
    };
    // -- double the candidate capacity until it covers the input count --
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x9, #8");                                  // start at the minimum hash table size
            emitter.label(search);
            emitter.instruction("ldr x1, [sp, #8]");                            // reload the count
            emitter.instruction("cmp x9, x1");                                  // is the candidate size already >= count?
            emitter.instruction(&format!("b.ge {ready}"));                      // stop doubling once the table covers the count
            emitter.instruction("lsl x9, x9, #1");                              // double the candidate size
            emitter.instruction(&format!("b {search}"));                        // retry the power-of-two search
            emitter.label(ready);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, 8");                                   // start at the minimum hash table size
            emitter.label(search);
            emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");               // reload the count
            emitter.instruction("cmp r9, rcx");                                 // is the candidate size already >= count?
            emitter.instruction(&format!("jge {ready}"));                       // stop doubling once the table covers the count
            emitter.instruction("shl r9, 1");                                   // double the candidate size
            emitter.instruction(&format!("jmp {search}"));                      // retry the power-of-two search
            emitter.label(ready);
        }
    }
}

/// Allocates two invalid index words followed by the packed payload.
fn emit_packed_backing(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x9, [sp, #40]");                           // save nTableSize

            // -- allocate the data block (8 sentinel bytes + nTableSize * 32-byte buckets) --
            emitter.instruction("lsl x0, x9, #5");                              // nTableSize * 32 bytes of bucket storage
            emitter.instruction("add x0, x0, #8");                              // add the two 4-byte sentinel slots
            emitter.instruction("bl __rt_heap_alloc");                          // x0 = data block base
            emitter.instruction("str x0, [sp, #32]");                           // save the data block pointer
            emitter.instruction("mov x9, #-1");                                 // HT_INVALID_IDX = 0xFFFFFFFF
            emitter.instruction("str w9, [x0]");                                // stamp the first packed sentinel
            emitter.instruction("str w9, [x0, #4]");                            // stamp the second packed sentinel
            emitter.instruction("add x9, x0, #8");                              // arData = data block base + 8
            emitter.instruction("str x9, [sp, #64]");                           // save arData
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rbp - 40], r9");                // save nTableSize

            // -- allocate the data block (8 sentinel bytes + nTableSize * 32-byte buckets) --
            emitter.instruction("mov rax, r9");                                 // copy nTableSize for the size computation
            emitter.instruction("shl rax, 5");                                  // nTableSize * 32 bytes of bucket storage
            emitter.instruction("add rax, 8");                                  // add the two 4-byte sentinel slots
            emitter.instruction("call __rt_heap_alloc");                        // rax = data block base
            emitter.instruction("mov QWORD PTR [rbp - 56], rax");               // save the data block pointer
            emitter.instruction("mov DWORD PTR [rax], -1");                     // stamp the first packed sentinel (HT_INVALID_IDX)
            emitter.instruction("mov DWORD PTR [rax + 4], -1");                 // stamp the second packed sentinel
            emitter.instruction("lea rcx, [rax + 8]");                          // arData = data block base + 8
            emitter.instruction("mov QWORD PTR [rbp - 64], rcx");               // save arData
        }
    }
}

/// Allocates and initializes the collision indices preceding the hash buckets.
fn emit_hash_backing(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x9, [sp, #16]");                           // save nTableSize

            // -- nTableMask = -nTableSize (sign-extended to 64-bit) --
            emitter.instruction("neg x10, x9");                                 // nTableMask as a signed 64-bit value
            emitter.instruction("str x10, [sp, #24]");                          // save nTableMask

            // -- allocate the data block: nTableSize*4 hash index bytes + nTableSize*32 bucket bytes --
            emitter.instruction("mov x0, x9");                                  // copy nTableSize for the bucket byte count
            emitter.instruction("lsl x0, x0, #5");                              // nTableSize * 32 bytes of bucket storage
            emitter.instruction("mov x1, x9");                                  // copy nTableSize for the hash index byte count
            emitter.instruction("lsl x1, x1, #2");                              // nTableSize * 4 bytes of hash index storage
            emitter.instruction("add x0, x0, x1");                              // total data block size = nTableSize * 36
            emitter.instruction("bl __rt_heap_alloc");                          // x0 = data block base (also the arHash base)
            emitter.instruction("str x0, [sp, #32]");                           // save the data block base
            emitter.instruction("ldr x9, [sp, #16]");                           // reload nTableSize
            emitter.instruction("lsl x9, x9, #2");                              // hash index size = nTableSize * 4
            emitter.instruction("add x9, x0, x9");                              // arData = base + hash index size
            emitter.instruction("str x9, [sp, #40]");                           // save arData

            // -- initialize every arHash slot to HT_INVALID_IDX (0xFFFFFFFF) --
            emitter.instruction("str xzr, [sp, #48]");                          // arHash init loop index i = 0
            emitter.label("__rt_zval_pack_array_hash_inithash");
            emitter.instruction("ldr x9, [sp, #16]");                           // reload nTableSize
            emitter.instruction("ldr x10, [sp, #48]");                          // reload the init loop index
            emitter.instruction("cmp x10, x9");                                 // has every slot been initialized?
            emitter.instruction("b.ge __rt_zval_pack_array_hash_inithash_done"); // exit once every hash slot is initialized
            emitter.instruction("ldr x11, [sp, #32]");                          // reload the data block base
            emitter.instruction("lsl x12, x10, #2");                            // i * 4 bytes per hash index slot
            emitter.instruction("add x12, x11, x12");                           // x12 = &arHash[i]
            emitter.instruction("mov w13, #-1");                                // HT_INVALID_IDX = 0xFFFFFFFF (32-bit)
            emitter.instruction("str w13, [x12]");                              // arHash[i] = HT_INVALID_IDX
            emitter.instruction("add x10, x10, #1");                            // advance to the next slot
            emitter.instruction("str x10, [sp, #48]");                          // store the incremented init index
            emitter.instruction("b __rt_zval_pack_array_hash_inithash");        // continue the hash-index init loop
            emitter.label("__rt_zval_pack_array_hash_inithash_done");
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rbp - 24], r9");                // save nTableSize

            // -- nTableMask = -nTableSize (sign-extended to 64-bit) --
            emitter.instruction("mov r10, r9");                                 // copy nTableSize
            emitter.instruction("neg r10");                                     // nTableMask = -nTableSize (unary two's-complement negate)
            emitter.instruction("mov QWORD PTR [rbp - 32], r10");               // save nTableMask

            // -- allocate the data block: nTableSize*4 hash index bytes + nTableSize*32 bucket bytes --
            emitter.instruction("mov rax, r9");                                 // copy nTableSize for the bucket byte count
            emitter.instruction("shl rax, 5");                                  // nTableSize * 32 bytes of bucket storage
            emitter.instruction("mov rcx, r9");                                 // copy nTableSize for the hash index byte count
            emitter.instruction("shl rcx, 2");                                  // nTableSize * 4 bytes of hash index storage
            emitter.instruction("add rax, rcx");                                // total data block size = nTableSize * 36
            emitter.instruction("call __rt_heap_alloc");                        // rax = data block base (also the arHash base)
            emitter.instruction("mov QWORD PTR [rbp - 40], rax");               // save the data block base
            emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                // reload nTableSize
            emitter.instruction("shl r9, 2");                                   // hash index size = nTableSize * 4
            emitter.instruction("lea r9, [rax + r9]");                          // arData = base + hash index size
            emitter.instruction("mov QWORD PTR [rbp - 48], r9");                // save arData

            // -- initialize every arHash slot to HT_INVALID_IDX (0xFFFFFFFF) --
            emitter.instruction("mov QWORD PTR [rbp - 56], 0");                 // arHash init loop index i = 0
            emitter.label("__rt_zval_pack_array_hash_inithash");
            emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                // reload nTableSize
            emitter.instruction("mov r10, QWORD PTR [rbp - 56]");               // reload the init loop index
            emitter.instruction("cmp r10, r9");                                 // has every slot been initialized?
            emitter.instruction("jge __rt_zval_pack_array_hash_inithash_done"); // exit once every hash slot is initialized
            emitter.instruction("mov r11, QWORD PTR [rbp - 40]");               // reload the data block base
            emitter.instruction("mov r12, r10");                                // copy i for the byte offset
            emitter.instruction("shl r12, 2");                                  // i * 4 bytes per hash index slot
            emitter.instruction("lea r12, [r11 + r12]");                        // r12 = &arHash[i]
            emitter.instruction("mov DWORD PTR [r12], -1");                     // arHash[i] = HT_INVALID_IDX (0xFFFFFFFF)
            emitter.instruction("inc r10");                                     // advance to the next slot
            emitter.instruction("mov QWORD PTR [rbp - 56], r10");               // store the incremented init index
            emitter.instruction("jmp __rt_zval_pack_array_hash_inithash");      // continue the hash-index init loop
            emitter.label("__rt_zval_pack_array_hash_inithash_done");
        }
    }
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
