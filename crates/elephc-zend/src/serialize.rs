//! Purpose:
//! PHP's `serialize()`/`unserialize()` format, as extensions call it:
//! `php_var_serialize`, `php_var_unserialize` and their state.
//!
//! Called from:
//! - Extension code: APCu's default serializer, msgpack's and igbinary's
//!   fallback for values they do not encode themselves.
//!
//! Key details:
//! - The text is byte-for-byte PHP's for null, booleans, integers, floats
//!   (shortest round trip), strings, arrays and plain objects, including the
//!   `r:`/`R:` back-references for repeated objects and references.
//! - An object whose class is not registered unserializes as
//!   `__PHP_Incomplete_Class`, remembering its name, as PHP does.
//! - Objects are written with their properties table; classes with their own
//!   serialization hooks (`Serializable`, `__serialize`/`__unserialize`,
//!   `__sleep`/`__wakeup`) are refused by name rather than handled without
//!   running them.

use std::collections::HashMap;
use std::ffi::c_int;
use std::ptr;

use crate::abi::*;
use crate::alloc::{alloc_struct, free};
use crate::hash;
use crate::smart_str::append_bytes;
use crate::string::{self, bytes};
use crate::zval::*;

struct SerializeState {
    seen: HashMap<usize, u32>,
    next: u32,
}

struct UnserializeState {
    values: Vec<*mut zval>,
    temporaries: Vec<*mut zval>,
    held: Vec<zval>,
    max_depth: zend_long,
    cur_depth: zend_long,
    /// The extension's table of lowercase class names `O:` may instantiate, as php-src keeps it
    /// (not copied): NULL allows every class, an empty table none.
    allowed_classes: *mut HashTable,
}

/// `php_var_serialize_init`: creates the serializer state that tracks already-seen values for back
/// references; free it with `php_var_serialize_destroy`.
#[no_mangle]
pub unsafe extern "C" fn php_var_serialize_init() -> php_serialize_data_t {
    Box::into_raw(Box::new(SerializeState { seen: HashMap::new(), next: 0 })).cast()
}

/// `php_var_serialize_destroy`: frees the serializer state (NULL is a no-op).
#[no_mangle]
pub unsafe extern "C" fn php_var_serialize_destroy(d: php_serialize_data_t) {
    if !d.is_null() {
        drop(Box::from_raw(d.cast::<SerializeState>()));
    }
}

/// `php_var_unserialize_init`: creates the unserializer state (value table for back references,
/// temporaries, held values, depth limit 4096); free it with `php_var_unserialize_destroy`.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_init() -> php_unserialize_data_t {
    Box::into_raw(Box::new(UnserializeState {
        values: Vec::new(),
        temporaries: Vec::new(),
        held: Vec::new(),
        max_depth: 4096,
        cur_depth: 0,
        allowed_classes: ptr::null_mut(),
    }))
    .cast()
}

/// `php_var_unserialize_destroy`: releases the values held by the unserializer state and frees it
/// (NULL is a no-op).
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_destroy(d: php_unserialize_data_t) {
    if d.is_null() {
        return;
    }
    let mut state = Box::from_raw(d.cast::<UnserializeState>());
    for held in state.held.iter_mut() {
        zval_ptr_dtor(held);
    }
    for temporary in state.temporaries.drain(..) {
        zval_ptr_dtor(temporary);
        free(temporary.cast());
    }
}

/// `var_destroy`: destroys the unserializer state behind `var_hash` and resets the pointer to NULL.
#[no_mangle]
pub unsafe extern "C" fn var_destroy(var_hash: *mut php_unserialize_data_t) {
    php_var_unserialize_destroy(*var_hash);
    *var_hash = ptr::null_mut();
}

/// Returns the unserializer state behind `var_hash`, creating it on first use.
unsafe fn state(var_hash: *mut php_unserialize_data_t) -> &'static mut UnserializeState {
    if (*var_hash).is_null() {
        *var_hash = php_var_unserialize_init();
    }
    &mut *(*var_hash).cast::<UnserializeState>()
}

/// A zval owned by the unserialize state, freed with it.
#[no_mangle]
pub unsafe extern "C" fn var_tmp_var(var_hashx: *mut php_unserialize_data_t) -> *mut zval {
    let slot = alloc_struct::<zval>();
    (*slot).u1.type_info = IS_UNDEF as u32;
    state(var_hashx).temporaries.push(slot);
    slot
}

