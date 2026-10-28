//! Purpose:
//! `HashTable`: PHP's ordered dictionary, which is every PHP array and every
//! engine registry (functions, classes, constants, INI directives).
//!
//! Called from:
//! - Extension code (`zend_hash_*`, `zend_symtable_*`, the `ZEND_HASH_FOREACH`
//!   macros that walk `arData` directly), and every engine module.
//!
//! Key details:
//! - Memory layout is PHP's: one block holding `2 * nTableSize` u32 hash slots
//!   followed by `nTableSize` buckets, with `arData` pointing at the first
//!   bucket and `nTableMask = -(2 * nTableSize)`. A key's slot is
//!   `arHash[(int32)(h | nTableMask)]`, collisions chain through the bucket
//!   zval's `u2.next`. APCu and igbinary copy that block by hand.
//! - Tables are never packed: the packed form is a memory optimisation PHP
//!   applies to lists, and no extension behaviour depends on it, so a list is
//!   an ordinary hashed table with integer keys (`HT_IS_PACKED` is 0).
//! - A fresh table is uninitialized (`HASH_FLAG_UNINITIALIZED`, `arData`
//!   pointing past a static pair of empty slots) until its first insert, as
//!   in PHP.
//! - Insertion order is kept; deletions leave `IS_UNDEF` holes that the next
//!   resize compacts. Integer keys advance `nNextFreeElement` as in PHP.

use std::ffi::{c_char, c_int, c_void};
use std::ptr;

use crate::abi::*;
use crate::alloc::{alloc, free};
use crate::string::{self, bytes, hash_bytes, is_interned, zend_string_hash_val, zend_string_release};
use crate::zval::{copy_value, zval_ptr_dtor};

/// What an uninitialized table's `arData` points just past: two empty slots,
/// so a lookup through `nTableMask = HT_MIN_MASK` finds nothing.
static mut UNINITIALIZED_BUCKET: [u32; 2] = [HT_INVALID_IDX, HT_INVALID_IDX];

// ------------------------------------------------------------ layout helpers

/// Returns the pointer to the first bucket (`arData`) of an initialized or uninitialized table.
pub unsafe fn ar_data(ht: *const HashTable) -> *mut Bucket {
    (*ht).data.arData
}

/// Returns the address of the hash slot (`arHash` entry) for a negative-mask index `n_index`, i.e.
/// the head of a collision chain.
unsafe fn slot(ht: *const HashTable, n_index: u32) -> *mut u32 {
    (*ht).data.arHash.offset(n_index as i32 as isize)
}

/// Returns the byte size of the u32 hash-slot area implied by `nTableMask` (`-mask` slots of 4
/// bytes).
fn hash_size(mask: u32) -> usize {
    (mask.wrapping_neg() as usize) * std::mem::size_of::<u32>()
}

/// Returns the start of the whole allocation (slot area followed by buckets) by stepping back from
/// `arData` over the hash slots; this is the pointer to free.
unsafe fn data_block(ht: *const HashTable) -> *mut c_void {
    (*ht).data.arData.cast::<u8>().sub(hash_size((*ht).nTableMask)).cast()
}

/// True once the table has allocated storage, i.e. `HASH_FLAG_UNINITIALIZED` is clear.
pub unsafe fn is_initialized(ht: *const HashTable) -> bool {
    (*ht).u.flags & HASH_FLAG_UNINITIALIZED == 0
}

/// Normalises a requested table size: clamps to `HT_MIN_SIZE`, rounds up to a power of two, and
/// raises a fatal error when it reaches `HT_MAX_SIZE`.
fn check_size(size: u32) -> u32 {
    if size <= HT_MIN_SIZE {
        return HT_MIN_SIZE;
    }
    if size >= HT_MAX_SIZE {
        crate::error::fatal(&format!(
            "Possible integer overflow in memory allocation ({size} * {} + {})",
            std::mem::size_of::<Bucket>(),
            std::mem::size_of::<Bucket>()
        ));
    }
    size.next_power_of_two()
}

/// Returns the address of bucket number `idx` in the table's bucket array (no bounds check).
pub unsafe fn bucket(ht: *const HashTable, idx: u32) -> *mut Bucket {
    ar_data(ht).add(idx as usize)
}

/// True when the bucket holds an `IS_UNDEF` value, i.e. it is a deleted hole.
unsafe fn is_undef(p: *const Bucket) -> bool {
    (*p).val.u1.v.r#type == IS_UNDEF
}

/// Allocates storage for `size` buckets with all slots empty.
unsafe fn allocate(ht: *mut HashTable, size: u32) {
    let mask = (size as u32).wrapping_mul(2).wrapping_neg();
    // A request table's storage goes with its request when nobody destroyed the table.
    let persistent = (*ht).gc.u.type_info & GC_PERSISTENT != 0;
    let block = crate::alloc::record_unless_persistent(
        alloc(hash_size(mask) + size as usize * std::mem::size_of::<Bucket>()),
        persistent,
    );
    ptr::write_bytes(block.cast::<u8>(), 0xff, hash_size(mask));
    (*ht).nTableMask = mask;
    (*ht).nTableSize = size;
    (*ht).data.arData = block.cast::<u8>().add(hash_size(mask)).cast();
}

/// Allocates storage for an uninitialized table (at least `HT_MIN_SIZE` buckets) and clears the
/// uninitialized flag; no-op when already initialized.
unsafe fn real_init(ht: *mut HashTable) {
    if is_initialized(ht) {
        return;
    }
    let size = (*ht).nTableSize.max(HT_MIN_SIZE);
    allocate(ht, size);
    (*ht).u.flags &= !HASH_FLAG_UNINITIALIZED;
}

/// Relinks every bucket's chain, compacting holes first when `compact`.
unsafe fn rehash(ht: *mut HashTable, compact: bool) {
    let mask = (*ht).nTableMask;
    ptr::write_bytes(data_block(ht).cast::<u8>(), 0xff, hash_size(mask));
    let mut write = 0u32;
    for read in 0..(*ht).nNumUsed {
        let p = bucket(ht, read);
        if is_undef(p) {
            if !compact {
                continue;
            }
            continue;
        }
        let target = if compact { write } else { read };
        if compact && target != read {
            ptr::copy_nonoverlapping(p, bucket(ht, target), 1);
            if (*ht).nInternalPointer == read {
                (*ht).nInternalPointer = target;
            }
        }
        let q = bucket(ht, target);
        let n_index = ((*q).h as u32) | mask;
        (*q).val.u2.next = *slot(ht, n_index);
        *slot(ht, n_index) = target;
        write = target + 1;
    }
    if compact {
        if (*ht).nInternalPointer >= (*ht).nNumUsed || (*ht).nInternalPointer >= write {
            (*ht).nInternalPointer = (*ht).nInternalPointer.min(write);
        }
        (*ht).nNumUsed = write;
    }
}

/// Makes room for one more bucket.
unsafe fn grow(ht: *mut HashTable) {
    if !is_initialized(ht) {
        real_init(ht);
        return;
    }
    if (*ht).nNumUsed < (*ht).nTableSize {
        return;
    }
    let used = (*ht).nNumUsed;
    let live = (*ht).nNumOfElements;
    if used > live + (live >> 5) {
        rehash(ht, true);
        return;
    }
    let old_block = data_block(ht);
    let old_data = ar_data(ht);
    let new_size = check_size((*ht).nTableSize * 2);
    allocate(ht, new_size);
    ptr::copy_nonoverlapping(old_data, ar_data(ht), used as usize);
    free(old_block);
    rehash(ht, false);
}

// ------------------------------------------------------------ lookup

/// Looks up a string key given as raw bytes and its precomputed hash; walks the collision chain and
/// returns the matching bucket or null (also null for an uninitialized table).
unsafe fn find_bucket_str(ht: *const HashTable, key: &[u8], h: zend_ulong) -> *mut Bucket {
    if !is_initialized(ht) {
        return ptr::null_mut();
    }
    let mut idx = *slot(ht, (h as u32) | (*ht).nTableMask);
    while idx != HT_INVALID_IDX {
        let p = bucket(ht, idx);
        if (*p).h == h && !(*p).key.is_null() && bytes((*p).key) == key {
            return p;
        }
        idx = (*p).val.u2.next;
    }
    ptr::null_mut()
}

/// Looks up a `zend_string` key (hashing it if needed), matching by pointer identity or by hash
/// plus byte equality; returns the bucket or null.
unsafe fn find_bucket_key(ht: *const HashTable, key: *mut zend_string) -> *mut Bucket {
    if !is_initialized(ht) {
        return ptr::null_mut();
    }
    let h = zend_string_hash_val(key);
    let mut idx = *slot(ht, (h as u32) | (*ht).nTableMask);
    while idx != HT_INVALID_IDX {
        let p = bucket(ht, idx);
        if (*p).key == key || ((*p).h == h && !(*p).key.is_null() && bytes((*p).key) == bytes(key)) {
            return p;
        }
        idx = (*p).val.u2.next;
    }
    ptr::null_mut()
}

/// Looks up an integer key `h` (a bucket with a null string key and equal hash); returns the bucket
/// or null.
unsafe fn find_bucket_index(ht: *const HashTable, h: zend_ulong) -> *mut Bucket {
    if !is_initialized(ht) {
        return ptr::null_mut();
    }
    let mut idx = *slot(ht, (h as u32) | (*ht).nTableMask);
    while idx != HT_INVALID_IDX {
        let p = bucket(ht, idx);
        if (*p).h == h && (*p).key.is_null() {
            return p;
        }
        idx = (*p).val.u2.next;
    }
    ptr::null_mut()
}

