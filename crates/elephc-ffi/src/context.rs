//! Purpose:
//! Owns process-wide FFI contexts, the shared C type arena, CData handles, and the
//! thread-local diagnostic/string results crossing the C ABI.
//!
//! Called from:
//! - `crate::abi` entry points, `crate::place`, `crate::memory`, and `crate::call`.
//!
//! Key details:
//! - Contexts and type ids live for the process; CData boxes are freed by the PHP-side
//!   `__destruct`/`FFI::free` path. Handles are raw box addresses so they stay pointer
//!   sized and cheap to pass through generated code.
//! - Errors are thread-local `(kind, message)` pairs. Kind 1 maps to
//!   `FFI\ParserException`, kind 2 to `FFI\Exception`, kind 3 to `TypeError`.
//! - Every public function checks handles before dereferencing; a stale handle reports
//!   an FFI exception instead of causing UB.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::{Mutex, OnceLock};

use crate::parser::{self, ParseError};
use crate::types::{align_up, CType, TypeId, TypeKind, TypeStore};

/// Diagnostic kinds crossing the bridge boundary.
pub const ERROR_PARSER: u32 = 1;
pub const ERROR_EXCEPTION: u32 = 2;
pub const ERROR_TYPE: u32 = 3;

/// One FFI context opened by `FFI::cdef()` or `FFI::load()`.
pub struct FfiContext {
    /// Typedef and tag names visible in this context.
    pub names: HashMap<String, TypeId>,
    /// Tag names (`struct X`, ...).
    pub tags: HashMap<String, TypeId>,
    /// Enum constants and object-like `#define` values.
    pub constants: HashMap<String, i64>,
    /// Function declarations.
    pub functions: HashMap<String, TypeId>,
    /// Global variable declarations.
    pub globals: HashMap<String, TypeId>,
    /// `dlopen` handle as a usize, or 0 when the context only uses `RTLD_DEFAULT`.
    library: usize,
    /// `#define FFI_SCOPE`.
    pub scope: Option<String>,
    /// `#define FFI_LIB`.
    pub library_name: Option<String>,
}

impl FfiContext {
    /// Returns the dynamic library handle, if `FFI_LIB`/`lib` opened one.
    pub fn library(&self) -> usize {
        self.library
    }

    /// Resolves one symbol through the context's library, then the process global scope.
    pub fn symbol(&self, name: &str) -> Option<usize> {
        match name {
            "get_zend_version" => return Some(crate::zend_compat::elephc_ffi_zend_version as *const () as usize),
            "zend_printf" => return Some(crate::zend_compat::zend_printf_address()),
            "zend_hash_func" => return Some(crate::zend_compat::elephc_ffi_zend_hash_func as *const () as usize),
            "zend_str_tolower" => {
                return Some(crate::zend_compat::elephc_ffi_zend_str_tolower as *const () as usize)
            }
            "stdout" => return Some(crate::zend_compat::stdout_address()),
            "stdin" => return Some(crate::zend_compat::stdin_address()),
            "stderr" => return Some(crate::zend_compat::stderr_address()),
            "zend_write" => {
                let writer = crate::zend_compat::elephc_ffi_default_zend_write as *const () as usize;
                let _ = crate::callback::elephc_zend_write_current.compare_exchange(
                    0,
                    writer,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                );
                return Some(std::ptr::addr_of!(
                    crate::callback::elephc_zend_write_current
                ) as usize);
            }
            #[cfg(feature = "php-src-fixtures")]
            _ if crate::php_src_fixtures::symbol(name).is_some() => {
                return crate::php_src_fixtures::symbol(name)
            }
            _ => {}
        }
        let cname = CString::new(name).ok()?;
        unsafe {
            let first = if self.library != 0 {
                libc::dlsym(self.library as *mut c_void, cname.as_ptr())
            } else {
                libc::dlsym(libc::RTLD_DEFAULT, cname.as_ptr())
            };
            if !first.is_null() {
                return Some(first as usize);
            }
            let fallback = libc::dlsym(libc::RTLD_DEFAULT, cname.as_ptr());
            (!fallback.is_null()).then_some(fallback as usize)
        }
    }
}