/// Keeps `val` alive until the state is destroyed.
#[no_mangle]
pub unsafe extern "C" fn var_push_dtor(var_hash: *mut php_unserialize_data_t, val: *mut zval) {
    let mut held = zval::UNDEF;
    copy(&mut held, val);
    state(var_hash).held.push(held);
}

/// `var_push_dtor_value`: keeps a copy of `val` alive until the unserializer state is destroyed.
#[no_mangle]
pub unsafe extern "C" fn var_push_dtor_value(var_hash: *mut php_unserialize_data_t, val: *mut zval) {
    let mut held = zval::UNDEF;
    copy_value(&mut held, val);
    state(var_hash).held.push(held);
}

/// `var_replace`: repoints every back-reference slot that refers to `ozval` at `nzval`.
#[no_mangle]
pub unsafe extern "C" fn var_replace(var_hash: *mut php_unserialize_data_t, ozval: *mut zval, nzval: *mut zval) {
    for value in state(var_hash).values.iter_mut() {
        if *value == ozval {
            *value = nzval;
        }
    }
}

/// `php_var_unserialize_get_allowed_classes`: the table `php_var_unserialize_set_allowed_classes`
/// installed, NULL when every class is allowed.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_get_allowed_classes(d: php_unserialize_data_t) -> *mut HashTable {
    (*d.cast::<UnserializeState>()).allowed_classes
}

/// `php_var_unserialize_set_allowed_classes`: restricts the classes `O:` instantiates to the
/// table's lowercase names (an empty table allows none); any other class unserializes as
/// `__PHP_Incomplete_Class`, as in php-src. The table is the caller's, kept and not copied.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_set_allowed_classes(d: php_unserialize_data_t, classes: *mut HashTable) {
    (*d.cast::<UnserializeState>()).allowed_classes = classes;
}

/// Whether class `ce` declares or inherits the method `lowercase_name` (a magic method php-src
/// looks up by name, such as `__sleep` or `__wakeup`).
unsafe fn has_method(ce: *mut zend_class_entry, lowercase_name: &[u8]) -> bool {
    !hash::zend_hash_str_find_ptr(&(*ce).function_table, lowercase_name.as_ptr().cast(), lowercase_name.len()).is_null()
}

/// php-src's `unserialize_allowed_class`: whether `O:` may instantiate the class named `name`.
unsafe fn allowed_class(state: &UnserializeState, name: &[u8]) -> bool {
    let classes = state.allowed_classes;
    if classes.is_null() {
        return true;
    }
    if (*classes).nNumOfElements == 0 {
        return false;
    }
    let lowercase = name.to_ascii_lowercase();
    hash::zend_hash_str_exists(classes, lowercase.as_ptr().cast(), lowercase.len())
}

/// `php_var_unserialize_set_max_depth`: sets the maximum nesting depth the unserializer accepts.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_set_max_depth(d: php_unserialize_data_t, max_depth: zend_long) {
    (*d.cast::<UnserializeState>()).max_depth = max_depth;
}

/// `php_var_unserialize_get_max_depth`: the maximum nesting depth the unserializer accepts.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_get_max_depth(d: php_unserialize_data_t) -> zend_long {
    (*d.cast::<UnserializeState>()).max_depth
}

/// `php_var_unserialize_set_cur_depth`: sets the unserializer's current nesting depth.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_set_cur_depth(d: php_unserialize_data_t, cur_depth: zend_long) {
    (*d.cast::<UnserializeState>()).cur_depth = cur_depth;
}

/// `php_var_unserialize_get_cur_depth`: the unserializer's current nesting depth.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_get_cur_depth(d: php_unserialize_data_t) -> zend_long {
    (*d.cast::<UnserializeState>()).cur_depth
}

// ------------------------------------------------------------ serialize

/// Appends a serialized length-prefixed quoted string, `tag:len:"bytes"`, to the output buffer.
fn quoted(buf: &mut Vec<u8>, tag: &str, data: &[u8]) {
    buf.extend_from_slice(format!("{tag}:{}:\"", data.len()).as_bytes());
    buf.extend_from_slice(data);
    buf.push(b'"');
}