// ------------------------------------------------------------ insertion

/// Appends a new bucket. `key` is stored as given (the caller already holds
/// the reference the table keeps), or NULL for an integer key.
unsafe fn append(ht: *mut HashTable, key: *mut zend_string, h: zend_ulong, data: *const zval) -> *mut zval {
    grow(ht);
    let idx = (*ht).nNumUsed;
    (*ht).nNumUsed += 1;
    (*ht).nNumOfElements += 1;
    let p = bucket(ht, idx);
    (*p).key = key;
    (*p).h = h;
    copy_value(&mut (*p).val, data);
    let n_index = (h as u32) | (*ht).nTableMask;
    (*p).val.u2.next = *slot(ht, n_index);
    *slot(ht, n_index) = idx;
    if key.is_null() {
        let h = h as zend_long;
        if h >= (*ht).nNextFreeElement {
            (*ht).nNextFreeElement = if h < ZEND_LONG_MAX { h + 1 } else { ZEND_LONG_MAX };
        }
    }
    &mut (*p).val
}

/// Replaces the value of an existing entry (`HASH_UPDATE`).
unsafe fn overwrite(ht: *mut HashTable, data: *mut zval, value: *const zval, indirect: bool) -> *mut zval {
    let mut target = data;
    if indirect && (*target).u1.v.r#type == IS_INDIRECT {
        target = (*target).value.zv;
    }
    if let Some(destructor) = (*ht).pDestructor {
        let mut old = zval::UNDEF;
        copy_value(&mut old, target);
        copy_value(target, value);
        destructor(&mut old);
    } else {
        copy_value(target, value);
    }
    target
}

/// An add that found its key already present: php-src fills the slot only for
/// `HASH_ADD | HASH_UPDATE_INDIRECT` on an `IS_INDIRECT` slot whose target is undefined (an unset
/// symbol-table variable) and returns that target; any other add returns null.
unsafe fn add_into_indirect(slot: *mut zval, data: *mut zval, flag: u32) -> *mut zval {
    if flag & HASH_ADD != 0 && flag & HASH_UPDATE_INDIRECT != 0 && (*slot).u1.v.r#type == IS_INDIRECT {
        let inner = (*slot).value.zv;
        if (*inner).u1.v.r#type == IS_UNDEF {
            copy_value(inner, data);
            return inner;
        }
    }
    ptr::null_mut()
}

/// Shared implementation of the string-key insert family: depending on `flag` (`HASH_ADD`,
/// `HASH_ADD_NEW`, `HASH_LOOKUP`, `HASH_UPDATE`, `HASH_UPDATE_INDIRECT`) it fills an existing
/// indirect slot, returns the existing value, overwrites it, or appends a new bucket (taking a
/// reference on a non-interned key). Returns the value slot or null when an add finds the key
/// already present.
unsafe fn add_or_update_key(ht: *mut HashTable, key: *mut zend_string, data: *mut zval, flag: u32) -> *mut zval {
    let existing = find_bucket_key(ht, key);
    if !existing.is_null() {
        if flag & (HASH_ADD | HASH_ADD_NEW) != 0 {
            return add_into_indirect(&mut (*existing).val, data, flag);
        }
        if flag & HASH_LOOKUP != 0 {
            return &mut (*existing).val;
        }
        return overwrite(ht, &mut (*existing).val, data, flag & HASH_UPDATE_INDIRECT != 0);
    }
    if !is_interned(key) {
        (*key).gc.refcount += 1;
    }
    zend_string_hash_val(key);
    append(ht, key, (*key).h, data)
}

/// Shared implementation of the integer-key insert family: an existing key makes add flags return
/// null, `HASH_LOOKUP` return the slot, and updates overwrite; otherwise a new bucket is appended.
unsafe fn add_or_update_index(ht: *mut HashTable, h: zend_ulong, data: *mut zval, flag: u32) -> *mut zval {
    let existing = find_bucket_index(ht, h);
    if !existing.is_null() {
        if flag & (HASH_ADD | HASH_ADD_NEW | HASH_ADD_NEXT) != 0 {
            return ptr::null_mut();
        }
        if flag & HASH_LOOKUP != 0 {
            return &mut (*existing).val;
        }
        return overwrite(ht, &mut (*existing).val, data, false);
    }
    append(ht, ptr::null_mut(), h, data)
}

/// A key string for a table: persistent tables get persistent keys.
unsafe fn key_string(ht: *const HashTable, str: *const c_char, len: usize) -> *mut zend_string {
    let persistent = (*ht).gc.u.type_info & GC_PERSISTENT != 0;
    string::zend_string_init(str, len, persistent)
}

/// Builds a byte slice from a C pointer and length, returning an empty slice for length 0 so a null
/// or dangling pointer is never dereferenced.
unsafe fn cbytes<'a>(str: *const c_char, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(str.cast(), len)
    }
}

// ------------------------------------------------------------ deletion

/// Removes bucket `p` (at index `idx`) from its collision chain in the hash slot area; the bucket
/// itself is left in place.
unsafe fn unlink(ht: *mut HashTable, idx: u32, p: *mut Bucket) {
    let n_index = ((*p).h as u32) | (*ht).nTableMask;
    let mut i = *slot(ht, n_index);
    if i == idx {
        *slot(ht, n_index) = (*p).val.u2.next;
    } else {
        while i != HT_INVALID_IDX {
            let prev = bucket(ht, i);
            if (*prev).val.u2.next == idx {
                (*prev).val.u2.next = (*p).val.u2.next;
                break;
            }
            i = (*prev).val.u2.next;
        }
    }
}

/// Removes the entry in bucket `p`, releasing its key and value.
pub unsafe fn delete_bucket(ht: *mut HashTable, p: *mut Bucket) {
    let idx = p.offset_from(ar_data(ht)) as u32;
    unlink(ht, idx, p);
    (*ht).nNumOfElements -= 1;
    if (*ht).nInternalPointer == idx {
        let mut next = idx + 1;
        while next < (*ht).nNumUsed && is_undef(bucket(ht, next)) {
            next += 1;
        }
        (*ht).nInternalPointer = next;
    }
    let key = (*p).key;
    (*p).key = ptr::null_mut();
    let mut old = zval::UNDEF;
    copy_value(&mut old, &(*p).val);
    (*p).val.u1.type_info = IS_UNDEF as u32;
    if idx + 1 == (*ht).nNumUsed {
        let mut used = idx;
        while used > 0 && is_undef(bucket(ht, used - 1)) {
            used -= 1;
        }
        (*ht).nNumUsed = used;
        if (*ht).nInternalPointer > used {
            (*ht).nInternalPointer = used;
        }
    }
    if !key.is_null() {
        zend_string_release(key);
    }
    if let Some(destructor) = (*ht).pDestructor {
        destructor(&mut old);
    }
}

// ------------------------------------------------------------ init and destroy

/// Initializes a caller-owned `HashTable` as an empty, uninitialized array or registry: refcount 1,
/// `GC_ARRAY` type info (persistent tables also get `GC_PERSISTENT`), no storage allocated,
/// `nNextFreeElement` unset, and the element destructor and normalised capacity recorded.
#[no_mangle]
pub unsafe extern "C" fn _zend_hash_init(ht: *mut HashTable, nSize: u32, pDestructor: dtor_func_t, persistent: bool) {
    (*ht).gc.refcount = 1;
    (*ht).gc.u.type_info = GC_ARRAY | if persistent { GC_PERSISTENT | GC_NOT_COLLECTABLE } else { 0 };
    (*ht).u.flags = HASH_FLAG_UNINITIALIZED;
    (*ht).nTableMask = HT_MIN_MASK;
    (*ht).data.arData = ptr::addr_of_mut!(UNINITIALIZED_BUCKET).cast::<u8>().add(8).cast();
    (*ht).nNumUsed = 0;
    (*ht).nNumOfElements = 0;
    (*ht).nInternalPointer = 0;
    (*ht).nNextFreeElement = ZEND_LONG_MIN;
    (*ht).pDestructor = pDestructor;
    (*ht).nTableSize = check_size(nSize);
}

/// `zend_hash_init` with the destructor only (the hash function argument PHP
/// once had is gone).
pub unsafe fn init(ht: *mut HashTable, size: u32, destructor: dtor_func_t, persistent: bool) {
    _zend_hash_init(ht, size, destructor, persistent);
}

/// Forces allocation of the table's storage; the `packed` hint is ignored because tables are never
/// packed.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_real_init(ht: *mut HashTable, _packed: bool) {
    real_init(ht);
}

/// Forces allocation of the table's storage for a mixed (hashed) table.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_real_init_mixed(ht: *mut HashTable) {
    real_init(ht);
}

/// Forces allocation of the table's storage; packed tables are not used, so this allocates the
/// ordinary hashed form.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_real_init_packed(ht: *mut HashTable) {
    real_init(ht);
}

/// No-op: tables are never packed, so there is nothing to convert.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_packed_to_hash(_ht: *mut HashTable) {}

/// No-op: this implementation never uses the packed representation.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_to_packed(_ht: *mut HashTable) {}