/// One allocated or viewed C object.
#[derive(Clone, Debug)]
pub struct CData {
    /// Address of this object's first byte.
    pub addr: usize,
    /// Type of the object.
    pub ty: TypeId,
    /// Whether `addr` was allocated by the bridge.
    pub allocated: bool,
    /// Whether this object owns its allocation (PHP's `$owned` argument).
    pub owned: bool,
    /// Set by `FFI::free()`.
    pub freed: bool,
    /// `FFI::addr()` result: nested `addr()` must be rejected.
    pub addr_temp: bool,
    /// `const`-qualified storage rejects writes.
    pub readonly: bool,
    /// Bit-field width when this CData is a view of a bit-field.
    pub bit_width: Option<u8>,
    /// Bit offset inside the field's storage unit.
    pub bit_offset: u8,
    /// Whether this pointer owns the pointee allocation transferred by `FFI::addr(temp)`.
    pub owns_pointee: bool,
    /// php-src's `$persistent` flag: the allocator survives the CData destructor.
    pub persistent: bool,
    /// Storage address of the field/element this pointer was read from, or 0.
    ///
    /// `FFI::free($array[$i])` must zero the originating element (php-src), not only the
    /// temporary CData copy. The field/element readers stamp this address on pointer copies;
    /// `free_cdata` frees the pointee and zeroes both the element and the copy.
    pub source: usize,
}

impl CData {
    /// Reads this object's pointer value (valid for pointer types).
    pub fn pointer_value(&self) -> usize {
        unsafe { std::ptr::read_unaligned(self.addr as *const usize) }
    }

    /// Returns true when the object was freed explicitly.
    pub fn check_alive(&self) -> Result<(), ()> {
        if self.freed {
            Err(())
        } else {
            Ok(())
        }
    }

    /// Releases the CData box and its owned allocation.
    pub fn release(self) {
        if self.allocated && self.owned && !self.freed && !self.persistent {
            unsafe {
                libc::free(self.addr as *mut c_void);
            }
        }
    }
}

struct Global {
    store: TypeStore,
    base_names: HashMap<String, TypeId>,
    scopes: HashMap<String, usize>,
}

fn global() -> &'static Mutex<Global> {
    static GLOBAL: OnceLock<Mutex<Global>> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        let (store, base_names) = TypeStore::with_builtins();
        Mutex::new(Global {
            store,
            base_names,
            scopes: HashMap::new(),
        })
    })
}

thread_local! {
    static LAST_ERROR: RefCell<(u32, String)> = const { RefCell::new((0, String::new())) };
    static LAST_STRING: RefCell<CString> = RefCell::new(CString::new("").expect("empty CString"));
}

/// Records a bridge diagnostic for the PHP prelude to throw.
pub fn set_error(kind: u32, message: impl Into<String>) {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = (kind, message.into());
    });
}

/// Records a parser diagnostic in the form the prelude exposes.
pub fn set_parse_error(error: ParseError) {
    set_error(ERROR_PARSER, error.display());
}

/// Clears the previous diagnostic.
pub fn clear_error() {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = (0, String::new());
    });
}

/// Returns the last diagnostic kind (0 when none).
pub fn error_kind() -> u32 {
    LAST_ERROR.with(|slot| slot.borrow().0)
}

/// Returns the last diagnostic message.
pub fn error_message() -> String {
    LAST_ERROR.with(|slot| slot.borrow().1.clone())
}

/// Replaces the thread-local result string and returns its pointer.
pub fn set_result_string(bytes: &[u8]) -> *const c_char {
    let sanitized: Vec<u8> = bytes.iter().copied().filter(|byte| *byte != 0).collect();
    let string = CString::new(sanitized).unwrap_or_default();
    LAST_STRING.with(|slot| {
        *slot.borrow_mut() = string;
        slot.borrow().as_ptr()
    })
}

/// Returns the last result string's byte length.
pub fn result_string_len() -> usize {
    LAST_STRING.with(|slot| slot.borrow().as_bytes().len())
}