/// Appends the serialized form of an array bucket's key: an integer key as `i:N;`, a string key as
/// `s:len:"key";`.
unsafe fn serialize_key(buf: &mut Vec<u8>, p: *const Bucket) {
    if (*p).key.is_null() {
        buf.extend_from_slice(format!("i:{};", (*p).h as zend_long).as_bytes());
    } else {
        quoted(buf, "s", bytes((*p).key));
        buf.push(b';');
    }
}

/// Appends the serialized form of a zval (scalars, strings, arrays, objects, references with `R:`
/// back references) to the buffer, recording seen values in the state; returns an error message
/// when a value cannot be serialized.
unsafe fn serialize_value(buf: &mut Vec<u8>, value: *mut zval, state: &mut SerializeState) -> Result<(), String> {
    state.next += 1;
    let mut value = value;
    if type_of(value) == IS_REFERENCE {
        let key = (*value).value.r#ref as usize;
        if let Some(index) = state.seen.get(&key) {
            // A repeated reference takes no number of its own (php_add_var_hash
            // undoes the increment), unlike a repeated object's `r:`.
            state.next -= 1;
            buf.extend_from_slice(format!("R:{index};").as_bytes());
            return Ok(());
        }
        state.seen.insert(key, state.next);
        value = deref(value);
    }
    match type_of(value) {
        IS_UNDEF | IS_NULL => buf.extend_from_slice(b"N;"),
        IS_FALSE => buf.extend_from_slice(b"b:0;"),
        IS_TRUE => buf.extend_from_slice(b"b:1;"),
        IS_LONG => buf.extend_from_slice(format!("i:{};", (*value).value.lval).as_bytes()),
        IS_DOUBLE => {
            let d = (*value).value.dval;
            let text = if d.is_nan() {
                "NAN".to_string()
            } else if d.is_infinite() {
                if d > 0.0 { "INF" } else { "-INF" }.to_string()
            } else {
                crate::convert::double_repr(d, -1, false)
            };
            buf.extend_from_slice(format!("d:{text};").as_bytes());
        }
        IS_STRING => {
            quoted(buf, "s", bytes((*value).value.str));
            buf.push(b';');
        }
        IS_ARRAY => {
            let table = (*value).value.arr;
            buf.extend_from_slice(format!("a:{}:{{", (*table).nNumOfElements).as_bytes());
            for idx in 0..(*table).nNumUsed {
                let p = hash::bucket(table, idx);
                if type_of(&(*p).val) == IS_UNDEF {
                    continue;
                }
                serialize_key(buf, p);
                serialize_value(buf, &mut (*p).val, state)?;
            }
            buf.push(b'}');
        }
        IS_OBJECT => {
            let object = (*value).value.obj;
            let key = object as usize;
            if let Some(index) = state.seen.get(&key) {
                buf.extend_from_slice(format!("r:{index};").as_bytes());
                return Ok(());
            }
            state.seen.insert(key, state.next);
            let ce = (*object).ce;
            if (*ce).ce_flags & ZEND_ACC_NOT_SERIALIZABLE != 0 {
                return Err(format!("Serialization of '{}' is not allowed", string::text((*ce).name)));
            }
            if (*ce).serialize.is_some() || !(*ce).__serialize.is_null() || has_method(ce, b"__sleep") {
                return Err(format!(
                    "serializing an object of class {} through its own hooks is not supported by a compiled program",
                    string::text((*ce).name)
                ));
            }
            let mut name = bytes((*ce).name).to_vec();
            let incomplete = ce == crate::class::php_ce_incomplete_class;
            if incomplete {
                let stored = crate::object::find_property(object, b"__PHP_Incomplete_Class_Name");
                if !stored.is_null() && type_of(stored) == IS_STRING {
                    name = bytes((*stored).value.str).to_vec();
                }
            }
            // Only an incomplete class's remembered name is bookkeeping, as in php-src: any other
            // object's property of that name is serialized like the rest.
            let marker = |p: *mut Bucket| {
                incomplete && !(*p).key.is_null() && bytes((*p).key) == b"__PHP_Incomplete_Class_Name"
            };
            let table = crate::object::zend_elephc_object_properties(object);
            let mut count = 0;
            for idx in 0..(*table).nNumUsed {
                let p = hash::bucket(table, idx);
                if type_of(&(*p).val) != IS_UNDEF && !marker(p) {
                    count += 1;
                }
            }
            quoted(buf, "O", &name);
            buf.extend_from_slice(format!(":{count}:{{").as_bytes());
            for idx in 0..(*table).nNumUsed {
                let p = hash::bucket(table, idx);
                if type_of(&(*p).val) == IS_UNDEF || marker(p) {
                    continue;
                }
                serialize_key(buf, p);
                serialize_value(buf, &mut (*p).val, state)?;
            }
            buf.push(b'}');
        }
        _ => buf.extend_from_slice(b"i:0;"),
    }
    Ok(())
}