/// Makes room for one more bucket (growing or compacting as `grow` decides); stands in for PHP's
/// packed-array growth.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_packed_grow(ht: *mut HashTable) {
    grow(ht);
}

/// Ensures capacity for at least `nSize` buckets: sets the size of an uninitialized table before
/// allocating it, or reallocates and rehashes an initialized table that is too small. The `packed`
/// hint is ignored.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_extend(ht: *mut HashTable, nSize: u32, _packed: bool) {
    if !is_initialized(ht) {
        (*ht).nTableSize = check_size(nSize);
        real_init(ht);
        return;
    }
    if nSize <= (*ht).nTableSize {
        return;
    }
    let used = (*ht).nNumUsed;
    let old_block = data_block(ht);
    let old_data = ar_data(ht);
    allocate(ht, check_size(nSize));
    ptr::copy_nonoverlapping(old_data, ar_data(ht), used as usize);
    free(old_block);
    rehash(ht, false);
}

/// Drops trailing buckets until `nNumUsed` equals the given value, unlinking live ones from their
/// chains and decrementing the element count without running destructors.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_discard(ht: *mut HashTable, nNumUsed: u32) {
    while (*ht).nNumUsed > nNumUsed {
        let p = bucket(ht, (*ht).nNumUsed - 1);
        (*ht).nNumUsed -= 1;
        if is_undef(p) {
            continue;
        }
        (*ht).nNumOfElements -= 1;
        unlink(ht, (*ht).nNumUsed, p);
    }
}

/// Compacts deleted holes and rebuilds all collision chains of an initialized table.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_rehash(ht: *mut HashTable) {
    if is_initialized(ht) {
        rehash(ht, true);
    }
}

/// Releases every key and value; leaves the table empty but allocated.
unsafe fn release_entries(ht: *mut HashTable) {
    for idx in 0..(*ht).nNumUsed {
        let p = bucket(ht, idx);
        if is_undef(p) {
            continue;
        }
        if let Some(destructor) = (*ht).pDestructor {
            destructor(&mut (*p).val);
        }
        if !(*p).key.is_null() {
            zend_string_release((*p).key);
        }
    }
}

/// Releases all keys and values (running the destructor), frees the storage block, and returns the
/// table to the empty uninitialized state; the `HashTable` struct itself is not freed.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_destroy(ht: *mut HashTable) {
    if !is_initialized(ht) {
        return;
    }
    release_entries(ht);
    free_storage(ht);
}

/// Frees the storage of a table whose entries are all gone and returns it to the empty
/// uninitialized state, as the end of `zend_hash_destroy` does.
unsafe fn free_storage(ht: *mut HashTable) {
    free(data_block(ht));
    (*ht).u.flags |= HASH_FLAG_UNINITIALIZED;
    (*ht).nTableMask = HT_MIN_MASK;
    (*ht).data.arData = ptr::addr_of_mut!(UNINITIALIZED_BUCKET).cast::<u8>().add(8).cast();
    (*ht).nNumUsed = 0;
    (*ht).nNumOfElements = 0;
}

/// `zend_hash_graceful_destroy`: deletes the entries one at a time in order, each bucket
/// undefined before its destructor runs and `nNumUsed` re-read after each, so a destructor that
/// reads or deletes from the same table sees a consistent one (php-src's reentrant variant).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_graceful_destroy(ht: *mut HashTable) {
    if !is_initialized(ht) {
        return;
    }
    let mut idx = 0;
    while idx < (*ht).nNumUsed {
        let p = bucket(ht, idx);
        if !is_undef(p) {
            delete_bucket(ht, p);
        }
        idx += 1;
    }
    free_storage(ht);
}

/// `zend_hash_graceful_reverse_destroy`: like `zend_hash_graceful_destroy`, from the last entry
/// to the first.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_graceful_reverse_destroy(ht: *mut HashTable) {
    if !is_initialized(ht) {
        return;
    }
    let mut idx = (*ht).nNumUsed;
    while idx > 0 {
        idx -= 1;
        // A destructor may have deleted later entries and lowered `nNumUsed`.
        if idx >= (*ht).nNumUsed {
            continue;
        }
        let p = bucket(ht, idx);
        if !is_undef(p) {
            delete_bucket(ht, p);
        }
    }
    free_storage(ht);
}

/// Releases every entry but keeps the allocated storage, resetting counters, internal pointer and
/// next free index so the table is empty and reusable.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_clean(ht: *mut HashTable) {
    if !is_initialized(ht) {
        return;
    }
    release_entries(ht);
    ptr::write_bytes(data_block(ht).cast::<u8>(), 0xff, hash_size((*ht).nTableMask));
    (*ht).nNumUsed = 0;
    (*ht).nNumOfElements = 0;
    (*ht).nNextFreeElement = ZEND_LONG_MIN;
    (*ht).nInternalPointer = 0;
}

/// Empties a symbol table; identical to `zend_hash_clean`.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_clean(ht: *mut HashTable) {
    zend_hash_clean(ht);
}

/// A new PHP array (values released with `zval_ptr_dtor`).
#[no_mangle]
pub unsafe extern "C" fn _zend_new_array(size: u32) -> *mut HashTable {
    let ht = crate::alloc::record_unless_persistent(crate::alloc::alloc_struct::<HashTable>().cast(), false)
        .cast::<HashTable>();
    _zend_hash_init(ht, size, Some(zval_ptr_dtor), false);
    ht
}

/// Allocates a new empty PHP array with the default capacity.
#[no_mangle]
pub unsafe extern "C" fn _zend_new_array_0() -> *mut HashTable {
    _zend_new_array(0)
}

/// Allocates a new empty PHP array sized for `size` elements (public wrapper of `_zend_new_array`).
#[no_mangle]
pub unsafe extern "C" fn zend_new_array(size: u32) -> *mut HashTable {
    _zend_new_array(size)
}

/// Allocates a two-element list array holding `val1` then `val2` (copied in with their references
/// moved as given).
#[no_mangle]
pub unsafe extern "C" fn zend_new_pair(val1: *const zval, val2: *const zval) -> *mut HashTable {
    let ht = _zend_new_array(2);
    zend_hash_next_index_insert_new(ht, val1 as *mut zval);
    zend_hash_next_index_insert_new(ht, val2 as *mut zval);
    ht
}

/// Destroys an array's entries and storage and frees the `HashTable` allocation itself, ignoring
/// its refcount.
#[no_mangle]
pub unsafe extern "C" fn zend_array_destroy(ht: *mut HashTable) {
    zend_hash_destroy(ht);
    free(ht.cast());
}

/// Drops one reference to an array (no-op for immutable arrays) and destroys it when the count
/// reaches zero.
#[no_mangle]
pub unsafe extern "C" fn zend_array_release(array: *mut zend_array) {
    if (*array).gc.u.type_info & GC_IMMUTABLE != 0 {
        return;
    }
    (*array).gc.refcount -= 1;
    if (*array).gc.refcount == 0 {
        zend_array_destroy(array);
    }
}

/// PHP's copy-on-write duplicate: a new array with the same entries, each
/// value one more reference (a reference held only by this array becomes its
/// value, as PHP does).
#[no_mangle]
pub unsafe extern "C" fn zend_array_dup(source: *mut HashTable) -> *mut HashTable {
    let target = _zend_new_array((*source).nNumOfElements);
    for idx in 0..(*source).nNumUsed {
        let p = bucket(source, idx);
        if is_undef(p) {
            continue;
        }
        let mut value: *mut zval = &mut (*p).val;
        if (*value).u1.v.r#type == IS_INDIRECT {
            value = (*value).value.zv;
            if (*value).u1.v.r#type == IS_UNDEF {
                continue;
            }
        }
        if (*value).u1.v.r#type == IS_REFERENCE && (*(*value).value.r#ref).gc.refcount == 1 {
            let inner: *mut zval = &mut (*(*value).value.r#ref).val;
            if !((*inner).u1.v.r#type == IS_ARRAY && (*inner).value.arr == source) {
                value = inner;
            }
        }
        let mut copy = zval::UNDEF;
        crate::zval::copy(&mut copy, value);
        if (*p).key.is_null() {
            append(target, ptr::null_mut(), (*p).h, &copy);
        } else {
            let key = (*p).key;
            if !is_interned(key) {
                (*key).gc.refcount += 1;
            }
            append(target, key, (*p).h, &copy);
        }
    }
    (*target).nNextFreeElement = (*source).nNextFreeElement;
    target
}

/// Returns the number of live elements in the array.
#[no_mangle]
pub unsafe extern "C" fn zend_array_count(ht: *mut HashTable) -> u32 {
    (*ht).nNumOfElements
}

/// Returns a new list array with the values of `source` re-keyed 0..n-1 in order, each value copied
/// (reference-counted).
#[no_mangle]
pub unsafe extern "C" fn zend_array_to_list(source: *const HashTable) -> *mut HashTable {
    let target = _zend_new_array((*source).nNumOfElements);
    for idx in 0..(*source).nNumUsed {
        let p = bucket(source, idx);
        if is_undef(p) {
            continue;
        }
        // As php-src, a reference nothing else holds is listed by its value.
        let mut entry = &mut (*p).val as *mut zval;
        if (*entry).u1.v.r#type == IS_REFERENCE && (*(*entry).value.r#ref).gc.refcount == 1 {
            entry = &mut (*(*entry).value.r#ref).val;
        }
        let mut copy = zval::UNDEF;
        crate::zval::copy(&mut copy, entry);
        zend_hash_next_index_insert_new(target, &mut copy);
    }
    target
}