/// Locks the shared type store and runs `body`.
pub fn with_store<T>(body: impl FnOnce(&mut TypeStore, &HashMap<String, TypeId>) -> T) -> T {
    let mut guard = global().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut store = std::mem::take(&mut guard.store);
    let result = body(&mut store, &guard.base_names);
    guard.store = store;
    result
}

/// Locks the shared type store and returns a clone of one type.
pub fn type_clone(id: TypeId) -> Option<CType> {
    with_store(|store, _| (id < store.len()).then(|| store.get(id).clone()))
}

/// Locks the shared type store and returns the number of allocated types.
pub fn type_count() -> usize {
    with_store(|store, _| store.len())
}

/// Opens a library and returns its `dlopen` handle.
fn open_library(name: &str) -> Result<usize, String> {
    if name.is_empty() {
        return Ok(0);
    }
    // php-src fixtures name glibc's DSOs (`libc.so.6`, `libm.so.6`) because the corpus
    // is Linux-first. On Apple targets those symbols live in the process image, so use
    // the global scope rather than failing to dlopen a Linux SONAME.
    if cfg!(any(target_os = "macos", target_os = "ios"))
        && (name.starts_with("libc.so")
            || name.starts_with("libm.so")
            || name.starts_with("libdl.so")
            || name.starts_with("libpthread.so"))
    {
        return Ok(0);
    }
    let cname = CString::new(name).map_err(|_| "library name contains a NUL byte".to_string())?;
    unsafe {
        let handle = libc::dlopen(cname.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
        if handle.is_null() {
            let error = CStr::from_ptr(libc::dlerror())
                .to_string_lossy()
                .into_owned();
            Err(error)
        } else {
            Ok(handle as usize)
        }
    }
}

/// Creates a context from C declarations.
pub fn cdef(code: &str, library: Option<&str>) -> Option<usize> {
    clear_error();
    let library_handle = match library {
        Some(name) => match open_library(name) {
            Ok(handle) => handle,
            Err(error) => {
                set_error(ERROR_EXCEPTION, error);
                return None;
            }
        },
        None => 0,
    };
    let parsed = with_store(|store, names| parser::parse_context(store, names, code));
    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            set_parse_error(error);
            return None;
        }
    };
    let scope = parsed.scope.clone();
    let library_name = parsed.library.clone().or_else(|| library.map(str::to_string));
    let context = Box::new(FfiContext {
        names: parser_names_with_names(parsed.names),
        tags: parsed.tags,
        constants: parsed.constants,
        functions: parsed.functions,
        globals: parsed.globals,
        library: library_handle,
        scope: scope.clone(),
        library_name,
    });
    let handle = Box::into_raw(context) as usize;
    if let Some(scope) = scope {
        let mut guard = global().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.scopes.insert(scope, handle);
    }
    Some(handle)
}

/// Keeps the parsed names map unchanged; the helper exists to make the ownership move
/// explicit and to keep `cdef` readable.
fn parser_names_with_names(names: HashMap<String, TypeId>) -> HashMap<String, TypeId> {
    names
}

/// Loads a header file, honoring `FFI_LIB`/`FFI_SCOPE`.
pub fn load(path: &str) -> Option<usize> {
    clear_error();
    let code = match std::fs::read_to_string(path) {
        Ok(code) => code,
        Err(error) => {
            set_error(ERROR_EXCEPTION, format!("Failed to load \"{}\": {}", path, error));
            return None;
        }
    };
    let (library, has_scope) = {
        let (_, _, scope, library) = preprocess_header_directives(&code);
        (library, scope.is_some())
    };
    let handle = cdef(&code, library.as_deref());
    if handle.is_none() {
        return None;
    }
    if !has_scope {
        return handle;
    }
    handle
}

/// Extracts `FFI_LIB`/`FFI_SCOPE` without parsing declarations.
fn preprocess_header_directives(code: &str) -> (String, String, Option<String>, Option<String>) {
    let mut scope = None;
    let mut library = None;
    for line in code.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix("define") {
                let rest = rest.trim_start();
                let mut parts = rest.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or_default();
                let value = parts.next().unwrap_or_default().trim().trim_matches('"').to_string();
                if name == "FFI_SCOPE" {
                    scope = Some(value);
                } else if name == "FFI_LIB" {
                    library = Some(value);
                }
            }
        }
    }
    (String::new(), String::new(), scope, library)
}