/// Appends `struc` serialized to `buf`.
#[no_mangle]
pub unsafe extern "C" fn php_var_serialize(buf: *mut smart_str, struc: *mut zval, data: *mut php_serialize_data_t) {
    let owned = data.is_null() || (*data).is_null();
    let state_ptr = if owned { php_var_serialize_init() } else { *data };
    let state = &mut *state_ptr.cast::<SerializeState>();
    let mut text = Vec::new();
    let result = serialize_value(&mut text, struc, state);
    if owned {
        php_var_serialize_destroy(state_ptr);
    }
    match result {
        Ok(()) => append_bytes(buf, &text),
        Err(message) => crate::exception::throw_error(crate::exception::zend_ce_exception, &message),
    }
}

// ------------------------------------------------------------ unserialize

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    /// Returns the next byte without consuming it, or None at the end of the input.
    fn peek(&self) -> Option<u8> {
        self.data.get(self.at).copied()
    }

    /// Consumes the next byte if it equals `byte`; None otherwise.
    fn expect(&mut self, byte: u8) -> Option<()> {
        if self.peek()? == byte {
            self.at += 1;
            Some(())
        } else {
            None
        }
    }

    /// Consumes and returns the bytes up to the next `stop` byte, and the stop byte itself; None
    /// when `stop` does not occur.
    fn until(&mut self, stop: u8) -> Option<&[u8]> {
        let start = self.at;
        let end = start + self.data[start..].iter().position(|&b| b == stop)?;
        self.at = end + 1;
        Some(&self.data[start..end])
    }

    /// Reads decimal text up to the `stop` byte and parses it as an integer; None when it is not a
    /// valid integer.
    fn integer(&mut self, stop: u8) -> Option<i64> {
        let text = self.until(stop)?;
        std::str::from_utf8(text).ok()?.parse().ok()
    }

    /// An `i:` value: as php-src's `parse_iv2`, digits past the integer range warn "Numerical
    /// result out of range" and give `PHP_INT_MAX` (or `PHP_INT_MIN` when negative).
    fn saturating_integer(&mut self, stop: u8) -> Option<i64> {
        let text = std::str::from_utf8(self.until(stop)?).ok()?;
        if let Ok(value) = text.parse() {
            return Some(value);
        }
        let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        // `php_error_docref`: prefixed with the running function, when there is one.
        let message = match unsafe { crate::error::active_function_prefix() } {
            Some(function) => format!("{function}(): Numerical result out of range"),
            None => "Numerical result out of range".to_string(),
        };
        crate::error::warning(&message);
        Some(if text.starts_with('-') { i64::MIN } else { i64::MAX })
    }

    /// An array's or object's entry count, then `:`. A count the rest of the
    /// input cannot hold (every entry takes at least 4 bytes, `i:0;N;` more),
    /// or that is negative, is a decoding failure: the table is sized from it
    /// up front, so decoding never grows it while back-references point into
    /// its slots.
    fn count(&mut self) -> Option<u32> {
        let count = u32::try_from(self.integer(b':')?).ok()?;
        let remaining = self.data.len().saturating_sub(self.at);
        (count as usize <= remaining / 4).then_some(count)
    }

    /// `N:"bytes"`: a length, then exactly that many bytes between quotes.
    /// A negative or out-of-range length is a decoding failure.
    fn quoted(&mut self) -> Option<&[u8]> {
        let len = usize::try_from(self.integer(b':')?).ok()?;
        self.expect(b'"')?;
        let start = self.at;
        let end = start.checked_add(len).filter(|&end| end <= self.data.len())?;
        self.at = end;
        self.expect(b'"')?;
        Some(&self.data[start..end])
    }
}

