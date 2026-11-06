//! Purpose:
//! Builds and executes dynamic C calls with libffi for `FFI` object methods, global
//! function pointers, and callable CData values.
//!
//! Called from:
//! - `crate::abi` call-builder entry points.
//!
//! Key details:
//! - Argument values are collected in PHP source order, then marshalled in parameter
//!   order. Integers use their declared width, floats their declared precision, strings
//!   become NUL-terminated copies that outlive the call, and CData arguments contribute a
//!   pointer (pointer/array/struct) or a scalar payload.
//! - Variadic calls use libffi's variadic CIF and apply C default argument promotions.
//! - The return buffer is 16-byte aligned; pointer-to-`char` returns are converted to PHP
//!   strings exactly as php-src does, while every other pointer returns as CData.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::os::raw::c_void;

use libffi::middle::{Arg, Cif, CodePtr, Ret, Type};

use crate::context::{
    cdata, data_address, set_error, type_clone, ERROR_EXCEPTION,
};
use crate::place::{read_float, read_int, Place};
use crate::types::{TypeId, TypeKind};

/// Kinds returned by `elephc_ffi_call_invoke`.
pub const RESULT_VOID: u32 = 0;
pub const RESULT_INT: u32 = 1;
pub const RESULT_FLOAT: u32 = 2;
pub const RESULT_BOOL: u32 = 3;
pub const RESULT_CDATA: u32 = 4;
pub const RESULT_STRING: u32 = 5;
pub const RESULT_CHAR: u32 = 6;
/// The call failed before invoking C; the prelude must throw the recorded error.
pub const RESULT_ERROR: u32 = 7;

/// One PHP argument supplied to a pending call.
#[derive(Clone, Debug)]
pub enum ArgValue {
    /// PHP integer or bool.
    Int(i64),
    /// PHP float.
    Float(f64),
    /// PHP string bytes.
    String(Vec<u8>),
    /// CData handle.
    CData(usize),
    /// Raw C address prepared for a PHP callback argument.
    Pointer(usize),
    /// PHP null.
    Null,
}

/// One call being marshalled by the PHP prelude.
pub struct CallBuilder {
    context: usize,
    function_name: String,
    function_type: TypeId,
    /// Direct code pointer for CData function-pointer calls; 0 means resolve by name.
    direct_pointer: usize,
    args: BTreeMap<usize, ArgValue>,
    /// Result kind selected by `invoke`.
    pub result_kind: u32,
    /// Integer result payload.
    pub result_int: i64,
    /// Float result payload.
    pub result_float: f64,
    /// Bool result payload.
    pub result_bool: bool,
    /// String result payload.
    pub result_bytes: Vec<u8>,
    /// CData result handle.
    pub result_cdata: usize,
}

impl CallBuilder {
    /// Creates a call builder for an FFI context function.
    pub fn new(context: usize, name: &str) -> Option<Box<Self>> {
        let context_ref = crate::context::context(context)?;
        let function_type = *context_ref.functions.get(name)?;
        Some(Box::new(Self {
            context,
            function_name: name.to_string(),
            function_type,
            direct_pointer: 0,
            args: BTreeMap::new(),
            result_kind: RESULT_VOID,
            result_int: 0,
            result_float: 0.0,
            result_bool: false,
            result_bytes: Vec::new(),
            result_cdata: 0,
        }))
    }

    /// Creates a call builder for a callable CData function pointer.
    pub fn new_cdata(handle: usize) -> Option<Box<Self>> {
        let data = cdata(handle)?;
        let ty = type_clone(data.ty)?;
        let TypeKind::Pointer(inner) = ty.kind else {
            set_error(ERROR_EXCEPTION, "FFI\\CData is not callable");
            return None;
        };
        let function_ty = type_clone(inner)?;
        if !matches!(function_ty.kind, TypeKind::Func(_)) {
            set_error(ERROR_EXCEPTION, "FFI\\CData is not callable");
            return None;
        }
        let pointer = data.pointer_value();
        if pointer == 0 {
            set_error(ERROR_EXCEPTION, "FFI\\CData is not callable");
            return None;
        }
        Some(Box::new(Self {
            context: 0,
            function_name: String::new(),
            function_type: inner,
            direct_pointer: pointer,
            args: BTreeMap::new(),
            result_kind: RESULT_VOID,
            result_int: 0,
            result_float: 0.0,
            result_bool: false,
            result_bytes: Vec::new(),
            result_cdata: 0,
        }))
    }