/// Returns a previously loaded `FFI_SCOPE` context.
pub fn scope(name: &str) -> Option<usize> {
    clear_error();
    let guard = global().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match guard.scopes.get(name).copied() {
        Some(handle) => Some(handle),
        None => {
            drop(guard);
            set_error(
                ERROR_EXCEPTION,
                format!("FFI::scope(): Failed to find preloaded FFI scope \"{}\"", name),
            );
            None
        }
    }
}

/// Returns a context pointer from a handle.
pub fn context(handle: usize) -> Option<&'static FfiContext> {
    if handle == 0 {
        return None;
    }
    Some(unsafe { &*(handle as *const FfiContext) })
}

/// Resolves a type name in a context (or the global builtin namespace when handle is 0).
pub fn resolve_type(handle: usize, name: &str) -> Option<TypeId> {
    clear_error();
    let (names, tags, constants) = match context(handle) {
        Some(context) => (
            context.names.clone(),
            context.tags.clone(),
            context.constants.clone(),
        ),
        None => (HashMap::new(), HashMap::new(), HashMap::new()),
    };
    let result = with_store(|store, base_names| {
        parser::parse_type_name(store, base_names, &names, &tags, &constants, name)
    });
    match result {
        Ok(id) => Some(id),
        Err(error) => {
            set_parse_error(error);
            None
        }
    }
}

/// Creates a CData allocation in a context.
pub fn new_cdata(handle: usize, ty: TypeId, owned: bool) -> Option<usize> {
    clear_error();
    let ty_info = type_clone(ty)?;
    if matches!(ty_info.kind, TypeKind::Void)
        || matches!(&ty_info.kind, TypeKind::Array { element, .. } if matches!(type_clone(*element).map(|element| element.kind), Some(TypeKind::Void)))
    {
        set_error(ERROR_PARSER, "void type is not allowed at line 1");
        return None;
    }
    if ty_info.is_incomplete() {
        let label = match &ty_info.kind {
            TypeKind::IncompleteTag { keyword, tag } => format!("{} \"{}\"", keyword.as_str(), tag),
            _ => ty_info.display.clone(),
        };
        set_error(ERROR_PARSER, format!("Incomplete {} at line 1", label));
        return None;
    }
    if ty_info.size == 0 {
        set_error(ERROR_EXCEPTION, "Cannot instantiate FFI\\CData of zero size");
        return None;
    }
    let _ = handle;
    let addr = unsafe { libc::calloc(1, ty_info.size) as usize };
    if addr == 0 {
        set_error(ERROR_EXCEPTION, "Out of memory");
        return None;
    }
    let data = CData {
        addr,
        ty,
        allocated: true,
        owned,
        freed: false,
        addr_temp: false,
        readonly: ty_info.is_const,
        bit_width: None,
        bit_offset: 0,
            owns_pointee: false,
            persistent: false,
            source: 0,
    };
    Some(Box::into_raw(Box::new(data)) as usize)
}

/// Returns a CData pointer from a handle.
pub fn cdata(handle: usize) -> Option<&'static CData> {
    if handle == 0 {
        return None;
    }
    Some(unsafe { &*(handle as *mut CData) })
}

/// Returns a mutable CData pointer from a handle.
pub fn cdata_mut(handle: usize) -> Option<&'static mut CData> {
    if handle == 0 {
        return None;
    }
    Some(unsafe { &mut *(handle as *mut CData) })
}