/// True when the array's keys are exactly the integers 0..n-1 in order (the semantics of PHP's
/// `array_is_list`).
#[no_mangle]
pub unsafe extern "C" fn zend_array_is_list(array: *const zend_array) -> bool {
    let mut expected: zend_ulong = 0;
    for idx in 0..(*array).nNumUsed {
        let p = bucket(array, idx);
        if is_undef(p) {
            continue;
        }
        if !(*p).key.is_null() || (*p).h != expected {
            return false;
        }
        expected += 1;
    }
    true
}

/// Inserts every entry of `source` into `target` (overwriting existing keys, dereferencing indirect
/// slots) and calls the optional copy constructor on each stored value.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_copy(target: *mut HashTable, source: *const HashTable, pCopyConstructor: copy_ctor_func_t) {
    for idx in 0..(*source).nNumUsed {
        let p = bucket(source, idx);
        if is_undef(p) {
            continue;
        }
        let mut data: *mut zval = &mut (*p).val;
        if (*data).u1.v.r#type == IS_INDIRECT {
            data = (*data).value.zv;
            if (*data).u1.v.r#type == IS_UNDEF {
                continue;
            }
        }
        let new = if (*p).key.is_null() {
            zend_hash_index_update(target, (*p).h, data)
        } else {
            zend_hash_update(target, (*p).key, data)
        };
        if let Some(ctor) = pCopyConstructor {
            ctor(new);
        }
    }
}

/// Inserts entries of `source` into `target`, overwriting existing keys when `overwrite` is true
/// and keeping them otherwise; the copy constructor runs on each newly stored value.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_merge(
    target: *mut HashTable,
    source: *const HashTable,
    pCopyConstructor: copy_ctor_func_t,
    overwrite: bool,
) {
    for idx in 0..(*source).nNumUsed {
        let p = bucket(source, idx);
        if is_undef(p) {
            continue;
        }
        let data: *mut zval = &mut (*p).val;
        let new = if (*p).key.is_null() {
            if overwrite {
                zend_hash_index_update(target, (*p).h, data)
            } else {
                zend_hash_index_add(target, (*p).h, data)
            }
        } else if overwrite {
            zend_hash_update(target, (*p).key, data)
        } else {
            zend_hash_add(target, (*p).key, data)
        };
        if !new.is_null() {
            if let Some(ctor) = pCopyConstructor {
                ctor(new);
            }
        }
    }
}

/// Like `zend_hash_merge` with overwrite, but each source entry is first offered to the
/// `pMergeSource` filter (with its key and `pParam`) and skipped when the filter rejects it.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_merge_ex(
    target: *mut HashTable,
    source: *const HashTable,
    pCopyConstructor: copy_ctor_func_t,
    pMergeSource: merge_checker_func_t,
    pParam: *mut c_void,
) {
    for idx in 0..(*source).nNumUsed {
        let p = bucket(source, idx);
        if is_undef(p) {
            continue;
        }
        let mut key = zend_hash_key { h: (*p).h, key: (*p).key };
        let accept = match pMergeSource {
            Some(check) => check(target, &mut (*p).val, &mut key, pParam),
            None => true,
        };
        if !accept {
            continue;
        }
        let new = if (*p).key.is_null() {
            zend_hash_index_update(target, (*p).h, &mut (*p).val)
        } else {
            zend_hash_update(target, (*p).key, &mut (*p).val)
        };
        if let Some(ctor) = pCopyConstructor {
            ctor(new);
        }
    }
}

// ------------------------------------------------------------ string keys

/// Public entry for inserting by `zend_string` key with an explicit `HASH_*` flag; returns the
/// value slot or null if an add collides.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_add_or_update(ht: *mut HashTable, key: *mut zend_string, pData: *mut zval, flag: u32) -> *mut zval {
    add_or_update_key(ht, key, pData, flag)
}

/// Inserts or replaces the value stored under a `zend_string` key; returns the value slot.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_update(ht: *mut HashTable, key: *mut zend_string, pData: *mut zval) -> *mut zval {
    add_or_update_key(ht, key, pData, HASH_UPDATE)
}

/// Like `zend_hash_update`, but writes through an `IS_INDIRECT` slot to the zval it points at (used
/// by symbol tables).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_update_ind(ht: *mut HashTable, key: *mut zend_string, pData: *mut zval) -> *mut zval {
    add_or_update_key(ht, key, pData, HASH_UPDATE | HASH_UPDATE_INDIRECT)
}

/// Inserts under a `zend_string` key only if the key is absent; returns null when it already
/// exists.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_add(ht: *mut HashTable, key: *mut zend_string, pData: *mut zval) -> *mut zval {
    add_or_update_key(ht, key, pData, HASH_ADD)
}

/// Inserts under a `zend_string` key the caller knows is absent; behaves like `zend_hash_add` here.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_add_new(ht: *mut HashTable, key: *mut zend_string, pData: *mut zval) -> *mut zval {
    add_or_update_key(ht, key, pData, HASH_ADD_NEW)
}

/// Returns the value slot for a `zend_string` key, creating it with a null value when missing.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_lookup(ht: *mut HashTable, key: *mut zend_string) -> *mut zval {
    let mut null = zval::UNDEF;
    null.u1.type_info = IS_NULL as u32;
    add_or_update_key(ht, key, &mut null, HASH_LOOKUP)
}

/// Adds a null value under a `zend_string` key if the key is absent; returns the slot or null.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_add_empty_element(ht: *mut HashTable, key: *mut zend_string) -> *mut zval {
    let mut null = zval::UNDEF;
    null.u1.type_info = IS_NULL as u32;
    zend_hash_add(ht, key, &mut null)
}

/// String-key insert for a C string plus length, with an explicit `HASH_*` flag: hashes the bytes,
/// handles existing entries per the flag, and otherwise allocates a key string (persistent if the
/// table is) and appends. Returns the value slot or null on an add collision.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_add_or_update(
    ht: *mut HashTable,
    str: *const c_char,
    len: usize,
    pData: *mut zval,
    flag: u32,
) -> *mut zval {
    let h = hash_bytes(cbytes(str, len));
    let existing = find_bucket_str(ht, cbytes(str, len), h);
    if !existing.is_null() {
        if flag & (HASH_ADD | HASH_ADD_NEW) != 0 {
            return add_into_indirect(&mut (*existing).val, pData, flag);
        }
        if flag & HASH_LOOKUP != 0 {
            return &mut (*existing).val;
        }
        return overwrite(ht, &mut (*existing).val, pData, flag & HASH_UPDATE_INDIRECT != 0);
    }
    let key = key_string(ht, str, len);
    (*key).h = h;
    append(ht, key, h, pData)
}

/// Inserts or replaces the value under a C-string key of given length.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_update(ht: *mut HashTable, str: *const c_char, len: usize, pData: *mut zval) -> *mut zval {
    zend_hash_str_add_or_update(ht, str, len, pData, HASH_UPDATE)
}

/// Like `zend_hash_str_update`, but writes through an `IS_INDIRECT` slot.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_update_ind(
    ht: *mut HashTable,
    str: *const c_char,
    len: usize,
    pData: *mut zval,
) -> *mut zval {
    zend_hash_str_add_or_update(ht, str, len, pData, HASH_UPDATE | HASH_UPDATE_INDIRECT)
}

/// Inserts under a C-string key only if absent; returns null when it exists.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_add(ht: *mut HashTable, str: *const c_char, len: usize, pData: *mut zval) -> *mut zval {
    zend_hash_str_add_or_update(ht, str, len, pData, HASH_ADD)
}

/// Inserts under a C-string key known to be absent; behaves like `zend_hash_str_add`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_add_new(ht: *mut HashTable, str: *const c_char, len: usize, pData: *mut zval) -> *mut zval {
    zend_hash_str_add_or_update(ht, str, len, pData, HASH_ADD_NEW)
}

/// Adds a null value under a C-string key if absent; returns the slot or null.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_add_empty_element(ht: *mut HashTable, str: *const c_char, len: usize) -> *mut zval {
    let mut null = zval::UNDEF;
    null.u1.type_info = IS_NULL as u32;
    zend_hash_str_add(ht, str, len, &mut null)
}

// ------------------------------------------------------------ integer keys

/// Public entry for inserting by integer key with an explicit `HASH_*` flag.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_add_or_update(ht: *mut HashTable, h: zend_ulong, pData: *mut zval, flag: u32) -> *mut zval {
    add_or_update_index(ht, h, pData, flag)
}

/// Inserts under integer key `h` only if absent; returns null when it exists.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_add(ht: *mut HashTable, h: zend_ulong, pData: *mut zval) -> *mut zval {
    add_or_update_index(ht, h, pData, HASH_ADD)
}

/// Inserts under integer key `h` known to be absent; behaves like `zend_hash_index_add`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_add_new(ht: *mut HashTable, h: zend_ulong, pData: *mut zval) -> *mut zval {
    add_or_update_index(ht, h, pData, HASH_ADD_NEW)
}

/// Inserts or replaces the value stored under integer key `h`; returns the slot.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_update(ht: *mut HashTable, h: zend_ulong, pData: *mut zval) -> *mut zval {
    add_or_update_index(ht, h, pData, HASH_UPDATE)
}