    /// Records one positional argument.
    pub fn set_arg(&mut self, index: usize, value: ArgValue) {
        self.args.insert(index, value);
    }

    /// Returns the declared C type at one parameter position, or `None` for a variadic tail.
    pub fn param_type(&self, index: usize) -> Option<TypeId> {
        let info = type_clone(self.function_type)?;
        let TypeKind::Func(function) = info.kind else {
            return None;
        };
        function.params.get(index).copied()
    }

    /// Executes the pending call and stores its result.
    pub fn invoke(&mut self) -> u32 {
        self.result_kind = RESULT_VOID;
        self.result_int = 0;
        self.result_float = 0.0;
        self.result_bool = false;
        self.result_bytes.clear();
        self.result_cdata = 0;

        let Some(function) = type_clone(self.function_type) else {
            set_error(ERROR_EXCEPTION, "Invalid C function type");
            return RESULT_VOID;
        };
        let TypeKind::Func(func) = function.kind else {
            set_error(ERROR_EXCEPTION, "Invalid C function type");
            return RESULT_VOID;
        };
        let pointer = if self.direct_pointer != 0 {
            self.direct_pointer
        } else {
            let Some(context) = crate::context::context(self.context) else {
                set_error(ERROR_EXCEPTION, "Invalid FFI context");
                return RESULT_VOID;
            };
            match context.symbol(&self.function_name) {
                Some(pointer) => pointer,
                None => {
                    set_error(
                        ERROR_EXCEPTION,
                        format!("Undefined symbol \"{}\"", self.function_name),
                    );
                    return RESULT_VOID;
                }
            }
        };

        let count = self.args.len();
        if !func.variadic && count != func.params.len() {
            set_error(
                ERROR_EXCEPTION,
                format!(
                    "Incorrect number of arguments for C function '{}', expecting exactly {} parameter{}",
                    self.function_name,
                    func.params.len(),
                    if func.params.len() == 1 { "" } else { "s" }
                ),
            );
            return RESULT_ERROR;
        }
        if func.variadic && count < func.params.len() {
            set_error(
                ERROR_EXCEPTION,
                format!(
                    "Incorrect number of arguments for C function '{}', expecting at least {} parameters",
                    self.function_name,
                    func.params.len()
                ),
            );
            return RESULT_ERROR;
        }
        for (index, expected) in func.params.iter().enumerate() {
            let actual = self.args.get(&index).cloned().unwrap_or(ArgValue::Null);
            let Some(ty) = type_clone(*expected) else { continue };
            if !argument_is_compatible(&ty, &actual) {
                set_error(
                    ERROR_EXCEPTION,
                    format!(
                        "Passing incompatible argument {} of C function '{}', expecting '{}', found PHP '{}'",
                        index + 1,
                        self.function_name,
                        ty.display,
                        arg_php_label(&actual)
                    ),
                );
                return RESULT_ERROR;
            }
        }

        let mut ffi_types = Vec::with_capacity(count);
        for index in 0..count {
            let expected = func.params.get(index).copied();
            let actual = self.args.get(&index).cloned().unwrap_or(ArgValue::Null);
            let Some((ty, _)) = ffi_type_for(expected, &actual) else {
                set_error(
                    ERROR_EXCEPTION,
                    format!("Unsupported argument type at position {}", index),
                );
                return RESULT_VOID;
            };
            ffi_types.push(ty);
        }

        let Some((return_type, _)) = ffi_type_for(Some(func.ret), &ArgValue::Null) else {
            set_error(ERROR_EXCEPTION, "Unsupported C return type");
            return RESULT_VOID;
        };

        let cif = if func.variadic {
            Cif::new_variadic(ffi_types.clone(), func.params.len(), return_type.clone())
        } else {
            Cif::new(ffi_types.clone(), return_type.clone())
        };

        let mut strings: Vec<CString> = Vec::new();
        let mut storages: Vec<Storage> = Vec::with_capacity(count);
        for index in 0..count {
            let expected = func.params.get(index).copied();
            let actual = self.args.get(&index).cloned().unwrap_or(ArgValue::Null);
            let storage = match self.marshal(expected, &actual, &mut strings) {
                Ok(storage) => storage,
                Err(message) => {
                    set_error(ERROR_EXCEPTION, message);
                    return RESULT_VOID;
                }
            };
            storages.push(storage);
        }
        let args: Vec<Arg<'_>> = storages.iter().map(|storage| storage.as_arg()).collect();

        let return_info = type_clone(func.ret);
        let is_void = matches!(
            return_info.as_ref().map(|ty| &ty.kind),
            Some(TypeKind::Void)
        );
        let return_size = return_info.as_ref().map(|ty| ty.size).unwrap_or(0);
        // libffi writes the complete declared return value through the `ret` pointer. A fixed
        // 64-byte stack buffer silently overflowed the caller frame for larger struct returns.
        let mut stack = RetBuffer::default();
        let heap = if !is_void && return_size > stack.bytes.len() {
            let pointer = unsafe { libc::calloc(1, return_size) } as *mut u8;
            if pointer.is_null() {
                set_error(ERROR_EXCEPTION, "Out of memory");
                return RESULT_VOID;
            }
            Some(pointer)
        } else {
            None
        };
        let result = unsafe {
            std::slice::from_raw_parts_mut(
                heap.unwrap_or(stack.bytes.as_mut_ptr()),
                return_size.max(1),
            )
        };
        let return_code = type_clone(func.ret).map(|ty| ty.kind_code()).unwrap_or(0);
        let active_name = CString::new(self.function_name.as_str()).ok();
        if let Some(name) = &active_name {
            ACTIVE_CALLS.with(|calls| calls.borrow_mut().push(name.clone()));
        }
        unsafe {
            if is_void {
                cif.call_return_into(CodePtr(pointer as *mut c_void), &args, Ret::void());
            } else {
                cif.call_return_into(
                    CodePtr(pointer as *mut c_void),
                    &args,
                    Ret::new(result),
                );
            }
        }
        if active_name.is_some() {
            ACTIVE_CALLS.with(|calls| {
                calls.borrow_mut().pop();
            });
        }
        // The generated runtime exits through a raw syscall, so libc's stdout/stderr buffers
        // must be drained here or C `printf`/`puts` output is lost while PHP `echo` appears.
        unsafe {
            libc::fflush(std::ptr::null_mut());
        }
        self.store_result(func.ret, result, return_code);
        if let Some(pointer) = heap {
            unsafe {
                libc::free(pointer as *mut c_void);
            }
        }
        self.result_kind
    }