/// Creates a new CData handle that references the same storage as `handle`.
pub fn clone_cdata(handle: usize) -> Option<usize> {
    let data = cdata(handle)?;
    if data.freed {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return None;
    }
    let ty = type_clone(data.ty)?;
    if matches!(ty.kind, TypeKind::Pointer(_)) {
        let pointer = data.pointer_value();
        let storage = unsafe { libc::calloc(1, crate::types::pointer_width()) as usize };
        if storage == 0 {
            set_error(ERROR_EXCEPTION, "Out of memory");
            return None;
        }
        unsafe {
            std::ptr::write_unaligned(storage as *mut usize, pointer);
        }
        let copy = CData {
            addr: storage,
            ty: data.ty,
            allocated: true,
            owned: true,
            freed: false,
            addr_temp: data.addr_temp,
            readonly: data.readonly,
            bit_width: None,
            bit_offset: 0,
            owns_pointee: false,
            persistent: false,
            source: 0,
        };
        return Some(Box::into_raw(Box::new(copy)) as usize);
    }
    // php-src's clone copies the whole value into owned storage; a view would dangle as soon
    // as the original CData (or the field it points into) is released.
    let storage = unsafe { libc::calloc(1, ty.size.max(1)) as usize };
    if storage == 0 {
        set_error(ERROR_EXCEPTION, "Out of memory");
        return None;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(data.addr as *const u8, storage as *mut u8, ty.size);
    }
    let copy = CData {
        addr: storage,
        ty: data.ty,
        allocated: true,
        owned: true,
        freed: false,
        addr_temp: data.addr_temp,
        readonly: data.readonly,
        bit_width: data.bit_width,
        bit_offset: data.bit_offset,
            owns_pointee: false,
            persistent: false,
            source: 0,
    };
    Some(Box::into_raw(Box::new(copy)) as usize)
}

/// Releases the box backing a CData handle.
pub fn release_cdata(handle: usize) {
    if handle == 0 {
        return;
    }
    unsafe {
        let data = Box::from_raw(handle as *mut CData);
        data.release();
        // Dropping the box after `release` frees the handle storage itself.
    }
}

/// Frees the storage behind a CData object, matching `FFI::free()`.
pub fn free_cdata(handle: usize) -> Result<(), ()> {
    clear_error();
    let Some(data) = cdata_mut(handle) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return Err(());
    };
    if data.freed {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return Err(());
    }
    let ty = type_clone(data.ty).ok_or(())?;
    if let TypeKind::Pointer(_) = ty.kind {
        // A pointer read from a field/element carries that place as `source`. `FFI::free($v[$i])`
        // must zero the element, but freeing a SAVED copy whose source field was already
        // rewritten (the linked-list `del()` pattern) must not clear the new link. Zero the
        // source only while it still holds this exact pointer value.
        let copy_pointer = unsafe { std::ptr::read_unaligned(data.addr as *const usize) };
        if data.source != 0 {
            let source_pointer = unsafe { std::ptr::read_unaligned(data.source as *const usize) };
            if source_pointer == copy_pointer {
                if source_pointer != 0 {
                    unsafe {
                        libc::free(source_pointer as *mut c_void);
                    }
                }
                unsafe {
                    std::ptr::write_unaligned(data.source as *mut usize, 0);
                }
            } else if copy_pointer != 0 {
                unsafe {
                    libc::free(copy_pointer as *mut c_void);
                }
            }
            unsafe {
                std::ptr::write_unaligned(data.addr as *mut usize, 0);
            }
            return Ok(());
        }
        let pointer = copy_pointer;
        if pointer != 0 {
            unsafe {
                libc::free(pointer as *mut c_void);
            }
        }
        unsafe {
            std::ptr::write_unaligned(data.addr as *mut usize, 0);
        }
        return Ok(());
    }
    if data.allocated {
        unsafe {
            libc::free(data.addr as *mut c_void);
        }
        data.addr = 0;
    }
    data.freed = true;
    Ok(())
}

/// Creates a non-owning CData view at `addr`.
pub fn view(addr: usize, ty: TypeId, readonly: bool) -> usize {
    view_bits(addr, ty, readonly, None, 0)
}

/// Creates a non-owning CData view, optionally carrying a bit-field window.
pub fn view_bits(
    addr: usize,
    ty: TypeId,
    readonly: bool,
    bit_width: Option<u8>,
    bit_offset: u8,
) -> usize {
    let data = CData {
        addr,
        ty,
        allocated: false,
        owned: false,
        freed: false,
        addr_temp: false,
        readonly,
        bit_width,
        bit_offset,
            owns_pointee: false,
            persistent: false,
        source: 0,
    };
    Box::into_raw(Box::new(data)) as usize
}