/// Returns the slot for integer key `h`, creating it with a null value when missing.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_lookup(ht: *mut HashTable, h: zend_ulong) -> *mut zval {
    let mut null = zval::UNDEF;
    null.u1.type_info = IS_NULL as u32;
    add_or_update_index(ht, h, &mut null, HASH_LOOKUP)
}

/// Adds a null value under integer key `h` if absent; returns the slot or null.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_add_empty_element(ht: *mut HashTable, h: zend_ulong) -> *mut zval {
    let mut null = zval::UNDEF;
    null.u1.type_info = IS_NULL as u32;
    zend_hash_index_add(ht, h, &mut null)
}

/// Returns the next free integer key: 0 for a table that never received one, otherwise
/// `nNextFreeElement`.
unsafe fn next_index(ht: *const HashTable) -> zend_ulong {
    if (*ht).nNextFreeElement == ZEND_LONG_MIN {
        0
    } else {
        (*ht).nNextFreeElement as zend_ulong
    }
}

/// Appends a value under the next free integer key (`$a[] = ...`); returns the slot or null if that
/// key is taken.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_next_index_insert(ht: *mut HashTable, pData: *mut zval) -> *mut zval {
    add_or_update_index(ht, next_index(ht), pData, HASH_ADD | HASH_ADD_NEXT)
}

/// Appends a value under the next free integer key; behaves like `zend_hash_next_index_insert`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_next_index_insert_new(ht: *mut HashTable, pData: *mut zval) -> *mut zval {
    add_or_update_index(ht, next_index(ht), pData, HASH_ADD | HASH_ADD_NEXT)
}

// ------------------------------------------------------------ deletion entry points

/// Deletes the entry for a `zend_string` key, releasing its key and value; returns `SUCCESS`, or
/// `FAILURE` if absent.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_del(ht: *mut HashTable, key: *mut zend_string) -> zend_result {
    let p = find_bucket_key(ht, key);
    if p.is_null() {
        return FAILURE;
    }
    delete_bucket(ht, p);
    SUCCESS
}

/// Like `zend_hash_del`, but for an `IS_INDIRECT` slot it only marks the pointed-at zval `IS_UNDEF`
/// (running the destructor) and keeps the bucket; `FAILURE` if absent or already undefined.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_del_ind(ht: *mut HashTable, key: *mut zend_string) -> zend_result {
    delete_ind(ht, find_bucket_key(ht, key))
}

/// The `_ind` deletion shared by the `zend_string` and C-string forms, as php-src does it: an
/// `IS_INDIRECT` slot has the zval it points at undefined (before its destructor runs) and the
/// table flagged `HASH_FLAG_HAS_EMPTY_IND`, keeping the bucket; any other entry is deleted.
/// `FAILURE` when `p` is NULL or its indirect zval is already undefined.
unsafe fn delete_ind(ht: *mut HashTable, p: *mut Bucket) -> zend_result {
    if p.is_null() {
        return FAILURE;
    }
    if (*p).val.u1.v.r#type == IS_INDIRECT {
        let data = (*p).val.value.zv;
        if (*data).u1.v.r#type == IS_UNDEF {
            return FAILURE;
        }
        let mut old = zval::UNDEF;
        copy_value(&mut old, data);
        (*data).u1.type_info = IS_UNDEF as u32;
        (*ht).u.flags |= HASH_FLAG_HAS_EMPTY_IND;
        if let Some(destructor) = (*ht).pDestructor {
            destructor(&mut old);
        }
        return SUCCESS;
    }
    delete_bucket(ht, p);
    SUCCESS
}

/// Deletes the entry for a C-string key; `SUCCESS` or `FAILURE` if absent.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_del(ht: *mut HashTable, str: *const c_char, len: usize) -> zend_result {
    let p = find_bucket_str(ht, cbytes(str, len), hash_bytes(cbytes(str, len)));
    if p.is_null() {
        return FAILURE;
    }
    delete_bucket(ht, p);
    SUCCESS
}

/// Like `zend_hash_str_del`, but an `IS_INDIRECT` slot is handled as `zend_hash_del_ind` does.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_del_ind(ht: *mut HashTable, str: *const c_char, len: usize) -> zend_result {
    delete_ind(ht, find_bucket_str(ht, cbytes(str, len), hash_bytes(cbytes(str, len))))
}

/// Deletes the entry for integer key `h`; `SUCCESS` or `FAILURE` if absent.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_del(ht: *mut HashTable, h: zend_ulong) -> zend_result {
    let p = find_bucket_index(ht, h);
    if p.is_null() {
        return FAILURE;
    }
    delete_bucket(ht, p);
    SUCCESS
}

/// Deletes a bucket the caller already holds, releasing its key and value.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_del_bucket(ht: *mut HashTable, p: *mut Bucket) {
    delete_bucket(ht, p);
}

/// Deletes the entry whose value slot is `zv` (in this unpacked layout the zval is the first member
/// of its bucket).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_packed_del_val(ht: *mut HashTable, zv: *mut zval) {
    delete_bucket(ht, zv.cast::<Bucket>());
}

// ------------------------------------------------------------ lookup entry points

/// Returns the value slot stored under a `zend_string` key, or null.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_find(ht: *const HashTable, key: *mut zend_string) -> *mut zval {
    let p = find_bucket_key(ht, key);
    if p.is_null() {
        ptr::null_mut()
    } else {
        &mut (*p).val
    }
}

/// Same as `zend_hash_find` (the key's hash is computed on demand here).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_find_known_hash(ht: *const HashTable, key: *const zend_string) -> *mut zval {
    zend_hash_find(ht, key as *mut zend_string)
}

/// Returns the value slot under a C-string key of given length, or null.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_find(ht: *const HashTable, str: *const c_char, len: usize) -> *mut zval {
    let p = find_bucket_str(ht, cbytes(str, len), hash_bytes(cbytes(str, len)));
    if p.is_null() {
        ptr::null_mut()
    } else {
        &mut (*p).val
    }
}

/// Returns the value slot under integer key `h`, or null.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_find(ht: *const HashTable, h: zend_ulong) -> *mut zval {
    let p = find_bucket_index(ht, h);
    if p.is_null() {
        ptr::null_mut()
    } else {
        &mut (*p).val
    }
}

/// Out-of-line form of `zend_hash_index_find` used by header macros.
#[no_mangle]
pub unsafe extern "C" fn _zend_hash_index_find(ht: *const HashTable, h: zend_ulong) -> *mut zval {
    zend_hash_index_find(ht, h)
}

/// Follows an `IS_INDIRECT` zval to the zval it points at; returns null for a null input or an
/// indirect target that is `IS_UNDEF`, and the input otherwise.
unsafe fn deref_indirect(zv: *mut zval) -> *mut zval {
    if zv.is_null() {
        return zv;
    }
    if (*zv).u1.v.r#type == IS_INDIRECT {
        let inner = (*zv).value.zv;
        if (*inner).u1.v.r#type == IS_UNDEF {
            return ptr::null_mut();
        }
        return inner;
    }
    zv
}

/// Like `zend_hash_find` but resolves indirect slots, returning null when the target is undefined.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_find_ind(ht: *const HashTable, key: *mut zend_string) -> *mut zval {
    deref_indirect(zend_hash_find(ht, key))
}

/// Like `zend_hash_str_find` but resolves indirect slots.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_find_ind(ht: *const HashTable, str: *const c_char, len: usize) -> *mut zval {
    deref_indirect(zend_hash_str_find(ht, str, len))
}

/// True when a `zend_string` key is present.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_exists(ht: *const HashTable, key: *mut zend_string) -> bool {
    !zend_hash_find(ht, key).is_null()
}

/// True when a C-string key is present.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_exists(ht: *const HashTable, str: *const c_char, len: usize) -> bool {
    !zend_hash_str_find(ht, str, len).is_null()
}

/// True when integer key `h` is present.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_exists(ht: *const HashTable, h: zend_ulong) -> bool {
    !zend_hash_index_find(ht, h).is_null()
}

/// True when the key is present and, if indirect, its target is defined.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_exists_ind(ht: *const HashTable, key: *mut zend_string) -> bool {
    !zend_hash_find_ind(ht, key).is_null()
}

/// C-string-key form of `zend_hash_exists_ind`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_exists_ind(ht: *const HashTable, str: *const c_char, len: usize) -> bool {
    !zend_hash_str_find_ind(ht, str, len).is_null()
}

/// Looks up an ASCII-lowercased copy of a C-string key and returns the stored `IS_PTR` pointer, or
/// null when missing (case-insensitive function/class table lookups).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_find_ptr_lc(ht: *const HashTable, str: *const c_char, len: usize) -> *mut c_void {
    let lower: Vec<u8> = cbytes(str, len).iter().map(u8::to_ascii_lowercase).collect();
    let p = find_bucket_str(ht, &lower, hash_bytes(&lower));
    if p.is_null() {
        ptr::null_mut()
    } else {
        (*p).val.value.ptr
    }
}

/// `zend_string` form of `zend_hash_str_find_ptr_lc`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_find_ptr_lc(ht: *const HashTable, key: *mut zend_string) -> *mut c_void {
    zend_hash_str_find_ptr_lc(ht, (*key).val.as_ptr(), (*key).len)
}