    /// Stores the raw return buffer in the PHP-facing result fields.
    fn store_result(&mut self, return_ty: TypeId, bytes: &[u8], _kind_code: u32) {
        let Some(ty) = type_clone(return_ty) else { return };
        match &ty.kind {
            TypeKind::Void => {
                self.result_kind = RESULT_VOID;
            }
            TypeKind::Bool => {
                self.result_kind = RESULT_BOOL;
                self.result_bool = bytes[0] != 0;
            }
            TypeKind::Char { .. } => {
                self.result_kind = RESULT_CHAR;
                self.result_int = i64::from(bytes[0]);
            }
            TypeKind::Int { bits, signed } => {
                self.result_kind = RESULT_INT;
                self.result_int = read_scalar_int(bytes, *bits, *signed);
            }
            TypeKind::Enum { underlying, .. } => {
                if let Some(inner) = type_clone(*underlying) {
                    if let TypeKind::Int { bits, signed } = inner.kind {
                        self.result_kind = RESULT_INT;
                        self.result_int = read_scalar_int(bytes, bits, signed);
                    }
                }
            }
            TypeKind::Float { bits: 32 } => {
                self.result_kind = RESULT_FLOAT;
                self.result_float = f64::from(f32::from_ne_bytes(bytes[..4].try_into().unwrap_or([0; 4])));
            }
            TypeKind::Float { .. } => {
                self.result_kind = RESULT_FLOAT;
                self.result_float = f64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
            }
            TypeKind::Pointer(inner) => {
                let pointer = usize::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
                let inner_ty = type_clone(*inner);
                if inner_ty.as_ref().is_some_and(|inner| matches!(inner.kind, TypeKind::Char { .. })) {
                    self.result_kind = RESULT_STRING;
                    self.result_bytes = read_c_string(pointer);
                    return;
                }
                let storage = unsafe { libc::calloc(1, crate::types::pointer_width()) as usize };
                if storage == 0 {
                    set_error(ERROR_EXCEPTION, "Out of memory");
                    return;
                }
                unsafe {
                    std::ptr::write_unaligned(storage as *mut usize, pointer);
                }
                let data = crate::context::CData {
                    addr: storage,
                    ty: return_ty,
                    allocated: true,
                    owned: true,
                    freed: false,
                    addr_temp: true,
                    readonly: false,
                    bit_width: None,
                    bit_offset: 0,
                    owns_pointee: false,
                    persistent: false,
                    source: 0,
                };
                self.result_cdata = Box::into_raw(Box::new(data)) as usize;
                self.result_kind = RESULT_CDATA;
            }
            TypeKind::Struct(_) => {
                let storage = unsafe { libc::calloc(1, ty.size.max(1)) as usize };
                if storage == 0 {
                    set_error(ERROR_EXCEPTION, "Out of memory");
                    return;
                }
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), storage as *mut u8, ty.size);
                }
                let data = crate::context::CData {
                    addr: storage,
                    ty: return_ty,
                    allocated: true,
                    owned: true,
                    freed: false,
                    addr_temp: false,
                    readonly: false,
                    bit_width: None,
                    bit_offset: 0,
                    owns_pointee: false,
                    persistent: false,
                    source: 0,
                };
                self.result_cdata = Box::into_raw(Box::new(data)) as usize;
                self.result_kind = RESULT_CDATA;
            }
            _ => {
                self.result_kind = RESULT_VOID;
            }
        }
    }

    /// Marshals one PHP argument into a stable storage cell.
    fn marshal(
        &self,
        expected: Option<TypeId>,
        actual: &ArgValue,
        strings: &mut Vec<CString>,
    ) -> Result<Storage, String> {
        let expected_ty = expected.and_then(type_clone);
        if let Some(ty) = &expected_ty {
            match &ty.kind {
                TypeKind::Pointer(_) | TypeKind::Func(_) => {
                    return self.marshal_pointer(actual, strings);
                }
                TypeKind::Array { .. } => {
                    return self.marshal_pointer(actual, strings);
                }
                TypeKind::Struct(_) => {
                    if let ArgValue::CData(handle) = actual {
                        let Some(data) = cdata(*handle) else {
                            return Err("Invalid FFI\\CData".to_string());
                        };
                        let copy = ty.size.max(1);
                        let mut bytes = vec![0u8; copy];
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                data.addr as *const u8,
                                bytes.as_mut_ptr(),
                                copy.min(ty.size),
                            );
                        }
                        return Ok(Storage::Bytes(bytes));
                    }
                    return Err("Incompatible C aggregate argument".to_string());
                }
                TypeKind::Float { bits: 32 } => {
                    return Ok(Storage::F32(self.scalar_float(actual)? as f32));
                }
                TypeKind::Float { .. } => return Ok(Storage::F64(self.scalar_float(actual)?)),
                TypeKind::Bool => return Ok(Storage::U8(u8::from(self.scalar_int(actual)? != 0))),
                TypeKind::Char { .. } => {
                    let value = self.scalar_int(actual)? as u8;
                    return Ok(Storage::U8(value));
                }
                TypeKind::Int { bits, .. } => {
                    let value = self.scalar_int(actual)?;
                    return Ok(storage_for_int(value, *bits));
                }
                TypeKind::Enum { underlying, .. } => {
                    let inner = type_clone(*underlying).ok_or("Invalid enum type")?;
                    let bits = match inner.kind {
                        TypeKind::Int { bits, .. } => bits,
                        _ => 32,
                    };
                    return Ok(storage_for_int(self.scalar_int(actual)?, bits));
                }
                TypeKind::Void => return Err("void argument".to_string()),
                _ => {}
            }
        }
        // Variadic position: apply C default argument promotions.
        match actual {
            ArgValue::Int(value) => Ok(Storage::I64(*value)),
            ArgValue::Float(value) => Ok(Storage::F64(*value)),
            ArgValue::String(bytes) => {
                let cstring = CString::new(bytes.clone()).map_err(|_| "string contains NUL".to_string())?;
                let pointer = cstring.as_ptr() as usize;
                strings.push(cstring);
                Ok(Storage::Pointer(pointer))
            }
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                Ok(Storage::Pointer(data_address(data)))
            }
            ArgValue::Pointer(value) => Ok(Storage::Pointer(*value)),
            ArgValue::Null => Ok(Storage::Pointer(0)),
        }
    }

    /// Marshals a pointer-shaped argument.
    fn marshal_pointer(
        &self,
        actual: &ArgValue,
        strings: &mut Vec<CString>,
    ) -> Result<Storage, String> {
        match actual {
            ArgValue::Null => Ok(Storage::Pointer(0)),
            ArgValue::Int(value) => Ok(Storage::Pointer(*value as usize)),
            ArgValue::String(bytes) => {
                let cstring = CString::new(bytes.clone()).map_err(|_| "string contains NUL".to_string())?;
                let pointer = cstring.as_ptr() as usize;
                strings.push(cstring);
                Ok(Storage::Pointer(pointer))
            }
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                let ty = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                let pointer = match ty.kind {
                    TypeKind::Pointer(_) => data.pointer_value(),
                    _ => data_address(data),
                };
                Ok(Storage::Pointer(pointer))
            }
            ArgValue::Float(value) => Ok(Storage::Pointer(*value as i64 as usize)),
            ArgValue::Pointer(value) => Ok(Storage::Pointer(*value)),
        }
    }

    /// Reads one CData/Int/Bool/Float argument as a PHP integer.
    fn scalar_int(&self, actual: &ArgValue) -> Result<i64, String> {
        match actual {
            ArgValue::Int(value) => Ok(*value),
            ArgValue::Float(value) => Ok(*value as i64),
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                let ty = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                if matches!(ty.kind, TypeKind::Float { .. }) {
                    return Ok(read_float(&Place {
                        addr: data.addr,
                        ty: data.ty,
                        bit_width: None,
                        bit_offset: 0,
                        readonly: true,
                        label: None,
                    }) as i64);
                }
                Ok(read_int(&Place {
                    addr: data.addr,
                    ty: data.ty,
                    bit_width: None,
                    bit_offset: 0,
                    readonly: true,
                    label: None,
                }))
            }
            ArgValue::Null => Ok(0),
            ArgValue::Pointer(value) => Ok(*value as i64),
            ArgValue::String(_) => Err("string passed to integer C parameter".to_string()),
        }
    }

    /// Reads one CData/Int/Float argument as a PHP float.
    fn scalar_float(&self, actual: &ArgValue) -> Result<f64, String> {
        match actual {
            ArgValue::Float(value) => Ok(*value),
            ArgValue::Int(value) => Ok(*value as f64),
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                let ty = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                if matches!(ty.kind, TypeKind::Float { .. }) {
                    return Ok(read_float(&Place {
                        addr: data.addr,
                        ty: data.ty,
                        bit_width: None,
                        bit_offset: 0,
                        readonly: true,
                        label: None,
                    }));
                }
                Ok(read_int(&Place {
                    addr: data.addr,
                    ty: data.ty,
                    bit_width: None,
                    bit_offset: 0,
                    readonly: true,
                    label: None,
                }) as f64)
            }
            ArgValue::Null => Ok(0.0),
            ArgValue::Pointer(value) => Ok(*value as f64),
            ArgValue::String(_) => Err("string passed to float C parameter".to_string()),
        }
    }
}