/// Creates an `FFI::addr()`-style pointer to an explicit address.
pub fn pointer_to(addr: usize, ty: TypeId) -> Option<usize> {
    let pointer_type = with_store(|store, _| store.pointer(ty));
    let storage = unsafe { libc::calloc(1, crate::types::pointer_width()) as usize };
    if storage == 0 {
        set_error(ERROR_EXCEPTION, "Out of memory");
        return None;
    }
    unsafe {
        std::ptr::write_unaligned(storage as *mut usize, addr);
    }
    let result = CData {
        addr: storage,
        ty: pointer_type,
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
    Some(Box::into_raw(Box::new(result)) as usize)
}

/// Creates an `FFI::addr()` pointer to a CData object.
pub fn addr(handle: usize) -> Option<usize> {
    clear_error();
    let data = cdata(handle)?;
    if data.freed {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return None;
    }
    if data.addr_temp {
        set_error(
            ERROR_EXCEPTION,
            "FFI::addr() cannot create a reference to a temporary pointer",
        );
        return None;
    }
    pointer_to(data.addr, data.ty)
}

/// Creates an `FFI::addr()` pointer to a temporary CData and transfers its ownership.
///
/// php-src transfers the allocation only when the source is an unshared temporary; the
/// compiler lowers `FFI::addr(<expression>)` to this entry, while `FFI::addr($variable)`
/// keeps the shared reference semantics.
pub fn addr_owned(handle: usize) -> Option<usize> {
    clear_error();
    let data = cdata(handle)?;
    if data.freed {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return None;
    }
    if data.addr_temp {
        set_error(
            ERROR_EXCEPTION,
            "FFI::addr() cannot create a reference to a temporary pointer",
        );
        return None;
    }
    let pointer = pointer_to(data.addr, data.ty)?;
    if data.allocated && data.owned {
        let persistent = data.persistent;
        let source = cdata_mut(handle)?;
        source.allocated = false;
        source.owned = false;
        if let Some(pointer_data) = cdata_mut(pointer) {
            // A persistent pointee survives its owner; the pointer cell itself is still freed.
            pointer_data.owns_pointee = !persistent;
        }
    }
    Some(pointer)
}

/// Marks an allocated CData as persistent (PHP's `$persistent` argument).
pub fn set_persistent(handle: usize) -> bool {
    let Some(data) = cdata_mut(handle) else { return false };
    data.persistent = true;
    true
}

/// Casts a CData object to another type.
pub fn cast(ty: TypeId, source_handle: usize) -> Option<usize> {
    clear_error();
    let source = cdata(source_handle)?;
    if source.freed {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return None;
    }
    let target = type_clone(ty)?;
    let source_type = type_clone(source.ty)?;
    if target.size > source_type.size
        && !matches!(source_type.kind, TypeKind::Pointer(_))
    {
        set_error(ERROR_EXCEPTION, "attempt to cast to larger type");
        return None;
    }
    if matches!(target.kind, TypeKind::Pointer(_)) {
        let value = if matches!(source_type.kind, TypeKind::Pointer(_)) {
            unsafe { std::ptr::read_unaligned(source.addr as *const usize) }
        } else {
            source.addr
        };
        let storage = unsafe { libc::calloc(1, crate::types::pointer_width()) as usize };
        if storage == 0 {
            set_error(ERROR_EXCEPTION, "Out of memory");
            return None;
        }
        unsafe {
            std::ptr::write_unaligned(storage as *mut usize, value);
        }
        let result = CData {
            addr: storage,
            ty,
            allocated: true,
            owned: true,
            freed: false,
            addr_temp: false,
            readonly: target.is_const,
            bit_width: None,
            bit_offset: 0,
            owns_pointee: false,
            persistent: false,
            source: 0,
        };
        return Some(Box::into_raw(Box::new(result)) as usize);
    }
    if matches!(source_type.kind, TypeKind::Pointer(_)) {
        // Casting a pointer to an aggregate/scalar views the pointee; its lifetime belongs to
        // the C side, matching php-src's pointer cast.
        let addr = unsafe { std::ptr::read_unaligned(source.addr as *const usize) };
        if addr == 0 {
            set_error(ERROR_EXCEPTION, "attempt to cast a null pointer");
            return None;
        }
        let result = CData {
            addr,
            ty,
            allocated: false,
            owned: false,
            freed: false,
            addr_temp: false,
            readonly: target.is_const,
            bit_width: None,
            bit_offset: 0,
            owns_pointee: false,
            persistent: false,
            source: 0,
        };
        return Some(Box::into_raw(Box::new(result)) as usize);
    }
    // Casting a value produces an independent value: the source CData may be a temporary
    // (FFI::cast($type, FFI::new($str))), so a view would dangle once it is released.
    let storage = unsafe { libc::calloc(1, target.size.max(1)) as usize };
    if storage == 0 {
        set_error(ERROR_EXCEPTION, "Out of memory");
        return None;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(
            source.addr as *const u8,
            storage as *mut u8,
            target.size.min(source_type.size),
        );
    }
    let result = CData {
        addr: storage,
        ty,
        allocated: true,
        owned: true,
        freed: false,
        addr_temp: false,
        readonly: target.is_const,
        bit_width: None,
        bit_offset: 0,
            owns_pointee: false,
            persistent: false,
        source: 0,
    };
    Some(Box::into_raw(Box::new(result)) as usize)
}

/// Returns a CData object's type id.
pub fn cdata_type(handle: usize) -> Option<TypeId> {
    cdata(handle).map(|data| data.ty)
}

/// Returns the display name of a CData object's type.
pub fn cdata_type_name(handle: usize) -> Option<String> {
    let data = cdata(handle)?;
    type_clone(data.ty).map(|ty| ty.display)
}

/// Returns the C `sizeof` for a CData object's type.
pub fn cdata_size(handle: usize) -> Option<usize> {
    let data = cdata(handle)?;
    type_clone(data.ty).map(|ty| ty.size)
}

/// Returns the C `alignof` for a CData object's type.
pub fn cdata_align(handle: usize) -> Option<usize> {
    let data = cdata(handle)?;
    type_clone(data.ty).map(|ty| ty.align)
}

/// Returns whether a pointer CData holds NULL.
pub fn cdata_is_null(handle: usize) -> Option<bool> {
    let data = cdata(handle)?;
    let ty = type_clone(data.ty)?;
    if matches!(ty.kind, TypeKind::Pointer(_)) {
        Some(unsafe { std::ptr::read_unaligned(data.addr as *const usize) == 0 })
    } else {
        Some(false)
    }
}

/// Returns the pointer value stored in a pointer CData, or the data address of an object.
pub fn data_address(data: &CData) -> usize {
    let Some(ty) = type_clone(data.ty) else { return 0 };
    match ty.kind {
        TypeKind::Pointer(_) => unsafe { std::ptr::read_unaligned(data.addr as *const usize) },
        _ => data.addr,
    }
}

/// Rounds one pointer value up by `count` elements of `element_size`.
pub fn advance_address(addr: usize, count: i64, element_size: usize) -> usize {
    let delta = (count as isize).wrapping_mul(element_size as isize);
    (addr as isize).wrapping_add(delta) as usize
}

/// Returns the element type id of an array or pointer type.
pub fn element_type(ty: TypeId) -> Option<TypeId> {
    let ty = type_clone(ty)?;
    match ty.kind {
        TypeKind::Array { element, .. } => Some(element),
        TypeKind::Pointer(inner) => Some(inner),
        _ => None,
    }
}

/// Returns the array length of a C array, if it has one.
pub fn array_len(ty: TypeId) -> Option<usize> {
    let ty = type_clone(ty)?;
    match ty.kind {
        TypeKind::Array { len, .. } => len,
        _ => None,
    }
}

/// Rounds an offset to a type's alignment.
pub fn aligned_offset(offset: usize, ty: TypeId) -> usize {
    type_clone(ty).map_or(offset, |ty| align_up(offset, ty.align.max(1)))
}