/// Renames a bucket in place: fails (null) if the new key belongs to another bucket, otherwise
/// relinks the chain under the new key, releasing the old key and retaining the new one. Returns
/// the bucket's value slot.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_set_bucket_key(ht: *mut HashTable, b: *mut Bucket, key: *mut zend_string) -> *mut zval {
    // As php-src: a bucket already holding the key is left alone (releasing its key first would
    // free the very string about to be retained), and another bucket holding it is a refusal.
    let existing = find_bucket_key(ht, key);
    if !existing.is_null() {
        return if existing == b { &mut (*b).val } else { ptr::null_mut() };
    }
    let idx = b.offset_from(ar_data(ht)) as u32;
    unlink(ht, idx, b);
    if !(*b).key.is_null() {
        zend_string_release((*b).key);
    }
    if !is_interned(key) {
        (*key).gc.refcount += 1;
    }
    (*b).key = key;
    (*b).h = zend_string_hash_val(key);
    let n_index = ((*b).h as u32) | (*ht).nTableMask;
    (*b).val.u2.next = *slot(ht, n_index);
    *slot(ht, n_index) = idx;
    &mut (*b).val
}

// ------------------------------------------------------------ pointer and memory variants

/// Wraps a raw pointer in an `IS_PTR` zval for storage in a table.
unsafe fn ptr_zval(p: *mut c_void) -> zval {
    let mut zv = zval::UNDEF;
    zv.value.ptr = p;
    zv.u1.type_info = IS_PTR as u32;
    zv
}

/// Extracts the raw pointer from an `IS_PTR` zval slot, passing null through.
unsafe fn zv_ptr(zv: *mut zval) -> *mut c_void {
    if zv.is_null() {
        ptr::null_mut()
    } else {
        (*zv).value.ptr
    }
}

/// Allocates `size` bytes with the engine allocator and copies `data` into them; used by the
/// `*_mem` inserts.
unsafe fn mem_copy(data: *mut c_void, size: usize) -> *mut c_void {
    let p = alloc(size);
    ptr::copy_nonoverlapping(data.cast::<u8>(), p.cast::<u8>(), size);
    p
}

/// Looks up a `zend_string` key and returns the stored raw pointer, or null.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_find_ptr(ht: *const HashTable, key: *mut zend_string) -> *mut c_void {
    zv_ptr(zend_hash_find(ht, key))
}

/// C-string-key form of `zend_hash_find_ptr`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_find_ptr(ht: *const HashTable, str: *const c_char, len: usize) -> *mut c_void {
    zv_ptr(zend_hash_str_find(ht, str, len))
}

/// Integer-key form of `zend_hash_find_ptr`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_find_ptr(ht: *const HashTable, h: zend_ulong) -> *mut c_void {
    zv_ptr(zend_hash_index_find(ht, h))
}

/// Adds a raw pointer under a `zend_string` key if absent; returns the pointer, or null if the key
/// exists.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_add_ptr(ht: *mut HashTable, key: *mut zend_string, pData: *mut c_void) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_add(ht, key, &mut zv))
}

/// Adds a raw pointer under a `zend_string` key known to be absent.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_add_new_ptr(ht: *mut HashTable, key: *mut zend_string, pData: *mut c_void) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_add_new(ht, key, &mut zv))
}

/// Adds a raw pointer under a C-string key if absent; null if the key exists.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_add_ptr(ht: *mut HashTable, str: *const c_char, len: usize, pData: *mut c_void) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_str_add(ht, str, len, &mut zv))
}

/// Adds a raw pointer under a C-string key known to be absent.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_add_new_ptr(
    ht: *mut HashTable,
    str: *const c_char,
    len: usize,
    pData: *mut c_void,
) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_str_add_new(ht, str, len, &mut zv))
}

/// Stores a raw pointer under a `zend_string` key, replacing any existing value; returns the
/// pointer.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_update_ptr(ht: *mut HashTable, key: *mut zend_string, pData: *mut c_void) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_update(ht, key, &mut zv))
}

/// Stores a raw pointer under a C-string key, replacing any existing value.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_update_ptr(
    ht: *mut HashTable,
    str: *const c_char,
    len: usize,
    pData: *mut c_void,
) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_str_update(ht, str, len, &mut zv))
}

/// Adds a raw pointer under integer key `h` if absent; null if it exists.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_add_ptr(ht: *mut HashTable, h: zend_ulong, pData: *mut c_void) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_index_add(ht, h, &mut zv))
}

/// Adds a raw pointer under integer key `h` known to be absent.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_add_new_ptr(ht: *mut HashTable, h: zend_ulong, pData: *mut c_void) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_index_add_new(ht, h, &mut zv))
}

/// Stores a raw pointer under integer key `h`, replacing any existing value.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_update_ptr(ht: *mut HashTable, h: zend_ulong, pData: *mut c_void) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_index_update(ht, h, &mut zv))
}

/// Appends a raw pointer under the next free integer key.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_next_index_insert_ptr(ht: *mut HashTable, pData: *mut c_void) -> *mut c_void {
    let mut zv = ptr_zval(pData);
    zv_ptr(zend_hash_next_index_insert(ht, &mut zv))
}

/// Copies `size` bytes into a fresh allocation and adds the copy's pointer under a `zend_string`
/// key (the table then owns that copy).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_add_mem(ht: *mut HashTable, key: *mut zend_string, pData: *mut c_void, size: usize) -> *mut c_void {
    let mut placeholder = ptr_zval(ptr::null_mut());
    fill_mem(zend_hash_add(ht, key, &mut placeholder), pData, size)
}

/// Completes a `*_add_mem` insert as php-src does: the slot was added holding a NULL pointer, and
/// only then is the copy made, so a refused add (the key exists) allocates nothing. NULL when
/// `slot` is NULL.
unsafe fn fill_mem(slot: *mut zval, data: *mut c_void, size: usize) -> *mut c_void {
    if slot.is_null() {
        return ptr::null_mut();
    }
    (*slot).value.ptr = mem_copy(data, size);
    (*slot).value.ptr
}

/// Copies `size` bytes and adds the copy under a C-string key.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_add_mem(
    ht: *mut HashTable,
    str: *const c_char,
    len: usize,
    pData: *mut c_void,
    size: usize,
) -> *mut c_void {
    let mut placeholder = ptr_zval(ptr::null_mut());
    fill_mem(zend_hash_str_add(ht, str, len, &mut placeholder), pData, size)
}

/// Copies `size` bytes and stores the copy under a `zend_string` key, replacing any existing value.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_update_mem(
    ht: *mut HashTable,
    key: *mut zend_string,
    pData: *mut c_void,
    size: usize,
) -> *mut c_void {
    zend_hash_update_ptr(ht, key, mem_copy(pData, size))
}

/// Copies `size` bytes and stores the copy under a C-string key, replacing any existing value.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_str_update_mem(
    ht: *mut HashTable,
    str: *const c_char,
    len: usize,
    pData: *mut c_void,
    size: usize,
) -> *mut c_void {
    zend_hash_str_update_ptr(ht, str, len, mem_copy(pData, size))
}

/// Copies `size` bytes and stores the copy under integer key `h`, replacing any existing value.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_index_update_mem(ht: *mut HashTable, h: zend_ulong, pData: *mut c_void, size: usize) -> *mut c_void {
    zend_hash_index_update_ptr(ht, h, mem_copy(pData, size))
}

/// Copies `size` bytes and appends the copy under the next free integer key.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_next_index_insert_mem(ht: *mut HashTable, pData: *mut c_void, size: usize) -> *mut c_void {
    let mut placeholder = ptr_zval(ptr::null_mut());
    fill_mem(zend_hash_next_index_insert(ht, &mut placeholder), pData, size)
}

/// Returns the raw pointer stored at the iteration position `pos`, or null past the end.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_get_current_data_ptr_ex(ht: *mut HashTable, pos: *mut HashPosition) -> *mut c_void {
    zv_ptr(zend_hash_get_current_data_ex(ht, pos))
}

/// Symbol-table lookup by C-string key (numeric strings become integer keys) returning the stored
/// raw pointer.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_str_find_ptr(ht: *mut HashTable, str: *const c_char, len: usize) -> *mut c_void {
    zv_ptr(zend_symtable_str_find(ht, str, len))
}

// ------------------------------------------------------------ symbol tables (numeric string keys)

/// PHP's rule for a string key that is really an integer: an optional `-`,
/// then a decimal without leading zeros (other than `0` itself, never `-0`)
/// that fits a zend_long.
pub fn numeric_key(key: &[u8]) -> Option<zend_ulong> {
    let (negative, digits) = match key.first() {
        Some(b'-') => (true, &key[1..]),
        _ => (false, key),
    };
    if digits.is_empty() || digits.len() > 19 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    if negative && digits == b"0" {
        return None;
    }
    let text = std::str::from_utf8(key).ok()?;
    text.parse::<i64>().ok().map(|value| value as zend_ulong)
}

/// Tests whether a C string is a canonical decimal integer key per PHP's array-key rules and, if
/// so, stores it in `*idx` and returns true.
#[no_mangle]
pub unsafe extern "C" fn _zend_handle_numeric_str_ex(key: *const c_char, length: usize, idx: *mut zend_ulong) -> bool {
    match numeric_key(cbytes(key, length)) {
        Some(value) => {
            *idx = value;
            true
        }
        None => false,
    }
}

/// Out-of-line form of `_zend_handle_numeric_str_ex` used by header macros.
#[no_mangle]
pub unsafe extern "C" fn _zend_handle_numeric_str(key: *const c_char, length: usize, idx: *mut zend_ulong) -> bool {
    _zend_handle_numeric_str_ex(key, length, idx)
}