/// One stable argument storage cell.
enum Storage {
    I64(i64),
    U64(u64),
    U8(u8),
    F32(f32),
    F64(f64),
    Pointer(usize),
    Bytes(Vec<u8>),
}

impl Storage {
    /// Borrows the storage as a libffi argument.
    fn as_arg(&self) -> Arg<'_> {
        match self {
            Self::I64(value) => Arg::new(value),
            Self::U64(value) => Arg::new(value),
            Self::U8(value) => Arg::new(value),
            Self::F32(value) => Arg::new(value),
            Self::F64(value) => Arg::new(value),
            Self::Pointer(value) => Arg::new(value),
            Self::Bytes(bytes) => Arg::new(bytes.as_slice()),
        }
    }
}

thread_local! {
    /// C function names of the FFI calls currently executing on this thread.
    static ACTIVE_CALLS: std::cell::RefCell<Vec<CString>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Returns the innermost C function name's `(pointer, length)` on this thread.
///
/// The pointer addresses the thread-local CString and stays valid while the call frame is
/// still active, which is the only time a callback fatal reads it.
pub fn active_call_name_ptr_len() -> (usize, usize) {
    ACTIVE_CALLS.with(|calls| {
        calls.borrow().last().map_or((0, 0), |name| {
            (name.as_ptr() as usize, name.as_bytes().len())
        })
    })
}

/// A 16-byte-aligned stack buffer for ordinary scalar and small aggregate results.
///
/// Larger aggregate returns switch to a heap buffer sized from the declared type; libffi
/// writes the complete value through the return pointer.
#[repr(C, align(16))]
struct RetBuffer {
    bytes: [u8; 64],
}

impl Default for RetBuffer {
    /// Returns a zeroed, correctly aligned return buffer.
    fn default() -> Self {
        Self { bytes: [0; 64] }
    }
}

/// Maps one declared C type to a libffi type.
///
/// A top-level array parameter decays to a pointer, matching C. Arrays nested inside a
/// struct/union expand into repeated element types so libffi computes the same layout and
/// ABI classification as the C compiler.
pub(crate) fn declared_ffi_type(expected: TypeId) -> Option<Type> {
    declared_ffi_type_with_arrays(expected, true)
}

/// Maps one C type, choosing whether a top-level array decays to a pointer.
fn declared_ffi_type_with_arrays(expected: TypeId, array_as_pointer: bool) -> Option<Type> {
    let ty = type_clone(expected)?;
    let ffi = match &ty.kind {
        TypeKind::Void => Type::void(),
        TypeKind::Bool => Type::u8(),
        TypeKind::Char { signed } => if *signed { Type::i8() } else { Type::u8() },
        TypeKind::Int { bits, signed } => match (bits, signed) {
            (8, true) => Type::i8(),
            (8, false) => Type::u8(),
            (16, true) => Type::i16(),
            (16, false) => Type::u16(),
            (32, true) => Type::i32(),
            (32, false) => Type::u32(),
            (_, true) => Type::i64(),
            (_, false) => Type::u64(),
        },
        TypeKind::Enum { underlying, .. } => {
            return declared_ffi_type_with_arrays(*underlying, array_as_pointer)
        }
        TypeKind::Float { bits: 32 } => Type::f32(),
        // Extended-precision `long double` has a target-specific 80/128-bit representation;
        // the bridge does not marshal it yet and must not silently pass an f64 in its slot.
        TypeKind::Float { bits: 80 } => return None,
        TypeKind::Float { .. } => Type::f64(),
        TypeKind::Array { element, len } if !array_as_pointer => {
            let len = (*len)?;
            if len == 0 {
                return None;
            }
            let mut elements = Vec::with_capacity(len);
            for _ in 0..len {
                elements.push(declared_ffi_type_with_arrays(*element, false)?);
            }
            Type::structure(elements)
        }
        TypeKind::Array { .. } | TypeKind::Pointer(_) | TypeKind::Func(_) => Type::pointer(),
        TypeKind::Struct(record) => {
            if record.incomplete {
                return None;
            }
            if record.union {
                // libffi has no union type. The first field's type is exact when it owns the
                // union's whole size/alignment (php-src's rule); otherwise the ABI
                // classification cannot be represented and the call fails loudly.
                let first = record.fields.first()?;
                let first_info = type_clone(first.ty)?;
                if first_info.size != ty.size || first_info.align != ty.align {
                    return None;
                }
                return declared_ffi_type_with_arrays(first.ty, false);
            }
            let fields = record
                .fields
                .iter()
                .map(|field| declared_ffi_type_with_arrays(field.ty, false))
                .collect::<Option<Vec<_>>>()?;
            Type::structure(fields)
        }
        TypeKind::IncompleteTag { .. } => return None,
    };
    Some(ffi)
}

/// Maps one C type (or a variadic PHP argument) to a libffi type.
fn ffi_type_for(expected: Option<TypeId>, actual: &ArgValue) -> Option<(Type, bool)> {
    if let Some(expected) = expected {
        if let Some(ffi) = declared_ffi_type(expected) {
            return Some((ffi, true));
        }
    }
    let ffi = match actual {
        ArgValue::Int(_) => Type::i64(),
        ArgValue::Float(_) => Type::f64(),
        ArgValue::String(_) | ArgValue::CData(_) | ArgValue::Pointer(_) | ArgValue::Null => {
            Type::pointer()
        }
    };
    Some((ffi, false))
}

/// Returns whether a PHP argument can satisfy one declared C parameter.
fn argument_is_compatible(expected: &crate::types::CType, actual: &ArgValue) -> bool {
    use crate::types::TypeKind;
    match &expected.kind {
        TypeKind::Void => false,
        TypeKind::Pointer(inner) => {
            let char_pointer = type_clone(*inner)
                .is_some_and(|inner| matches!(inner.kind, TypeKind::Char { .. }));
            if char_pointer {
                matches!(
                    actual,
                    ArgValue::Null | ArgValue::String(_) | ArgValue::CData(_) | ArgValue::Pointer(_)
                )
            } else {
                matches!(
                    actual,
                    ArgValue::Null | ArgValue::String(_) | ArgValue::CData(_) | ArgValue::Pointer(_)
                )
            }
        }
        TypeKind::Func(_) => matches!(
            actual,
            ArgValue::Null | ArgValue::CData(_) | ArgValue::Pointer(_)
        ),
        TypeKind::Array { .. } | TypeKind::Struct(_) => {
            matches!(actual, ArgValue::Null | ArgValue::CData(_))
        }
        TypeKind::Int { .. } | TypeKind::Enum { .. } | TypeKind::Char { .. } | TypeKind::Bool => {
            matches!(actual, ArgValue::Int(_) | ArgValue::Float(_) | ArgValue::CData(_))
        }
        TypeKind::Float { .. } => {
            matches!(actual, ArgValue::Int(_) | ArgValue::Float(_) | ArgValue::CData(_))
        }
        TypeKind::IncompleteTag { .. } => false,
    }
}

/// Returns PHP's type label for one call argument.
fn arg_php_label(actual: &ArgValue) -> &'static str {
    match actual {
        ArgValue::Int(_) => "int",
        ArgValue::Float(_) => "float",
        ArgValue::String(_) => "string",
        ArgValue::CData(_) => r"FFI\CData",
        ArgValue::Pointer(_) => "pointer",
        ArgValue::Null => "null",
    }
}

