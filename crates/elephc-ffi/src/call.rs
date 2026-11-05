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
    alloc_aligned, cdata, data_address, set_error, type_clone, ERROR_EXCEPTION,
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
    /// PHP integer.
    Int(i64),
    /// PHP bool, kept distinct so diagnostics report PHP's type name.
    Bool(bool),
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
        if data.check_alive().is_err() {
            set_error(ERROR_EXCEPTION, "Use after free()");
            return None;
        }
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
        crate::context::clear_error();
        self.result_kind = RESULT_VOID;
        self.result_int = 0;
        self.result_float = 0.0;
        self.result_bool = false;
        self.result_bytes.clear();
        self.result_cdata = 0;

        let Some(function) = type_clone(self.function_type) else {
            set_error(ERROR_EXCEPTION, "Invalid C function type");
            return RESULT_ERROR;
        };
        let TypeKind::Func(func) = function.kind else {
            set_error(ERROR_EXCEPTION, "Invalid C function type");
            return RESULT_ERROR;
        };
        let pointer = if self.direct_pointer != 0 {
            self.direct_pointer
        } else {
            let Some(context) = crate::context::context(self.context) else {
                set_error(ERROR_EXCEPTION, "Invalid FFI context");
                return RESULT_ERROR;
            };
            match context.symbol(&self.function_name) {
                Some(pointer) => pointer,
                None => {
                    set_error(
                        ERROR_EXCEPTION,
                        format!("Attempt to call undefined C function '{}'", self.function_name),
                    );
                    return RESULT_ERROR;
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
                    "Incorrect number of arguments for C function '{}', expecting at least {} parameter{}",
                    self.function_name,
                    func.params.len(),
                    if func.params.len() == 1 { "" } else { "s" }
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
                return RESULT_ERROR;
            };
            ffi_types.push(ty);
        }

        let Some((return_type, _)) = ffi_type_for(Some(func.ret), &ArgValue::Null) else {
            set_error(ERROR_EXCEPTION, "Unsupported C return type");
            return RESULT_ERROR;
        };

        let cif = if func.variadic {
            Cif::new_variadic(ffi_types.clone(), func.params.len(), return_type.clone())
        } else {
            Cif::new(ffi_types.clone(), return_type.clone())
        };

        let mut strings: Vec<Vec<u8>> = Vec::new();
        let mut storages: Vec<Storage> = Vec::with_capacity(count);
        for index in 0..count {
            let expected = func.params.get(index).copied();
            let actual = self.args.get(&index).cloned().unwrap_or(ArgValue::Null);
            let storage = match self.marshal(expected, &actual, &mut strings) {
                Ok(storage) => storage,
                Err(message) => {
                    set_error(ERROR_EXCEPTION, message);
                    return RESULT_ERROR;
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
        let return_align = return_info.as_ref().map_or(1, |ty| ty.align) as usize;
        // libffi writes the complete declared return value through the `ret` pointer. A fixed
        // 64-byte stack buffer silently overflowed the caller frame for larger struct returns,
        // and its 16-byte alignment is insufficient for over-aligned aggregates.
        let mut stack = RetBuffer::default();
        let heap = if !is_void && (return_size > stack.bytes.len() || return_align > 16) {
            let pointer = alloc_aligned(return_size, return_align) as *mut u8;
            if pointer.is_null() {
                set_error(ERROR_EXCEPTION, "Out of memory");
                return RESULT_ERROR;
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
            TypeKind::Float { bits: 64 } => {
                self.result_kind = RESULT_FLOAT;
                self.result_float = f64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
            }
            TypeKind::Float { .. } => {
                self.result_kind = RESULT_FLOAT;
                self.result_float = crate::long_double::decode(bytes, ty.size);
            }
            TypeKind::Pointer(inner) => {
                let pointer = usize::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
                let inner_ty = type_clone(*inner);
                if inner_ty.as_ref().is_some_and(|inner| matches!(inner.kind, TypeKind::Char { .. })) {
                    if pointer == 0 {
                        self.result_kind = RESULT_VOID;
                        return;
                    }
                    self.result_kind = RESULT_STRING;
                    self.result_bytes = read_c_string(pointer);
                    return;
                }
                let storage = alloc_aligned(crate::types::pointer_width(), crate::types::pointer_width());
                if storage == 0 {
                    set_error(ERROR_EXCEPTION, "Out of memory");
                    self.result_kind = RESULT_ERROR;
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
                    parent: 0,
                };
                self.result_cdata = Box::into_raw(Box::new(data)) as usize;
                self.result_kind = RESULT_CDATA;
            }
            TypeKind::Struct(_) => {
                let storage = alloc_aligned(ty.size, ty.align);
                if storage == 0 {
                    set_error(ERROR_EXCEPTION, "Out of memory");
                    self.result_kind = RESULT_ERROR;
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
                    parent: 0,
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
        strings: &mut Vec<Vec<u8>>,
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
                TypeKind::Float { bits: 64 } => return Ok(Storage::F64(self.scalar_float(actual)?)),
                TypeKind::Float { .. } => {
                    let bytes = match actual {
                        ArgValue::CData(handle) => {
                            let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                            if data.check_alive().is_err() {
                                return Err("Use after free()".to_string());
                            }
                            let source = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                            if !matches!(source.kind, TypeKind::Float { .. }) || source.size != ty.size {
                                return Err("Incompatible C float argument".to_string());
                            }
                            let mut bytes = [0u8; 16];
                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    data.addr as *const u8,
                                    bytes.as_mut_ptr(),
                                    ty.size.min(16),
                                );
                            }
                            bytes
                        }
                        _ => crate::long_double::encode(self.scalar_float(actual)?, ty.size),
                    };
                    return Ok(Storage::LongDouble(bytes));
                }
                TypeKind::Bool => return Ok(Storage::U8(u8::from(Self::scalar_bool(actual)?))),
                TypeKind::Char { .. } => {
                    let value = self.scalar_char(actual)?;
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
            ArgValue::Bool(value) => Ok(Storage::I64(i64::from(*value))),
            ArgValue::Float(value) => Ok(Storage::F64(*value)),
            ArgValue::String(bytes) => Ok(Storage::Pointer(push_c_string(strings, bytes))),
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                if data.check_alive().is_err() {
                    return Err("Use after free()".to_string());
                }
                let ty = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                match ty.kind {
                    // A variadic position applies PHP's CData-to-scalar conversion, not a
                    // pointer: `printf("%g", $doubleCData)` passes the double value.
                    TypeKind::Float { bits: 80 | 128 } => {
                        let mut bytes = [0u8; 16];
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                data.addr as *const u8,
                                bytes.as_mut_ptr(),
                                ty.size.min(16),
                            );
                        }
                        Ok(Storage::LongDouble(bytes))
                    }
                    TypeKind::Float { .. } => Ok(Storage::F64(self.scalar_float(actual)?)),
                    TypeKind::Int { .. }
                    | TypeKind::Char { .. }
                    | TypeKind::Enum { .. }
                    | TypeKind::Bool => Ok(Storage::I64(self.scalar_int(actual)?)),
                    TypeKind::Pointer(_) => Ok(Storage::Pointer(data.pointer_value())),
                    _ => Ok(Storage::Pointer(data_address(data))),
                }
            }
            ArgValue::Pointer(value) => Ok(Storage::Pointer(*value)),
            ArgValue::Null => Ok(Storage::Pointer(0)),
        }
    }

    /// Marshals a pointer-shaped argument.
    fn marshal_pointer(
        &self,
        actual: &ArgValue,
        strings: &mut Vec<Vec<u8>>,
    ) -> Result<Storage, String> {
        match actual {
            ArgValue::Null => Ok(Storage::Pointer(0)),
            ArgValue::String(bytes) => Ok(Storage::Pointer(push_c_string(strings, bytes))),
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                if data.check_alive().is_err() {
                    return Err("Use after free()".to_string());
                }
                let ty = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                let pointer = match ty.kind {
                    TypeKind::Pointer(_) => data.pointer_value(),
                    _ => data_address(data),
                };
                Ok(Storage::Pointer(pointer))
            }
            ArgValue::Pointer(value) => Ok(Storage::Pointer(*value)),
            _ => Err("incompatible pointer argument".to_string()),
        }
    }

    /// Reads one CData/Int/Bool/Float argument as a PHP boolean.
    ///
    /// PHP's `zend_is_true` tests a float against zero instead of truncating it first, so
    /// `0.5` and `-0.5` are true for a C `_Bool` parameter. A floating-point CData source
    /// follows the same rule.
    fn scalar_bool(actual: &ArgValue) -> Result<bool, String> {
        match actual {
            ArgValue::Bool(value) => Ok(*value),
            ArgValue::Float(value) => Ok(*value != 0.0),
            ArgValue::Int(value) => Ok(*value != 0),
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                if data.check_alive().is_err() {
                    return Err("Use after free()".to_string());
                }
                let ty = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                let place = Place {
                    addr: data.addr,
                    ty: data.ty,
                    bit_width: None,
                    bit_offset: 0,
                    readonly: true,
                    label: None,
                    parent: 0,
                };
                if matches!(ty.kind, TypeKind::Float { .. }) {
                    return Ok(read_float(&place) != 0.0);
                }
                Ok(read_int(&place) != 0)
            }
            ArgValue::Null => Ok(false),
            ArgValue::Pointer(value) => Ok(*value != 0),
            ArgValue::String(_) => Err("string passed to boolean C parameter".to_string()),
        }
    }

    /// Reads one CData/Int/Bool/Float argument as a PHP integer.
    fn scalar_int(&self, actual: &ArgValue) -> Result<i64, String> {
        match actual {
            ArgValue::Int(value) => Ok(*value),
            ArgValue::Bool(value) => Ok(i64::from(*value)),
            ArgValue::Float(value) => Ok(*value as i64),
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                if data.check_alive().is_err() {
                    return Err("Use after free()".to_string());
                }
                let ty = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                if matches!(ty.kind, TypeKind::Float { .. }) {
                    return Ok(read_float(&Place {
                        addr: data.addr,
                        ty: data.ty,
                        bit_width: None,
                        bit_offset: 0,
                        readonly: true,
                        label: None,
                        parent: 0,
                    }) as i64);
                }
                Ok(read_int(&Place {
                    addr: data.addr,
                    ty: data.ty,
                    bit_width: None,
                    bit_offset: 0,
                    readonly: true,
                    label: None,
                    parent: 0,
                }))
            }
            ArgValue::Null => Ok(0),
            ArgValue::Pointer(value) => Ok(*value as i64),
            ArgValue::String(_) => Err("string passed to integer C parameter".to_string()),
        }
    }

    /// Reads one CData/Int/Float argument as a PHP float.
    /// Reads a one-byte PHP string or scalar argument for a C `char` parameter.
    fn scalar_char(&self, actual: &ArgValue) -> Result<u8, String> {
        if let ArgValue::String(bytes) = actual {
            return match bytes.as_slice() {
                [byte] => Ok(*byte),
                _ => Err("incompatible string for a C char parameter".to_string()),
            };
        }
        Ok(self.scalar_int(actual)? as u8)
    }

    fn scalar_float(&self, actual: &ArgValue) -> Result<f64, String> {
        match actual {
            ArgValue::Float(value) => Ok(*value),
            ArgValue::Int(value) => Ok(*value as f64),
            ArgValue::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
            ArgValue::CData(handle) => {
                let data = cdata(*handle).ok_or("Invalid FFI\\CData")?;
                if data.check_alive().is_err() {
                    return Err("Use after free()".to_string());
                }
                let ty = type_clone(data.ty).ok_or("Invalid FFI\\CData")?;
                if matches!(ty.kind, TypeKind::Float { .. }) {
                    return Ok(read_float(&Place {
                        addr: data.addr,
                        ty: data.ty,
                        bit_width: None,
                        bit_offset: 0,
                        readonly: true,
                        label: None,
                        parent: 0,
                    }));
                }
                Ok(read_int(&Place {
                    addr: data.addr,
                    ty: data.ty,
                    bit_width: None,
                    bit_offset: 0,
                    readonly: true,
                    label: None,
                    parent: 0,
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
    LongDouble([u8; 16]),
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
            Self::LongDouble(value) => Arg::new(value),
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
        TypeKind::Float { bits: 64 } => Type::f64(),
        // `longdouble` maps to the target ABI's own extended type: x87 80-bit on
        // linux-x86_64, binary128 on linux-aarch64, and a 64-bit double on Apple targets.
        TypeKind::Float { .. } => Type::longdouble(),
        TypeKind::Array { element, len } if !array_as_pointer => {
            let len = (*len)?;
            if len == 0 {
                return None;
            }
            if has_explicit_over_alignment(&ty) {
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
            if record.incomplete || record.fields.is_empty() || ty.size == 0 {
                return None;
            }
            if has_explicit_over_alignment(&ty) {
                return None;
            }
            // A packed record's libffi structure must reproduce the parser's padding exactly;
            // bit-field containers follow php-src's own 32-bit rule. Passing the expanded field
            // list would let libffi read/write past the parser-sized argument buffer, so the
            // call fails loudly until those layouts are modelled explicitly.
            if record.packed || record.fields.iter().any(|field| field.bit_width.is_some()) {
                return None;
            }
            if record.union {
                // libffi has no union type. Reuse the first field only when it owns the
                // union's whole size/alignment AND every member uses the same calling class:
                // a `union { double; uint64_t; }` travels in integer registers on x86_64, so
                // describing it as a double would corrupt valid calls and callbacks.
                let first = record.fields.first()?;
                let first_info = type_clone(first.ty)?;
                if first_info.size != ty.size
                    || first_info.align != ty.align
                    || !union_members_share_abi_class(record)
                {
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

/// Returns the natural C alignment for a type, ignoring explicit `aligned` attributes.
fn natural_alignment(ty: &crate::types::CType) -> usize {
    match &ty.kind {
        TypeKind::Void | TypeKind::Bool | TypeKind::Char { .. } => 1,
        TypeKind::Int { bits, .. } => (*bits as usize / 8).max(1),
        TypeKind::Enum { underlying, .. } => type_clone(*underlying)
            .map_or(1, |inner| natural_alignment(&inner)),
        TypeKind::Float { bits } => {
            if *bits <= 32 { 4 } else if *bits <= 64 { 8 } else { 16 }
        }
        TypeKind::Pointer(_) | TypeKind::Func(_) => crate::types::pointer_width(),
        TypeKind::Array { element, .. } => type_clone(*element)
            .map_or(1, |inner| natural_alignment(&inner)),
        TypeKind::Struct(record) => record
            .fields
            .iter()
            .filter_map(|field| type_clone(field.ty).map(|inner| natural_alignment(&inner)))
            .max()
            .unwrap_or(1),
        TypeKind::IncompleteTag { .. } => 1,
    }
}

/// Returns whether a type or one of its members carries explicit over-alignment.
///
/// libffi lays a record out from its field types alone, so `__attribute__((aligned(N)))`
/// changes the C ABI without changing the libffi description. Such records are rejected
/// before a call instead of being passed with the wrong size/alignment.
fn has_explicit_over_alignment(ty: &crate::types::CType) -> bool {
    if ty.align > natural_alignment(ty) {
        return true;
    }
    match &ty.kind {
        TypeKind::Struct(record) => record.fields.iter().any(|field| {
            type_clone(field.ty).is_some_and(|inner| has_explicit_over_alignment(&inner))
        }),
        TypeKind::Array { element, .. } => type_clone(*element)
            .is_some_and(|inner| has_explicit_over_alignment(&inner)),
        _ => false,
    }
}

/// Returns whether every scalar union member uses the same libffi calling class.
///
/// Aggregate members are rejected: their class depends on every nested field, which the
/// first-member shortcut cannot represent safely.
fn union_members_share_abi_class(record: &crate::types::StructType) -> bool {
    let mut class = None;
    for field in &record.fields {
        let Some(ty) = type_clone(field.ty) else { return false };
        let current = match ty.kind {
            TypeKind::Float { bits } => 100 + u32::from(bits),
            TypeKind::Int { .. }
            | TypeKind::Char { .. }
            | TypeKind::Enum { .. }
            | TypeKind::Bool
            | TypeKind::Pointer(_)
            | TypeKind::Func(_) => 0,
            _ => return false,
        };
        if class.is_some_and(|existing| existing != current) {
            return false;
        }
        class = Some(current);
    }
    class.is_some()
}

/// Maps one C type (or a variadic PHP argument) to a libffi type.
fn ffi_type_for(expected: Option<TypeId>, actual: &ArgValue) -> Option<(Type, bool)> {
    if let Some(expected) = expected {
        // A declared parameter whose C type libffi cannot represent must fail instead of
        // silently substituting the PHP argument's classifier: an 80-bit long double slot
        // filled as an i64, or a packed/bit-field aggregate sent as a pointer, is an ABI
        // miscompile that corrupts the callee's frame.
        return declared_ffi_type(expected).map(|ffi| (ffi, true));
    }
    let ffi = match actual {
        ArgValue::Int(_) | ArgValue::Bool(_) => Type::i64(),
        ArgValue::Float(_) => Type::f64(),
        ArgValue::CData(handle) => {
            let kind = cdata(*handle).and_then(|data| type_clone(data.ty)).map(|ty| ty.kind);
            match kind {
                Some(TypeKind::Float { bits: 32 } | TypeKind::Float { bits: 64 }) => Type::f64(),
                Some(TypeKind::Float { .. }) => Type::longdouble(),
                Some(TypeKind::Int { .. })
                | Some(TypeKind::Char { .. })
                | Some(TypeKind::Enum { .. })
                | Some(TypeKind::Bool) => Type::i64(),
                _ => Type::pointer(),
            }
        }
        ArgValue::String(_) | ArgValue::Pointer(_) | ArgValue::Null => Type::pointer(),
    };
    Some((ffi, false))
}

/// Returns whether two C pointee types are compatible for a pointer argument.
fn pointee_types_compatible(expected: TypeId, actual: TypeId) -> bool {
    let (Some(expected), Some(actual)) = (type_clone(expected), type_clone(actual)) else {
        return false;
    };
    if matches!(expected.kind, TypeKind::Void) || matches!(actual.kind, TypeKind::Void) {
        return true;
    }
    match (&expected.kind, &actual.kind) {
        (TypeKind::Pointer(left), TypeKind::Pointer(right)) => {
            pointee_types_compatible(*left, *right)
        }
        (TypeKind::Array { element: left, len: left_len },
         TypeKind::Array { element: right, len: right_len }) => {
            let lengths_match = match (left_len, right_len) {
                (Some(left), Some(right)) => left == right,
                _ => true,
            };
            lengths_match && pointee_types_compatible(*left, *right)
        }
        (TypeKind::Func(left), TypeKind::Func(right)) => {
            crate::abi::function_signatures_match(left, right)
        }
        (TypeKind::Struct(_), TypeKind::Struct(_)) => {
            expected.display == actual.display && expected.size == actual.size
        }
        // An incomplete tag is the same C type before and after its definition. php-src's
        // own type identity accepts `struct _IO_FILE *` against a `FILE *` typedef.
        (TypeKind::IncompleteTag { .. }, TypeKind::IncompleteTag { .. })
        | (TypeKind::IncompleteTag { .. }, TypeKind::Struct(_))
        | (TypeKind::Struct(_), TypeKind::IncompleteTag { .. }) => {
            expected.display == actual.display
        }
        (TypeKind::Enum { .. }, TypeKind::Enum { .. }) => expected.display == actual.display,
        (TypeKind::Int { .. }, TypeKind::Int { .. })
        | (TypeKind::Float { .. }, TypeKind::Float { .. })
        | (TypeKind::Char { .. }, TypeKind::Char { .. })
        | (TypeKind::Bool, TypeKind::Bool) => {
            expected.kind_code() == actual.kind_code()
        }
        _ => false,
    }
}

/// Returns whether a CData argument can satisfy a declared non-function pointer parameter.
fn pointer_cdata_compatible(expected_inner: TypeId, handle: usize) -> bool {
    let Some(data) = cdata(handle) else { return false };
    if data.check_alive().is_err() {
        return false;
    }
    let Some(source) = type_clone(data.ty) else { return false };
    match source.kind {
        TypeKind::Pointer(source_inner) => pointee_types_compatible(expected_inner, source_inner),
        TypeKind::Array { element, .. } => pointee_types_compatible(expected_inner, element),
        _ => false,
    }
}

/// Returns whether a CData/raw pointer argument matches a declared function-pointer parameter.
///
/// php-src requires the same calling convention, variadic flag, return type, and parameter
/// types before it writes the CData's pointer into a function-pointer argument slot. An
/// integer CData or a differently shaped function pointer would make C jump into data memory
/// or call through the wrong ABI.
fn function_pointer_argument_matches(expected: &crate::types::CType, actual: &ArgValue) -> bool {
    use crate::types::TypeKind;
    match actual {
        ArgValue::Null | ArgValue::Pointer(_) => true,
        ArgValue::CData(handle) => {
            let Some(data) = cdata(*handle) else { return false };
            if data.check_alive().is_err() {
                return false;
            }
            let Some(source) = type_clone(data.ty) else { return false };
            let TypeKind::Pointer(source_inner) = source.kind else { return false };
            let Some(source_func) = type_clone(source_inner).and_then(|ty| match ty.kind {
                TypeKind::Func(function) => Some(function),
                _ => None,
            }) else {
                return false;
            };
            let expected_func = match &expected.kind {
                TypeKind::Pointer(inner) => type_clone(*inner).and_then(|ty| match ty.kind {
                    TypeKind::Func(function) => Some(function),
                    _ => None,
                }),
                TypeKind::Func(function) => Some(function.clone()),
                _ => None,
            };
            expected_func.is_some_and(|expected_func| {
                crate::abi::function_signatures_match(&expected_func, &source_func)
            })
        }
        _ => false,
    }
}

/// Returns whether a PHP argument can satisfy one declared C parameter.
fn argument_is_compatible(expected: &crate::types::CType, actual: &ArgValue) -> bool {
    use crate::types::TypeKind;
    match &expected.kind {
        TypeKind::Void => false,
        TypeKind::Pointer(inner) => {
            let inner_info = type_clone(*inner);
            if inner_info
                .as_ref()
                .is_some_and(|inner| matches!(inner.kind, TypeKind::Func(_)))
            {
                return function_pointer_argument_matches(expected, actual);
            }
            let text_pointer = inner_info.is_some_and(|inner| {
                matches!(inner.kind, TypeKind::Char { .. } | TypeKind::Void)
            });
            match actual {
                ArgValue::String(_) => text_pointer,
                ArgValue::Null | ArgValue::Pointer(_) => true,
                ArgValue::CData(handle) => pointer_cdata_compatible(*inner, *handle),
                _ => false,
            }
        }
        TypeKind::Func(_) => function_pointer_argument_matches(expected, actual),
        TypeKind::Array { .. } | TypeKind::Struct(_) => match actual {
            ArgValue::CData(handle) => cdata(*handle).is_some_and(|data| {
                data.check_alive().is_ok()
                    && type_clone(data.ty).is_some_and(|source| {
                    source.size == expected.size && source.display == expected.display
                })
            }),
            _ => false,
        },
        TypeKind::Char { .. } => match actual {
            // php-src accepts a one-byte PHP string for a C `char` parameter.
            ArgValue::String(bytes) => bytes.len() == 1,
            _ => matches!(actual, ArgValue::Int(_) | ArgValue::Bool(_) | ArgValue::Float(_) | ArgValue::CData(_)),
        },
        TypeKind::Int { .. } | TypeKind::Enum { .. } | TypeKind::Bool => {
            matches!(actual, ArgValue::Int(_) | ArgValue::Bool(_) | ArgValue::Float(_) | ArgValue::CData(_))
        }
        TypeKind::Float { .. } => {
            matches!(actual, ArgValue::Int(_) | ArgValue::Bool(_) | ArgValue::Float(_) | ArgValue::CData(_))
        }
        TypeKind::IncompleteTag { .. } => false,
    }
}

/// Returns PHP's type label for one call argument.
fn arg_php_label(actual: &ArgValue) -> &'static str {
    match actual {
        ArgValue::Int(_) => "int",
        ArgValue::Bool(_) => "bool",
        ArgValue::Float(_) => "float",
        ArgValue::String(_) => "string",
        ArgValue::CData(_) => r"FFI\CData",
        ArgValue::Pointer(_) => "pointer",
        ArgValue::Null => "null",
    }
}

/// Copies PHP string bytes into a NUL-terminated C string and keeps it alive for the call.
///
/// php-src copies the whole PHP string and appends a terminator, so interior NULs are passed
/// through to C instead of rejecting the call.
fn push_c_string(strings: &mut Vec<Vec<u8>>, bytes: &[u8]) -> usize {
    let mut buffer = Vec::with_capacity(bytes.len() + 1);
    buffer.extend_from_slice(bytes);
    buffer.push(0);
    let pointer = buffer.as_ptr() as usize;
    strings.push(buffer);
    pointer
}

/// Maps an integer value to the libffi storage cell for its declared width.
fn storage_for_int(value: i64, bits: u8) -> Storage {
    match bits {
        8 => Storage::U8(value as u8),
        16 => Storage::U64(value as u16 as u64),
        32 => Storage::U64(value as u32 as u64),
        _ => Storage::I64(value),
    }
}

/// Reads a scalar integer return from the raw libffi return buffer.
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

/// Copies a NUL-terminated C string into owned bytes with a one-GiB safety cap.
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
        let context = context_for("struct big { uint64_t data[64]; };");
        // Eager cdef resolution rejects a declaration whose symbol does not exist yet, so the
        // test resolves the equivalent function-pointer type directly.
        let pointer_ty = crate::context::resolve_type(context, "struct big (*)(void)")
            .expect("function pointer type");
        let function_ty = match crate::context::type_clone(pointer_ty).expect("pointer type").kind {
            TypeKind::Pointer(inner) => inner,
            other => panic!("expected function pointer, got {other:?}"),
        };
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

    /// A one-byte PHP string is a valid C `char` argument; longer strings are rejected.
    #[test]
    fn char_parameter_accepts_a_one_byte_string() {
        let context = context_for("typedef char test_char;");
        let char_ty = crate::context::resolve_type(context, "test_char").expect("char type");
        let char_info = crate::context::type_clone(char_ty).expect("char info");
        assert!(argument_is_compatible(&char_info, &ArgValue::String(b"A".to_vec())));
        assert!(!argument_is_compatible(&char_info, &ArgValue::String(b"AB".to_vec())));
    }

    /// Verifies `_Bool` arguments test floats against zero instead of truncating them.
    #[test]
    fn bool_arguments_test_floats_against_zero() {
        assert!(CallBuilder::scalar_bool(&ArgValue::Float(0.5)).expect("0.5"));
        assert!(CallBuilder::scalar_bool(&ArgValue::Float(-0.5)).expect("-0.5"));
        assert!(!CallBuilder::scalar_bool(&ArgValue::Float(0.0)).expect("0.0"));
        assert!(!CallBuilder::scalar_bool(&ArgValue::Int(0)).expect("0"));
        assert!(CallBuilder::scalar_bool(&ArgValue::Int(-1)).expect("-1"));
        let context = context_for("typedef double bool_source;");
        let ty = crate::context::resolve_type(context, "bool_source").expect("double type");
        let handle = crate::context::new_cdata(context, ty, true).expect("cdata");
        let data = crate::context::cdata(handle).expect("data");
        unsafe { std::ptr::write_unaligned(data.addr as *mut f64, 0.5); }
        assert!(CallBuilder::scalar_bool(&ArgValue::CData(handle)).expect("float cdata"));
        crate::context::release_cdata(handle);
    }

    /// Verifies a CData argument must be a compatible function pointer for a callback parameter.
    #[test]
    fn function_pointer_parameter_requires_matching_signature() {
        let context = context_for(
            "typedef int (*expected_cb)(int);              struct record { int (*fn)(int); };",
        );
        let expected = crate::context::resolve_type(context, "expected_cb").expect("expected type");
        let expected_info = type_clone(expected).expect("expected info");
        let compatible = crate::context::resolve_type(context, "int (*)(int)").expect("compatible type");
        let compatible_value = crate::context::new_cdata(context, compatible, true).expect("compatible value");
        assert!(argument_is_compatible(&expected_info, &ArgValue::CData(compatible_value)));
        let incompatible = crate::context::resolve_type(context, "double (*)(double)").expect("incompatible type");
        let incompatible_value = crate::context::new_cdata(context, incompatible, true).expect("incompatible value");
        assert!(!argument_is_compatible(&expected_info, &ArgValue::CData(incompatible_value)));
        let scalar = crate::context::resolve_type(context, "int").expect("scalar type");
        let scalar_value = crate::context::new_cdata(context, scalar, true).expect("scalar value");
        assert!(!argument_is_compatible(&expected_info, &ArgValue::CData(scalar_value)));
        assert!(argument_is_compatible(&expected_info, &ArgValue::Null));
        assert!(argument_is_compatible(&expected_info, &ArgValue::Pointer(0)));
        crate::context::release_cdata(compatible_value);
        crate::context::release_cdata(incompatible_value);
        crate::context::release_cdata(scalar_value);
    }

    /// Verifies allocated and cloned CData storage honors an over-aligned C type.
    #[test]
    fn cdata_allocation_honors_declared_alignment() {
        let context = context_for("struct __attribute__((aligned(64))) aligned_record { int a; };");
        let ty = crate::context::resolve_type(context, "struct aligned_record").expect("aligned type");
        let handle = crate::context::new_cdata(context, ty, true).expect("aligned value");
        let data = crate::context::cdata(handle).expect("data");
        assert_eq!(data.addr % 64, 0);
        let clone = crate::context::clone_cdata(handle).expect("clone");
        let cloned = crate::context::cdata(clone).expect("cloned data");
        assert_eq!(cloned.addr % 64, 0);
        crate::context::release_cdata(handle);
        crate::context::release_cdata(clone);
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

#[cfg(test)]
mod boundary_tests {
    //! Purpose:
    //! Regression tests for call argument classification and ABI compatibility gates.
    //!
    //! Called from:
    //! - `cargo test -p elephc-ffi`.
    //!
    //! Key details:
    //! - A declared parameter libffi cannot represent must fail instead of falling back to the
    //!   PHP argument's classifier; a struct argument must match the declared type.

    use super::*;
    use std::ffi::CString;

    /// Parses one cdef block and returns its context handle.
    fn context_for(code: &str) -> usize {
        let code = CString::new(code).expect("cdef");
        let context = crate::abi::elephc_ffi_cdef(code.as_ptr(), std::ptr::null());
        assert_ne!(context, 0, "cdef must parse");
        context
    }

    /// Packed records and bit-field records stay unrepresentable.
    #[test]
    fn unrepresentable_declared_types_do_not_fall_back_to_the_argument() {
        let context = context_for(
            "struct __attribute__((packed)) packed { char a; int b; };\
             struct bits { int a:3; int b:5; };",
        );
        let packed = crate::context::resolve_type(context, "struct packed").expect("packed type");
        let bits = crate::context::resolve_type(context, "struct bits").expect("bits type");
        assert!(declared_ffi_type(packed).is_none());
        assert!(declared_ffi_type(bits).is_none());
        assert!(ffi_type_for(Some(packed), &ArgValue::Int(7)).is_none());
        assert!(ffi_type_for(Some(bits), &ArgValue::Int(7)).is_none());
    }

    /// Verifies layouts libffi cannot reproduce are rejected instead of mis-passed.
    #[test]
    fn unrepresentable_layouts_are_rejected_before_a_call() {
        let context = context_for(
            "struct __attribute__((aligned(64))) over { int a; };             struct empty {};             union mixed_float { double d; float f; };             union same_float { double a; double b; };             struct normal { int a; };",
        );
        for name in ["struct over", "struct empty", "union mixed_float"] {
            let ty = crate::context::resolve_type(context, name).expect(name);
            assert!(declared_ffi_type(ty).is_none(), "{name} must be rejected");
        }
        for name in ["union same_float", "struct normal"] {
            let ty = crate::context::resolve_type(context, name).expect(name);
            assert!(declared_ffi_type(ty).is_some(), "{name} must stay representable");
        }
        let long_double = crate::context::resolve_type(context, "long double").expect("long double");
        assert!(
            declared_ffi_type(long_double).is_some(),
            "long double must map to libffi's target type"
        );
    }

    /// Verifies aggregate parameter matching uses C's case-sensitive type spelling.
    #[test]
    fn aggregate_argument_type_spelling_is_case_sensitive() {
        let context = context_for("struct lower { int a; }; struct LOWER { int a; };");
        let expected = crate::context::resolve_type(context, "struct lower").expect("lower");
        let expected_info = type_clone(expected).expect("lower info");
        let other = crate::context::resolve_type(context, "struct LOWER").expect("upper");
        let other_value = crate::context::new_cdata(context, other, true).expect("upper value");
        assert!(!argument_is_compatible(&expected_info, &ArgValue::CData(other_value)));
        crate::context::release_cdata(other_value);
    }

    /// Verifies pointer parameters reject incompatible CData and raw scalar arguments.
    #[test]
    fn pointer_parameters_require_compatible_cdata() {
        let context = context_for(
            "struct S { int a; }; struct T { int a; };",
        );
        let int_ptr = crate::context::resolve_type(context, "int *").expect("int pointer");
        let int_ptr_info = type_clone(int_ptr).expect("int pointer info");
        let int_ty = crate::context::resolve_type(context, "int").expect("int");
        let int_value = crate::context::new_cdata(context, int_ty, true).expect("int value");
        assert!(!argument_is_compatible(&int_ptr_info, &ArgValue::CData(int_value)));
        assert!(!argument_is_compatible(&int_ptr_info, &ArgValue::Bool(true)));
        let struct_s = crate::context::resolve_type(context, "struct S").expect("struct S");
        let struct_value = crate::context::new_cdata(context, struct_s, true).expect("struct value");
        assert!(!argument_is_compatible(&int_ptr_info, &ArgValue::CData(struct_value)));
        let char_ptr = crate::context::resolve_type(context, "char *").expect("char pointer");
        let char_value = crate::context::new_cdata(context, char_ptr, true).expect("char pointer value");
        assert!(!argument_is_compatible(&int_ptr_info, &ArgValue::CData(char_value)));
        let void_ptr = crate::context::resolve_type(context, "void *").expect("void pointer");
        let void_ptr_info = type_clone(void_ptr).expect("void pointer info");
        let array = crate::context::resolve_type(context, "int [1]").expect("int array");
        let array_value = crate::context::new_cdata(context, array, true).expect("array value");
        assert!(argument_is_compatible(&void_ptr_info, &ArgValue::CData(array_value)));
        let int_ptr_value = crate::context::new_cdata(context, int_ptr, true).expect("int pointer value");
        assert!(argument_is_compatible(&void_ptr_info, &ArgValue::CData(int_ptr_value)));
        let int_ptr_ptr = crate::context::resolve_type(context, "int **").expect("int pointer pointer");
        let int_ptr_ptr_info = type_clone(int_ptr_ptr).expect("int pointer pointer info");
        assert!(!argument_is_compatible(&int_ptr_ptr_info, &ArgValue::CData(int_ptr_value)));
        crate::context::release_cdata(int_value);
        crate::context::release_cdata(struct_value);
        crate::context::release_cdata(char_value);
        crate::context::release_cdata(array_value);
        crate::context::release_cdata(int_ptr_value);
    }

    /// Verifies a variadic pointer CData contributes its pointer value, not its storage cell.
    #[test]
    fn variadic_pointer_cdata_uses_the_pointer_value() {
        let context = context_for("int printf(const char *format, ...);");
        let char_ty = crate::context::resolve_type(context, "char").expect("char type");
        let handle = crate::context::pointer_to(0x1234, char_ty).expect("pointer cdata");
        let builder = CallBuilder::new(context, "printf").expect("printf builder");
        let mut strings = Vec::new();
        let storage = builder
            .marshal(None, &ArgValue::CData(handle), &mut strings)
            .expect("marshal pointer");
        match storage {
            Storage::Pointer(value) => assert_eq!(value, 0x1234),
            _ => panic!("expected pointer storage"),
        }
        crate::context::release_cdata(handle);
    }

    /// A CData aggregate argument must match the declared parameter's type and size.
    #[test]
    fn aggregate_arguments_require_a_matching_declared_type() {
        let context = context_for(
            "struct small { int a; }; struct big { int a; int b; };",
        );
        let small = crate::context::resolve_type(context, "struct small").expect("small type");
        let big = crate::context::resolve_type(context, "struct big").expect("big type");
        let big_value = crate::context::new_cdata(0, big, true).expect("big allocation");
        let small_info = type_clone(small).expect("small info");
        assert!(!argument_is_compatible(&small_info, &ArgValue::CData(big_value)));
        crate::context::release_cdata(big_value);
    }
}