/// Symbol-table update by `zend_string`: a canonical numeric string is stored under the integer
/// key, otherwise under the string key.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_update(ht: *mut HashTable, key: *mut zend_string, pData: *mut zval) -> *mut zval {
    match numeric_key(bytes(key)) {
        Some(idx) => zend_hash_index_update(ht, idx, pData),
        None => zend_hash_update(ht, key, pData),
    }
}

/// Like `zend_symtable_update`, writing through indirect slots for non-numeric keys.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_update_ind(ht: *mut HashTable, key: *mut zend_string, pData: *mut zval) -> *mut zval {
    match numeric_key(bytes(key)) {
        Some(idx) => zend_hash_index_update(ht, idx, pData),
        None => zend_hash_update_ind(ht, key, pData),
    }
}

/// Symbol-table add of a key known to be absent, converting canonical numeric strings to integer
/// keys.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_add_new(ht: *mut HashTable, key: *mut zend_string, pData: *mut zval) -> *mut zval {
    match numeric_key(bytes(key)) {
        Some(idx) => zend_hash_index_add_new(ht, idx, pData),
        None => zend_hash_add_new(ht, key, pData),
    }
}

/// Symbol-table delete by `zend_string`, mapping numeric strings to integer keys; `SUCCESS` or
/// `FAILURE`.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_del(ht: *mut HashTable, key: *mut zend_string) -> zend_result {
    match numeric_key(bytes(key)) {
        Some(idx) => zend_hash_index_del(ht, idx),
        None => zend_hash_del(ht, key),
    }
}

/// Like `zend_symtable_del` but string keys go through `zend_hash_del_ind`.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_del_ind(ht: *mut HashTable, key: *mut zend_string) -> zend_result {
    match numeric_key(bytes(key)) {
        Some(idx) => zend_hash_index_del(ht, idx),
        None => zend_hash_del_ind(ht, key),
    }
}

/// Symbol-table lookup by `zend_string`, mapping numeric strings to integer keys.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_find(ht: *const HashTable, key: *mut zend_string) -> *mut zval {
    match numeric_key(bytes(key)) {
        Some(idx) => zend_hash_index_find(ht, idx),
        None => zend_hash_find(ht, key),
    }
}

/// Like `zend_symtable_find` but resolves indirect slots for string keys.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_find_ind(ht: *const HashTable, key: *mut zend_string) -> *mut zval {
    match numeric_key(bytes(key)) {
        Some(idx) => zend_hash_index_find(ht, idx),
        None => zend_hash_find_ind(ht, key),
    }
}

/// True when the symbol-table key (numeric strings as integers) is present.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_exists(ht: *mut HashTable, key: *mut zend_string) -> bool {
    !zend_symtable_find(ht, key).is_null()
}

/// Like `zend_symtable_exists`, resolving indirect slots.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_exists_ind(ht: *mut HashTable, key: *mut zend_string) -> bool {
    !zend_symtable_find_ind(ht, key).is_null()
}

/// Symbol-table update by C-string key, mapping numeric strings to integer keys.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_str_update(ht: *mut HashTable, str: *const c_char, len: usize, pData: *mut zval) -> *mut zval {
    match numeric_key(cbytes(str, len)) {
        Some(idx) => zend_hash_index_update(ht, idx, pData),
        None => zend_hash_str_update(ht, str, len, pData),
    }
}

/// Like `zend_symtable_str_update`, writing through indirect slots for non-numeric keys.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_str_update_ind(
    ht: *mut HashTable,
    str: *const c_char,
    len: usize,
    pData: *mut zval,
) -> *mut zval {
    match numeric_key(cbytes(str, len)) {
        Some(idx) => zend_hash_index_update(ht, idx, pData),
        None => zend_hash_str_update_ind(ht, str, len, pData),
    }
}

/// Symbol-table delete by C-string key, mapping numeric strings to integer keys.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_str_del(ht: *mut HashTable, str: *const c_char, len: usize) -> zend_result {
    match numeric_key(cbytes(str, len)) {
        Some(idx) => zend_hash_index_del(ht, idx),
        None => zend_hash_str_del(ht, str, len),
    }
}

/// `zend_symtable_str_del` whose string keys go through `zend_hash_str_del_ind` (an
/// `IS_INDIRECT` slot is undefined, not removed); numeric strings delete the integer key.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_str_del_ind(ht: *mut HashTable, str: *const c_char, len: usize) -> zend_result {
    match numeric_key(cbytes(str, len)) {
        Some(idx) => zend_hash_index_del(ht, idx),
        None => zend_hash_str_del_ind(ht, str, len),
    }
}

/// Symbol-table lookup by C-string key, mapping numeric strings to integer keys.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_str_find(ht: *mut HashTable, str: *const c_char, len: usize) -> *mut zval {
    match numeric_key(cbytes(str, len)) {
        Some(idx) => zend_hash_index_find(ht, idx),
        None => zend_hash_str_find(ht, str, len),
    }
}

/// True when the C-string symbol-table key is present.
#[no_mangle]
pub unsafe extern "C" fn zend_symtable_str_exists(ht: *mut HashTable, str: *const c_char, len: usize) -> bool {
    !zend_symtable_str_find(ht, str, len).is_null()
}

// ------------------------------------------------------------ apply

/// Walks live buckets forward or in reverse, calling `call` on each value; a result with
/// `ZEND_HASH_APPLY_REMOVE` deletes the bucket and `ZEND_HASH_APPLY_STOP` ends the walk. Tolerates
/// the table shrinking during the callback.
unsafe fn apply_with(ht: *mut HashTable, reverse: bool, mut call: impl FnMut(*mut zval, *mut Bucket) -> c_int) {
    let used = (*ht).nNumUsed;
    let indexes: Box<dyn Iterator<Item = u32>> = if reverse { Box::new((0..used).rev()) } else { Box::new(0..used) };
    for idx in indexes {
        if idx >= (*ht).nNumUsed {
            continue;
        }
        let p = bucket(ht, idx);
        if is_undef(p) {
            continue;
        }
        let result = call(&mut (*p).val, p);
        if result & ZEND_HASH_APPLY_REMOVE != 0 {
            delete_bucket(ht, p);
        }
        if result & ZEND_HASH_APPLY_STOP != 0 {
            break;
        }
    }
}

/// Calls `apply_func` on each value in order, honouring the remove/stop result bits; no-op without
/// a callback.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_apply(ht: *mut HashTable, apply_func: apply_func_t) {
    let Some(func) = apply_func else { return };
    apply_with(ht, false, |zv, _| func(zv));
}

/// Like `zend_hash_apply`, passing `argument` to every callback invocation.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_apply_with_argument(ht: *mut HashTable, apply_func: apply_func_arg_t, argument: *mut c_void) {
    let Some(func) = apply_func else { return };
    apply_with(ht, false, |zv, _| func(zv, argument));
}

/// Like `zend_hash_apply` but visits entries from last to first.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_reverse_apply(ht: *mut HashTable, apply_func: apply_func_t) {
    let Some(func) = apply_func else { return };
    apply_with(ht, true, |zv, _| func(zv));
}

// ------------------------------------------------------------ internal pointer iteration

/// Advances a position past deleted holes to the next live bucket index (or `nNumUsed` at the end).
unsafe fn valid_pos(ht: *const HashTable, mut pos: u32) -> u32 {
    while pos < (*ht).nNumUsed && is_undef(bucket(ht, pos)) {
        pos += 1;
    }
    pos
}

/// Returns the table's internal pointer, normalised to a live bucket.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_get_current_pos(ht: *const HashTable) -> HashPosition {
    valid_pos(ht, (*ht).nInternalPointer)
}

/// Sets `*pos` to the first live bucket (`reset()` semantics).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_internal_pointer_reset_ex(ht: *const HashTable, pos: *mut HashPosition) {
    *pos = valid_pos(ht, 0);
}

/// Sets `*pos` to the last live bucket, or past the end when the table is empty (`end()`
/// semantics).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_internal_pointer_end_ex(ht: *const HashTable, pos: *mut HashPosition) {
    let mut idx = (*ht).nNumUsed;
    while idx > 0 {
        idx -= 1;
        if !is_undef(bucket(ht, idx)) {
            *pos = idx;
            return;
        }
    }
    *pos = (*ht).nNumUsed;
}

/// Advances `*pos` to the next live bucket (the end position when exhausted); `FAILURE` if already
/// past the end.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_move_forward_ex(ht: *const HashTable, pos: *mut HashPosition) -> zend_result {
    let mut idx = valid_pos(ht, *pos);
    if idx >= (*ht).nNumUsed {
        return FAILURE;
    }
    loop {
        idx += 1;
        if idx >= (*ht).nNumUsed {
            *pos = (*ht).nNumUsed;
            return SUCCESS;
        }
        if !is_undef(bucket(ht, idx)) {
            *pos = idx;
            return SUCCESS;
        }
    }
}

/// Moves `*pos` to the previous live bucket, or to the end position when none remains; `FAILURE` if
/// already past the end.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_move_backwards_ex(ht: *const HashTable, pos: *mut HashPosition) -> zend_result {
    let mut idx = *pos;
    if idx >= (*ht).nNumUsed {
        return FAILURE;
    }
    while idx > 0 {
        idx -= 1;
        if !is_undef(bucket(ht, idx)) {
            *pos = idx;
            return SUCCESS;
        }
    }
    *pos = (*ht).nNumUsed;
    SUCCESS
}