/// Decodes one serialized value at the reader's position into `rval`, enforcing the depth limit;
/// None on malformed input.
unsafe fn unserialize_value(reader: &mut Reader, rval: *mut zval, state: &mut UnserializeState, depth: zend_long) -> Option<()> {
    if state.max_depth > 0 && depth > state.max_depth {
        return None;
    }
    let tag = reader.peek()?;
    reader.at += 1;
    if tag == b'r' || tag == b'R' {
        reader.expect(b':')?;
        let index = reader.integer(b';')?;
        let target = *state.values.get((index as usize).checked_sub(1)?)?;
        // A repeated key reuses its slot, so a back-reference can name the slot being decoded
        // (`a:2:{i:0;i:5;i:0;R:2;}`); php-src refuses it, and sharing it would count an owner twice.
        if target == rval || deref(target) == rval {
            return None;
        }
        if tag == b'R' {
            if type_of(target) != IS_REFERENCE {
                let reference = zend_elephc_new_reference(target);
                (*target).value.r#ref = reference;
                (*target).u1.type_info = IS_REFERENCE_EX;
            }
            // PHP numbers a repeated `r:` value but not an `R:` one (it skips
            // `var_push` for it), so only the `r:` branch records `rval`.
            copy(rval, target);
        } else {
            // A lowercase `r:` shares an object; php-src refuses one naming anything else
            // (`a:1:{i:0;r:1;}` would make an array that contains itself).
            if type_of(deref(target)) != IS_OBJECT {
                return None;
            }
            copy(rval, deref(target));
            state.values.push(rval);
        }
        return Some(());
    }
    state.values.push(rval);
    match tag {
        b'N' => {
            reader.expect(b';')?;
            set_null(rval);
        }
        b'b' => {
            // php-src's grammar is `b:[01];`: any other value is malformed.
            reader.expect(b':')?;
            let value = match reader.integer(b';')? {
                0 => false,
                1 => true,
                _ => return None,
            };
            set_bool(rval, value);
        }
        b'i' => {
            reader.expect(b':')?;
            set_long(rval, reader.saturating_integer(b';')?);
        }
        b'd' => {
            reader.expect(b':')?;
            let text = std::str::from_utf8(reader.until(b';')?).ok()?;
            let value = match text {
                "NAN" => f64::NAN,
                "INF" => f64::INFINITY,
                "-INF" => f64::NEG_INFINITY,
                _ => text.parse().ok()?,
            };
            set_double(rval, value);
        }
        b's' => {
            reader.expect(b':')?;
            let data = reader.quoted()?.to_vec();
            reader.expect(b';')?;
            set_bytes(rval, &data);
        }
        b'a' => {
            reader.expect(b':')?;
            let count = reader.count()?;
            reader.expect(b'{')?;
            let table = hash::_zend_new_array(count);
            set_arr(rval, table);
            for _ in 0..count {
                read_entry(reader, table, state, depth, true)?;
            }
            reader.expect(b'}')?;
        }
        b'O' => {
            reader.expect(b':')?;
            let name = reader.quoted()?.to_vec();
            reader.expect(b':')?;
            let count = reader.count()?;
            reader.expect(b'{')?;
            let ce = if allowed_class(state, &name) { crate::class::lookup(&name) } else { ptr::null_mut() };
            if !ce.is_null() && (*ce).ce_flags & ZEND_ACC_NOT_SERIALIZABLE != 0 {
                let message = format!("Unserialization of '{}' is not allowed", string::text((*ce).name));
                crate::exception::throw(ptr::null_mut(), message.as_bytes(), 0);
                return None;
            }
            // A class restoring itself through its own hooks is refused, as serializing one is,
            // rather than built from its raw property table without running them.
            if !ce.is_null()
                && ((*ce).unserialize.is_some() || !(*ce).__unserialize.is_null() || has_method(ce, b"__wakeup"))
            {
                return None;
            }
            if ce.is_null() {
                crate::object::object_init_ex(rval, crate::class::php_ce_incomplete_class);
                let mut stored = zval::UNDEF;
                set_bytes(&mut stored, &name);
                crate::object::set_property((*rval).value.obj, b"__PHP_Incomplete_Class_Name", &mut stored);
            } else if crate::object::object_init_ex(rval, ce) != SUCCESS {
                return None;
            }
            let table = crate::object::properties((*rval).value.obj);
            // Back-references point into the table: it must not grow while filled.
            hash::zend_hash_extend(table, (*table).nNumUsed.saturating_add(count), false);
            for _ in 0..count {
                read_entry(reader, table, state, depth, false)?;
            }
            reader.expect(b'}')?;
        }
        _ => return None,
    }
    Some(())
}