fn storage_for_int(value: i64, bits: u8) -> Storage {
    match bits {
        8 => Storage::U8(value as u8),
        16 => Storage::U64(value as u16 as u64),
        32 => Storage::U64(value as u32 as u64),
        _ => Storage::I64(value),
    }
}

fn read_scalar_int(bytes: &[u8], bits: u8, signed: bool) -> i64 {
    match bits {
        8 => if signed { i64::from(bytes[0] as i8) } else { i64::from(bytes[0]) },
        16 => {
            let value = u16::from_ne_bytes(bytes[..2].try_into().unwrap_or([0; 2]));
            if signed { i64::from(value as i16) } else { i64::from(value) }
        }
        32 => {
            let value = u32::from_ne_bytes(bytes[..4].try_into().unwrap_or([0; 4]));
            if signed { i64::from(value as i32) } else { i64::from(value) }
        }
        _ => i64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8])),
    }
}

fn read_c_string(pointer: usize) -> Vec<u8> {
    if pointer == 0 {
        return Vec::new();
    }
    unsafe {
        let mut length = 0usize;
        while std::ptr::read(pointer.wrapping_add(length) as *const u8) != 0 && length <= 1 << 30 {
            length += 1;
        }
        let mut bytes = vec![0u8; length];
        std::ptr::copy_nonoverlapping(pointer as *const u8, bytes.as_mut_ptr(), length);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::os::raw::c_void;

    /// libffi closure that fills a large struct return buffer with a recognizable byte.
    unsafe extern "C" fn fill_large_struct(
        _cif: *mut libffi::raw::ffi_cif,
        result: *mut c_void,
        _args: *mut *mut c_void,
        _userdata: *mut c_void,
    ) {
        std::ptr::write_bytes(result as *mut u8, 0xAB, 512);
    }

    /// Opens a throwaway cdef context for a test declaration string.
    fn context_for(code: &str) -> usize {
        let code = CString::new(code).expect("cdef");
        let context = crate::abi::elephc_ffi_cdef(code.as_ptr(), std::ptr::null());
        assert_ne!(context, 0, "cdef must parse");
        context
    }

    /// Verifies a struct return larger than the old 64-byte stack buffer is copied whole.
    #[test]
    fn large_struct_return_uses_a_sized_buffer() {
        let context = context_for(
            "struct big { uint64_t data[64]; }; struct big make_big(void);",
        );
        let function_ty = crate::context::context(context)
            .and_then(|context| context.functions.get("make_big").copied())
            .expect("make_big declaration");
        let function = match crate::context::type_clone(function_ty).expect("function type").kind {
            TypeKind::Func(function) => function,
            other => panic!("expected function type, got {other:?}"),
        };
        let return_ty = declared_ffi_type(function.ret).expect("large struct libffi type");
        let cif = Cif::new(std::iter::empty(), return_ty);
        let (closure, code) = libffi::low::try_closure_alloc().expect("closure allocation");
        let status = unsafe {
            libffi::raw::ffi_prep_closure_loc(
                closure as *mut libffi::raw::ffi_closure,
                cif.as_raw_ptr(),
                Some(fill_large_struct),
                std::ptr::null_mut(),
                code.as_mut_ptr(),
            )
        };
        assert_eq!(status, libffi::raw::ffi_status_FFI_OK);

        let pointer = crate::context::pointer_to(code.as_ptr() as usize, function_ty)
            .expect("function pointer CData");
        let mut call = CallBuilder::new_cdata(pointer).expect("call builder");
        assert_eq!(call.invoke(), RESULT_CDATA);
        let result = crate::context::cdata(call.result_cdata).expect("result CData");
        assert_eq!(
            crate::context::type_clone(result.ty).expect("result type").size,
            512
        );
        let bytes = unsafe { std::slice::from_raw_parts(result.addr as *const u8, 512) };
        assert!(bytes.iter().all(|byte| *byte == 0xAB));

        crate::context::release_cdata(call.result_cdata);
        crate::context::release_cdata(pointer);
        unsafe {
            libffi::raw::ffi_closure_free(closure.cast());
        }
    }

    /// Verifies array fields expand and a safe union keeps the declared CIF size.
    #[test]
    fn cif_layout_matches_array_and_union_fields() {
        let context = context_for(
            "union exact { uint32_t a; uint32_t b; }; \
             union mixed { char a; uint32_t b; }; \
             struct record { char name[8]; uint32_t x; };",
        );
        for (name, expected) in [("union exact", 4usize), ("struct record", 12)] {
            let ty = crate::context::resolve_type(context, name).expect("declared type");
            let ffi = declared_ffi_type(ty).expect("representable CIF type");
            let raw = ffi.as_raw_ptr();
            // A CIF argument type forces libffi to compute the structure's size/alignment.
            let cif = Cif::new(vec![ffi], Type::void());
            let size = unsafe { (*raw).size };
            assert_eq!(size, expected, "{name} CIF size");
            drop(cif);
        }

        let mixed = crate::context::resolve_type(context, "union mixed").expect("mixed union");
        assert!(
            declared_ffi_type(mixed).is_none(),
            "a union whose first field does not own its size must fail loudly"
        );
    }
}