/// Reports the key at `*pos`: stores a string key in `*str_index` (`HASH_KEY_IS_STRING`) or an
/// integer in `*num_index` (`HASH_KEY_IS_LONG`), or returns `HASH_KEY_NON_EXISTENT` past the end.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_get_current_key_ex(
    ht: *const HashTable,
    str_index: *mut *mut zend_string,
    num_index: *mut zend_ulong,
    pos: *const HashPosition,
) -> c_int {
    let idx = valid_pos(ht, *pos);
    if idx >= (*ht).nNumUsed {
        return HASH_KEY_NON_EXISTENT;
    }
    let p = bucket(ht, idx);
    if (*p).key.is_null() {
        *num_index = (*p).h;
        HASH_KEY_IS_LONG
    } else {
        *str_index = (*p).key;
        HASH_KEY_IS_STRING
    }
}

/// Writes the key at `*pos` into `key` as an int or a string copy, or null past the end.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_get_current_key_zval_ex(ht: *const HashTable, key: *mut zval, pos: *const HashPosition) {
    let idx = valid_pos(ht, *pos);
    if idx >= (*ht).nNumUsed {
        (*key).u1.type_info = IS_NULL as u32;
        return;
    }
    let p = bucket(ht, idx);
    if (*p).key.is_null() {
        (*key).value.lval = (*p).h as zend_long;
        (*key).u1.type_info = IS_LONG as u32;
    } else {
        crate::zval::set_str_copy(key, (*p).key);
    }
}

/// Returns `HASH_KEY_IS_LONG`, `HASH_KEY_IS_STRING` or `HASH_KEY_NON_EXISTENT` for the entry at
/// `*pos`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_get_current_key_type_ex(ht: *const HashTable, pos: *const HashPosition) -> c_int {
    let idx = valid_pos(ht, *pos);
    if idx >= (*ht).nNumUsed {
        HASH_KEY_NON_EXISTENT
    } else if (*bucket(ht, idx)).key.is_null() {
        HASH_KEY_IS_LONG
    } else {
        HASH_KEY_IS_STRING
    }
}

/// Returns the value slot at `*pos`, or null past the end.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_get_current_data_ex(ht: *const HashTable, pos: *const HashPosition) -> *mut zval {
    let idx = valid_pos(ht, *pos);
    if idx >= (*ht).nNumUsed {
        ptr::null_mut()
    } else {
        &mut (*bucket(ht, idx)).val
    }
}

/// External iterators (foreach by reference in PHP code) never exist here:
/// an iterator is its position.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_iterator_add(_ht: *mut HashTable, pos: HashPosition) -> u32 {
    pos
}

/// Returns the position for an iterator id; an iterator is its position, so this is the id itself.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_iterator_pos(idx: u32, _ht: *mut HashTable) -> HashPosition {
    idx
}

/// Like `zend_hash_iterator_pos` for an array zval; returns the id as the position.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_iterator_pos_ex(idx: u32, _array: *mut zval) -> HashPosition {
    idx
}

/// No-op: iterators are plain positions with nothing to unregister.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_iterator_del(_idx: u32) {}

// ------------------------------------------------------------ sorting and comparison

/// Sorts the entries with `compare` (stable). With `renumber`, the keys become
/// 0..n-1. Each bucket's position before sorting is left in `u2.extra`, which
/// PHP's stable comparators read.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_sort_ex(
    ht: *mut HashTable,
    _sort_func: sort_func_t,
    compare: bucket_compare_func_t,
    renumber: bool,
) {
    if !is_initialized(ht) {
        return;
    }
    let mut entries: Vec<Bucket> = Vec::with_capacity((*ht).nNumOfElements as usize);
    for idx in 0..(*ht).nNumUsed {
        let p = bucket(ht, idx);
        if is_undef(p) {
            continue;
        }
        let mut copy = *p;
        copy.val.u2.extra = entries.len() as u32;
        entries.push(copy);
    }
    if let Some(compare) = compare {
        entries.sort_by(|a, b| {
            let (mut a, mut b) = (*a, *b);
            compare(&mut a, &mut b).cmp(&0)
        });
    }
    for (i, entry) in entries.iter_mut().enumerate() {
        if renumber {
            if !entry.key.is_null() {
                zend_string_release(entry.key);
                entry.key = ptr::null_mut();
            }
            entry.h = i as zend_ulong;
        }
        ptr::write(bucket(ht, i as u32), *entry);
    }
    (*ht).nNumUsed = entries.len() as u32;
    (*ht).nInternalPointer = 0;
    if renumber {
        (*ht).nNextFreeElement = entries.len() as zend_long;
    }
    rehash(ht, false);
}

/// Sorts the table with `compare_func` (stable), optionally renumbering keys; wrapper over
/// `zend_hash_sort_ex`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_sort(ht: *mut HashTable, compare_func: bucket_compare_func_t, renumber: bool) {
    zend_hash_sort_ex(ht, None, compare_func, renumber);
}

/// A stable sort of `nmemb` elements of `siz` bytes.
#[no_mangle]
pub unsafe extern "C" fn zend_sort(base: *mut c_void, nmemb: usize, siz: usize, cmp: compare_func_t, _swp: swap_func_t) {
    let Some(cmp) = cmp else { return };
    if nmemb < 2 || siz == 0 {
        return;
    }
    let bytes_base = base.cast::<u8>();
    let mut order: Vec<usize> = (0..nmemb).collect();
    order.sort_by(|&a, &b| cmp(bytes_base.add(a * siz).cast(), bytes_base.add(b * siz).cast()).cmp(&0));
    let copy = std::slice::from_raw_parts(bytes_base, nmemb * siz).to_vec();
    for (target, source) in order.into_iter().enumerate() {
        ptr::copy_nonoverlapping(copy.as_ptr().add(source * siz), bytes_base.add(target * siz), siz);
    }
}

/// Same stable sort as `zend_sort` (PHP's insertion-sort entry point).
#[no_mangle]
pub unsafe extern "C" fn zend_insert_sort(base: *mut c_void, nmemb: usize, siz: usize, cmp: compare_func_t, swp: swap_func_t) {
    zend_sort(base, nmemb, siz, cmp, swp);
}

/// The entry `compar` ranks first (`flag` 0) or last (`flag` 1).
#[no_mangle]
pub unsafe extern "C" fn zend_hash_minmax(ht: *const HashTable, compar: compare_func_t, flag: u32) -> *mut zval {
    let Some(compar) = compar else { return ptr::null_mut() };
    let mut best: *mut Bucket = ptr::null_mut();
    for idx in 0..(*ht).nNumUsed {
        let p = bucket(ht, idx);
        if is_undef(p) {
            continue;
        }
        if best.is_null() {
            best = p;
            continue;
        }
        let order = compar(p.cast(), best.cast());
        if (flag != 0 && order > 0) || (flag == 0 && order < 0) {
            best = p;
        }
    }
    if best.is_null() {
        ptr::null_mut()
    } else {
        &mut (*best).val
    }
}

/// Compares two arrays as PHP's array comparison does: first by element count, then entry by entry
/// with `compar`, either in order (keys must match positionally) or by looking each key of `ht1` up
/// in `ht2`. Returns <0, 0, >0, and 1 when a key is missing from `ht2`.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_compare(ht1: *mut HashTable, ht2: *mut HashTable, compar: compare_func_t, ordered: bool) -> c_int {
    if ht1 == ht2 {
        return 0;
    }
    let (n1, n2) = ((*ht1).nNumOfElements, (*ht2).nNumOfElements);
    if n1 != n2 {
        return if n1 > n2 { 1 } else { -1 };
    }
    let Some(compar) = compar else { return 0 };
    let mut pos2 = valid_pos(ht2, 0);
    for idx in 0..(*ht1).nNumUsed {
        let p1 = bucket(ht1, idx);
        if is_undef(p1) {
            continue;
        }
        let other: *mut zval = if ordered {
            let p2 = bucket(ht2, pos2);
            pos2 = valid_pos(ht2, pos2 + 1);
            if (*p1).key.is_null() != (*p2).key.is_null() {
                return if (*p1).key.is_null() { -1 } else { 1 };
            }
            if (*p1).key.is_null() {
                if (*p1).h != (*p2).h {
                    return if (*p1).h > (*p2).h { 1 } else { -1 };
                }
            } else if !string::zend_string_equals((*p1).key, (*p2).key) {
                return crate::string::zend_binary_strcmp(
                    (*(*p1).key).val.as_ptr(),
                    (*(*p1).key).len,
                    (*(*p2).key).val.as_ptr(),
                    (*(*p2).key).len,
                );
            }
            &mut (*p2).val
        } else {
            let found = if (*p1).key.is_null() {
                zend_hash_index_find(ht2, (*p1).h)
            } else {
                zend_hash_find(ht2, (*p1).key)
            };
            if found.is_null() {
                return 1;
            }
            found
        };
        let result = compar((&mut (*p1).val as *mut zval).cast(), other.cast());
        if result != 0 {
            return result;
        }
    }
    0
}

/// Moves external iterators off a bucket that changed place; there are none.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_iterators_update(_ht: *mut HashTable, _from: HashPosition, _to: HashPosition) {}