/// One `key;value` pair into `table`. Property keys lose their visibility
/// mangling (`\0*\0name`), since objects here keep plain names.
unsafe fn read_entry(reader: &mut Reader, table: *mut HashTable, state: &mut UnserializeState, depth: zend_long, array: bool) -> Option<()> {
    let mut key = zval::UNDEF;
    match reader.peek()? {
        b'i' => {
            reader.at += 1;
            reader.expect(b':')?;
            set_long(&mut key, reader.integer(b';')?);
        }
        b's' => {
            reader.at += 1;
            reader.expect(b':')?;
            let data = reader.quoted()?.to_vec();
            reader.expect(b';')?;
            let name = if array { &data[..] } else { crate::object::visible_name(&data) };
            set_bytes(&mut key, name);
        }
        _ => return None,
    }
    // Objects keep property names as strings, integer-looking ones included.
    if type_of(&key) == IS_LONG && !array {
        let digits = key.value.lval.to_string();
        set_bytes(&mut key, digits.as_bytes());
    }
    let slot = claim_slot(table, &key, array, state);
    zval_ptr_dtor(&mut key);
    unserialize_value(reader, slot, state, depth + 1)
}

/// The slot `key` decodes into. A key seen before (`a:2:{i:0;a:..;i:0;N;}`)
/// reuses its slot, but the value it held is moved to `state.held` rather
/// than freed, as PHP does: back-references already recorded may point into
/// it, and must stay valid until decoding ends.
unsafe fn claim_slot(table: *mut HashTable, key: &zval, array: bool, state: &mut UnserializeState) -> *mut zval {
    let existing = match (type_of(key) == IS_LONG, array) {
        (true, _) => hash::zend_hash_index_find(table, key.value.lval as zend_ulong),
        (false, true) => hash::zend_symtable_find(table, key.value.str),
        (false, false) => hash::zend_hash_find(table, key.value.str),
    };
    if !existing.is_null() {
        state.held.push(existing.read());
        set_null(existing);
        return existing;
    }
    let mut value = zval::UNDEF;
    set_null(&mut value);
    match (type_of(key) == IS_LONG, array) {
        (true, _) => hash::zend_hash_index_update(table, key.value.lval as zend_ulong, &mut value),
        (false, true) => hash::zend_symtable_update(table, key.value.str, &mut value),
        (false, false) => hash::zend_hash_update(table, key.value.str, &mut value),
    }
}

/// Reads one value from `*p` (advancing it). 1 on success, 0 on failure.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize(
    rval: *mut zval,
    p: *mut *const u8,
    max: *const u8,
    var_hash: *mut php_unserialize_data_t,
) -> c_int {
    let len = max.offset_from(*p).max(0) as usize;
    let data = if len == 0 { &[][..] } else { std::slice::from_raw_parts(*p, len) };
    let mut reader = Reader { data, at: 0 };
    let state = state(var_hash);
    set_null(rval);
    let numbered_before = state.values.len();
    let ok = unserialize_value(&mut reader, rval, state, state.cur_depth + 1).is_some();
    if ok {
        *p = (*p).add(reader.at);
        1
    } else {
        // The partial value is destroyed below: forget the slots it numbered, so a later call
        // sharing this state cannot resolve an `R:`/`r:` into freed storage.
        state.values.truncate(numbered_before);
        zval_ptr_dtor(rval);
        set_bool(rval, false);
        0
    }
}

/// `php_var_unserialize_ref`: unserializes a value; same as `php_var_unserialize`.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_ref(
    rval: *mut zval,
    p: *mut *const u8,
    max: *const u8,
    var_hash: *mut php_unserialize_data_t,
) -> c_int {
    php_var_unserialize(rval, p, max, var_hash)
}

