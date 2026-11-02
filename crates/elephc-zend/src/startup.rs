//! Purpose:
//! The engine's globals (`EG`, `CG`, `PG`, `SG`, `BG`, `FG`, `PS`) and its
//! one-time startup: interned strings, the function/class/constant/INI
//! tables, the core class hierarchy, and the modules whose surface the engine
//! itself provides.
//!
//! Called from:
//! - `crate::host` before the first call into an extension, and the unit tests.
//!
//! Key details:
//! - The state is that of a php-cli process inside its one request, with
//!   php-cli's defaults (`display_errors=1`, `serialize_precision=-1`...).
//! - Startup is idempotent and cheap to re-check.

use std::ffi::CStr;
use std::ptr;

use crate::abi::*;
use crate::alloc::alloc_struct;
use crate::hash;

#[no_mangle]
pub static mut executor_globals: zend_executor_globals = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static mut compiler_globals: zend_compiler_globals = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static mut module_registry: HashTable = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static mut core_globals: php_core_globals = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static mut sapi_globals: sapi_globals_struct = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static mut sapi_module: sapi_module_struct = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static mut basic_globals: php_basic_globals = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static mut file_globals: php_file_globals = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static mut ps_globals: php_ps_globals = unsafe { std::mem::zeroed() };

static mut STARTED: bool = false;
/// Whether the compiled program reports PHP's built-in server SAPI (`--web`) rather than the CLI.
static mut SERVER_SAPI: bool = false;

/// Sets the SAPI the engine reports to the program's own (`php_sapi_name()`); takes effect when
/// called before the engine starts, which the program's setup does.
pub fn set_server_sapi(server: bool) {
    unsafe { SERVER_SAPI = server }
}

/// The SAPI name the engine reports: `cli`, or `cli-server` for a `--web` program, as
/// `php_sapi_name()` answers in the program.
pub fn sapi_name() -> &'static CStr {
    if unsafe { SERVER_SAPI } { c"cli-server" } else { c"cli" }
}

/// `EG(...)`.
#[allow(clippy::mut_from_ref)]
pub fn eg() -> &'static mut zend_executor_globals {
    unsafe { &mut *ptr::addr_of_mut!(executor_globals) }
}

/// `CG(...)`.
pub fn cg() -> &'static mut zend_compiler_globals {
    unsafe { &mut *ptr::addr_of_mut!(compiler_globals) }
}

/// Returns the process-wide module registry hash table that extension modules register into.
pub fn modules() -> *mut HashTable {
    ptr::addr_of_mut!(module_registry)
}

/// Allocates and initialises a persistent hash table of the given size with the given element
/// destructor.
unsafe fn new_table(size: u32, destructor: dtor_func_t) -> *mut HashTable {
    let table = alloc_struct::<HashTable>();
    hash::init(table, size, destructor, true);
    table
}

/// Starts the engine once per process.
pub fn startup() {
    unsafe {
        if STARTED {
            return;
        }
        STARTED = true;
        crate::string::startup();

        let eg = eg();
        let cg = cg();
        cg.function_table = new_table(256, None);
        cg.class_table = new_table(64, None);
        cg.auto_globals = new_table(8, None);
        eg.function_table = cg.function_table;
        eg.class_table = cg.class_table;
        eg.zend_constants = new_table(128, None);
        eg.ini_directives = new_table(64, None);
        hash::init(ptr::addr_of_mut!(module_registry), 32, None, true);
        hash::init(&mut eg.symbol_table, 8, Some(crate::zval::zval_ptr_dtor), false);
        hash::init(&mut eg.regular_list, 8, None, false);
        hash::init(&mut eg.persistent_list, 8, Some(crate::api::persistent_entry_destructor), true);

        eg.uninitialized_zval.u1.type_info = IS_NULL as u32;
        eg.error_zval.u1.type_info = _IS_ERROR as u32;
        eg.error_reporting = E_ALL;
        eg.precision = 14;
        eg.lineno_override = -1;
        eg.error_handling = EH_NORMAL;
        eg.bailout = ptr::null_mut();
        eg.active = true;

        let pg = &mut *ptr::addr_of_mut!(core_globals);
        pg.serialize_precision = -1;
        pg.memory_limit = -1;
        pg.display_errors = true;
        pg.modules_activated = true;
        pg.implicit_flush = true;
        for global in pg.http_globals.iter_mut() {
            global.u1.type_info = IS_NULL as u32;
        }
        let sg = &mut *ptr::addr_of_mut!(sapi_globals);
        sg.request_info.no_headers = true;
        sg.global_request_time = 0.0;
        let module = &mut *ptr::addr_of_mut!(sapi_module);
        module.name = sapi_name().as_ptr().cast_mut();
        module.pretty_name = if SERVER_SAPI { c"Built-in HTTP server" } else { c"Command Line Interface" }
            .as_ptr()
            .cast_mut();
        let bg = &mut *ptr::addr_of_mut!(basic_globals);
        bg.unserialize_max_depth = 4096;
        let fg = &mut *ptr::addr_of_mut!(file_globals);
        fg.def_chunk_size = 8192;
        fg.default_socket_timeout = 60;
        let ps = &mut *ptr::addr_of_mut!(ps_globals);
        ps.http_session_vars.u1.type_info = IS_NULL as u32;

        crate::module::register_provided_modules();
        crate::module::with_core_module(|| crate::exception::register_core_classes());
        crate::constants::register_core_constants();
    }
}