/// `php_var_unserialize_intern`: unserializes a value; same as `php_var_unserialize`.
#[no_mangle]
pub unsafe extern "C" fn php_var_unserialize_intern(
    rval: *mut zval,
    p: *mut *const u8,
    max: *const u8,
    var_hash: *mut php_unserialize_data_t,
) -> c_int {
    php_var_unserialize(rval, p, max, var_hash)
}

/// `php_var_serialize_get_lock`: the serialization lock level, always 0 here.
#[no_mangle]
pub extern "C" fn php_var_serialize_get_lock() -> zend_long {
    0
}

/// `php_serialize_unserialize_failed`: reports an unserialization failure by returning FAILURE.
#[no_mangle]
pub extern "C" fn php_serialize_unserialize_failed() -> zend_result {
    FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: unserializes `text`, serializes the resulting value back, and returns the bytes
    /// produced.
    unsafe fn round_trip(text: &[u8]) -> Vec<u8> {
        let mut value = zval::UNDEF;
        let mut cursor = text.as_ptr();
        let mut state = php_var_unserialize_init();
        assert_eq!(php_var_unserialize(&mut value, &mut cursor, text.as_ptr().add(text.len()), &mut state), 1);
        php_var_unserialize_destroy(state);
        let mut out = smart_str { s: ptr::null_mut(), a: 0 };
        php_var_serialize(&mut out, &mut value, ptr::null_mut());
        let result = bytes(out.s).to_vec();
        zval_ptr_dtor(&mut value);
        result
    }

    /// Verifies that serialized text for null, booleans, integers, floats, strings, arrays and a
    /// stdClass object survives an unserialize then serialize round trip unchanged.
    #[test]
    fn round_trips_php_serialize_text() {
        let _guard = crate::tests::lock();
        unsafe {
            for text in [
                &b"N;"[..],
                b"b:1;",
                b"i:-42;",
                b"d:0.1;",
                b"d:1.0E+25;",
                b"s:5:\"hello\";",
                b"a:2:{i:0;s:1:\"a\";s:1:\"k\";a:1:{i:0;b:0;}}",
                b"O:8:\"stdClass\":1:{s:1:\"x\";i:1;}",
            ] {
                assert_eq!(round_trip(text), text, "{}", String::from_utf8_lossy(text));
            }
        }
    }

    /// Verifies a failed call drops the values it numbered: a later `R:` naming one of them is
    /// malformed instead of reaching the destroyed value.
    #[test]
    fn a_failed_call_leaves_no_reference_into_its_destroyed_value() {
        let _guard = crate::tests::lock();
        unsafe {
            let mut state = php_var_unserialize_init();
            let truncated = &b"a:2:{i:0;s:3:\"abc\";i:1;"[..];
            let mut value = zval::UNDEF;
            let mut cursor = truncated.as_ptr();
            assert_eq!(php_var_unserialize(&mut value, &mut cursor, truncated.as_ptr().add(truncated.len()), &mut state), 0);
            // The array above was destroyed; a back reference to it must now be unresolvable.
            let reference = &b"R:2;"[..];
            let mut again = zval::UNDEF;
            let mut cursor = reference.as_ptr();
            assert_eq!(php_var_unserialize(&mut again, &mut cursor, reference.as_ptr().add(reference.len()), &mut state), 0);
            php_var_unserialize_destroy(state);
        }
    }

    /// Verifies a back-reference naming the slot being decoded is malformed, as in php-src: a
    /// repeated key reuses its slot, so `R:2` here would make the slot a reference to itself.
    #[test]
    fn a_reference_to_the_slot_being_decoded_is_refused() {
        let _guard = crate::tests::lock();
        unsafe {
            for text in [&b"a:2:{i:0;i:5;i:0;R:2;}"[..], &b"a:2:{i:0;O:8:\"stdClass\":0:{}i:0;r:2;}"[..]] {
                let mut state = php_var_unserialize_init();
                let mut value = zval::UNDEF;
                let mut cursor = text.as_ptr();
                assert_eq!(php_var_unserialize(&mut value, &mut cursor, text.as_ptr().add(text.len()), &mut state), 0);
                php_var_unserialize_destroy(state);
            }
        }
    }

    /// Verifies a repeated `R:` takes no number: in
    /// `a:4:{i:0;i:5;i:1;R:2;i:2;s:1:"x";i:3;R:3;}` (real PHP's own output) the
    /// fourth element is a reference to "x", not to 5, and serializing the result
    /// again numbers the same way. `r:` still counts, as in
    /// `a:4:{i:0;O:8:"stdClass":0:{}i:1;r:2;i:2;i:5;i:3;R:4;}`.
    #[test]
    fn repeated_references_are_not_numbered() {
        let _guard = crate::tests::lock();
        unsafe {
            let text = &b"a:4:{i:0;i:5;i:1;R:2;i:2;s:1:\"x\";i:3;R:3;}"[..];
            assert_eq!(round_trip(text), text);
            let mut value = zval::UNDEF;
            let mut cursor = text.as_ptr();
            let mut state = php_var_unserialize_init();
            assert_eq!(php_var_unserialize(&mut value, &mut cursor, text.as_ptr().add(text.len()), &mut state), 1);
            php_var_unserialize_destroy(state);
            let table = value.value.arr;
            let fourth = deref(&mut (*hash::bucket(table, 3)).val);
            assert_eq!(type_of(fourth), IS_STRING);
            assert_eq!(bytes((*fourth).value.str), b"x");
            let second = deref(&mut (*hash::bucket(table, 1)).val);
            assert_eq!((*second).value.lval, 5);
            zval_ptr_dtor(&mut value);
            let mixed = &b"a:4:{i:0;O:8:\"stdClass\":0:{}i:1;r:2;i:2;i:5;i:3;R:4;}"[..];
            assert_eq!(round_trip(mixed), mixed);
        }
    }

    /// Verifies a lowercase `r:` must name an object, as php-src requires: `a:1:{i:0;r:1;}` and
    /// `a:2:{i:0;a:0:{}i:1;r:2;}` are malformed (real PHP reports an error for both).
    #[test]
    fn a_lowercase_reference_must_name_an_object() {
        let _guard = crate::tests::lock();
        unsafe {
            for text in [&b"a:1:{i:0;r:1;}"[..], b"a:2:{i:0;a:0:{}i:1;r:2;}", b"a:2:{i:0;i:1;i:1;r:2;}"] {
                let mut value = zval::UNDEF;
                let mut cursor = text.as_ptr();
                let mut state = php_var_unserialize_init();
                let parsed = php_var_unserialize(&mut value, &mut cursor, text.as_ptr().add(text.len()), &mut state);
                php_var_unserialize_destroy(state);
                assert_eq!(parsed, 0, "{}", String::from_utf8_lossy(text));
                zval_ptr_dtor(&mut value);
            }
        }
    }

    /// Verifies the allowed-classes table an extension installs is obeyed, as in php-src: an empty
    /// table makes every `O:` an `__PHP_Incomplete_Class` remembering its name, a table naming the
    /// class lets it through, and the table reads back.
    #[test]
    fn allowed_classes_restrict_what_objects_become() {
        let _guard = crate::tests::lock();
        unsafe {
            let text = b"O:8:\"stdClass\":1:{s:1:\"a\";i:1;}";
            let class_of = |allowed: *mut HashTable| {
                let mut value = zval::UNDEF;
                let mut cursor = text.as_ptr();
                let mut state = php_var_unserialize_init();
                php_var_unserialize_set_allowed_classes(state, allowed);
                assert_eq!(php_var_unserialize_get_allowed_classes(state), allowed);
                let parsed = php_var_unserialize(&mut value, &mut cursor, text.as_ptr().add(text.len()), &mut state);
                php_var_unserialize_destroy(state);
                assert_eq!(parsed, 1);
                let name = string::text((*value.value.obj).ce.as_ref().unwrap().name);
                zval_ptr_dtor(&mut value);
                name
            };
            let none = hash::_zend_new_array(0);
            assert_eq!(class_of(none), "__PHP_Incomplete_Class");
            let some = hash::_zend_new_array(0);
            let mut yes = crate::zval::long(1);
            hash::zend_hash_str_update(some, c"stdclass".as_ptr(), 8, &mut yes);
            assert_eq!(class_of(some), "stdClass");
            assert_eq!(class_of(ptr::null_mut()), "stdClass");
            hash::zend_array_destroy(none);
            hash::zend_array_destroy(some);
        }
    }
}
