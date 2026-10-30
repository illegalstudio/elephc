//! Purpose:
//! Checks the C side of the ABI against the Rust side: the generated header
//! compiles as C and as C++, its structures have exactly the size and field
//! offsets Rust gives them, and the macros expand cleanly.
//!
//! Called from:
//! - `cargo test -p elephc-zend`.
//!
//! Key details:
//! - Uses the system C compiler (`CC`, else `cc`), as an extension build does.
//! - The macro check compiles with `-Werror=uninitialized`: a macro
//!   that passes one of its own locals into another macro declaring the same
//!   name initializes that local from itself, and the compiler is entitled to
//!   drop the statement (`ZVAL_COPY` once returned NULL that way).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use elephc_zend::abi::*;

/// Creates a fresh per-process scratch directory named after the test and writes the crate's
/// embedded PHP include tree under it; returns the directory.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("elephc_zend_abi_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (relative, contents) in elephc_zend::include_tree() {
        let path = dir.join("include").join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    dir
}

/// The C or C++ compiler to use: `CXX` or `CC` from the environment, otherwise `c++` or `cc`.
fn compiler(cxx: bool) -> String {
    let var = if cxx { "CXX" } else { "CC" };
    std::env::var(var).unwrap_or_else(|_| if cxx { "c++".into() } else { "cc".into() })
}

/// The `-I` arguments exposing the scratch include tree (root, main, Zend, TSRM, ext) to the
/// compiler.
fn include_args(dir: &Path) -> Vec<String> {
    ["", "main", "Zend", "TSRM", "ext"]
        .iter()
        .flat_map(|sub| ["-I".to_string(), dir.join("include").join(sub).display().to_string()])
        .collect()
}

/// `(C expression, Rust value)` pairs: sizes and offsets that must agree.
fn layout_facts() -> Vec<(String, usize)> {
    use std::mem::{offset_of, size_of};
    let mut facts = elephc_zend::abi::values::value_layout_facts();
    macro_rules! size {
        ($($ty:ident),*) => { $( facts.push((format!("sizeof({})", stringify!($ty)), size_of::<$ty>())); )* };
    }
    macro_rules! offset {
        ($ty:ident: $($field:ident),*) => {
            $( facts.push((format!("offsetof({}, {})", stringify!($ty), stringify!($field)), offset_of!($ty, $field))); )*
        };
    }
    size!(
        zend_object, zend_resource, zend_reference, zend_execute_data,
        zend_internal_arg_info, zend_function_entry, zend_internal_function, zend_function, zend_module_entry,
        zend_class_entry, zend_object_handlers, zend_property_info, zend_class_constant, zend_constant,
        zend_ini_entry_def, zend_ini_entry, zend_executor_globals, zend_compiler_globals, smart_str, smart_string,
        zend_fcall_info, zend_fcall_info_cache, php_core_globals, php_basic_globals, PHP_MD5_CTX, PHP_SHA1_CTX,
        PHP_SHA256_CTX, php_hash_ops, zend_attribute, zend_object_iterator
    );
    offset!(zend_object: gc, handle, ce, handlers, properties, properties_table);
    offset!(zend_execute_data: opline, call, return_value, func, This, prev_execute_data);
    offset!(zend_internal_function: r#type, fn_flags, function_name, scope, num_args, required_num_args, arg_info, handler, module);
    offset!(zend_module_entry: size, zend_api, ini_entry, deps, name, functions, module_startup_func, request_startup_func, info_func, version, globals_size, globals_ptr, globals_ctor, module_started, module_number, build_id);
    offset!(zend_class_entry: r#type, name, parent_u, ce_flags, default_properties_count, function_table, properties_info, constants_table, constructor, default_object_handlers, create_u, get_iterator, serialize, num_interfaces, interfaces_u, info);
    offset!(zend_object_handlers: offset, free_obj, dtor_obj, clone_obj, read_property, write_property, get_properties, get_method, cast_object, compare, get_properties_for);
    offset!(zend_executor_globals: uninitialized_zval, function_table, class_table, zend_constants, current_execute_data, exception, error_reporting, ini_directives, bailout);
    offset!(php_core_globals: serialize_precision, unserialize_callback_func, http_globals);
    offset!(zend_ini_entry_def: name, on_modify, mh_arg1, mh_arg2, value, displayer, value_length, name_length, modifiable);
    facts
}

/// Rust field names that C spells differently (anonymous unions, keywords).
fn c_field(expr: &str) -> String {
    expr.replace("r#type", "type")
        .replace("zend_array, data", "zend_array, arData")
        .replace("parent_u", "parent")
        .replace("create_u", "create_object")
        .replace("interfaces_u", "interfaces")
}

/// Verifies the Rust structures have the sizes and field offsets the C headers give them, by
/// compiling and running a C program that prints each `sizeof` and `offsetof` and comparing against
/// the Rust values.
#[test]
fn structures_have_the_layout_rust_gives_them() {
    let dir = scratch("layout");
    let facts = layout_facts();
    let mut source = String::from("#include \"php.h\"\n#include <stdio.h>\nint main(void) {\n");
    for (expr, _) in &facts {
        let _ = writeln!(source, "    printf(\"%zu\\n\", (size_t){});", c_field(expr));
    }
    source.push_str("    return 0;\n}\n");
    fs::write(dir.join("layout.c"), source).unwrap();
    let binary = dir.join("layout");
    let compile = Command::new(compiler(false))
        .args(include_args(&dir))
        .arg(dir.join("layout.c"))
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("run the C compiler");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = Command::new(&binary).output().unwrap();
    let printed: Vec<usize> = String::from_utf8_lossy(&run.stdout).lines().map(|l| l.parse().unwrap()).collect();
    assert_eq!(printed.len(), facts.len());
    let mismatches: Vec<String> = facts
        .iter()
        .zip(&printed)
        .filter(|((_, rust), c)| rust != *c)
        .map(|((expr, rust), c)| format!("{expr}: Rust {rust}, C {c}"))
        .collect();
    assert!(mismatches.is_empty(), "layout mismatches:\n{}", mismatches.join("\n"));
    fs::remove_dir_all(dir).unwrap();
}

/// A translation unit expanding the macros extensions use most, the way
/// they use them.
const MACRO_SMOKE: &str = r#"
#include "php.h"
#include "php_ini.h"
#include "zend_exceptions.h"
#include "zend_interfaces.h"
#include "zend_smart_str.h"
#include "ext/standard/info.h"
#include "ext/standard/php_var.h"
#include "ext/spl/spl_exceptions.h"

ZEND_BEGIN_MODULE_GLOBALS(smoke)
    zend_long limit;
    char *name;
    bool flag;
ZEND_END_MODULE_GLOBALS(smoke)
ZEND_DECLARE_MODULE_GLOBALS(smoke)
#define SMOKE_G(v) ZEND_MODULE_GLOBALS_ACCESSOR(smoke, v)

PHP_INI_BEGIN()
    STD_PHP_INI_ENTRY("smoke.limit", "10", PHP_INI_ALL, OnUpdateLong, limit, zend_smoke_globals, smoke_globals)
    STD_PHP_INI_ENTRY("smoke.name", "x", PHP_INI_ALL, OnUpdateString, name, zend_smoke_globals, smoke_globals)
    STD_PHP_INI_BOOLEAN("smoke.flag", "1", PHP_INI_ALL, OnUpdateBool, flag, zend_smoke_globals, smoke_globals)
PHP_INI_END()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_smoke_all, 0, 1, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, s, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, l, IS_LONG, 0, "0")
    ZEND_ARG_TYPE_MASK(0, m, MAY_BE_STRING|MAY_BE_NULL, "null")
    ZEND_ARG_OBJ_INFO(0, o, stdClass, 1)
    ZEND_ARG_INFO(1, ref)
    ZEND_ARG_VARIADIC_TYPE_INFO(0, rest, IS_MIXED, 0)
ZEND_END_ARG_INFO()

static zend_class_entry *smoke_ce;

PHP_FUNCTION(smoke_all)
{
    zend_string *s;
    zend_long l = 0;
    zend_string *m = NULL;
    zval *o = NULL, *ref = NULL, *rest = NULL;
    uint32_t rest_count = 0;
    double d = 0;
    bool b = 0, b_null = 0;
    HashTable *ht = NULL;
    ZEND_PARSE_PARAMETERS_START(1, -1)
        Z_PARAM_STR(s)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(l)
        Z_PARAM_STR_OR_NULL(m)
        Z_PARAM_OBJECT_OR_NULL(o)
        Z_PARAM_ZVAL(ref)
        Z_PARAM_VARIADIC('*', rest, rest_count)
    ZEND_PARSE_PARAMETERS_END();
    (void)d; (void)b; (void)b_null; (void)ht; (void)o;

    array_init(return_value);
    add_assoc_str(return_value, "s", zend_string_copy(s));
    add_assoc_long(return_value, "l", l);
    add_next_index_stringl(return_value, ZSTR_VAL(s), ZSTR_LEN(s));
    zval copy;
    ZVAL_COPY(&copy, return_value);
    zval_ptr_dtor(&copy);
    ZVAL_COPY_VALUE(&copy, return_value);
    zval_copy_ctor(&copy);
    zval_ptr_dtor(&copy);
    ZVAL_COPY_VALUE(&copy, return_value);
    zval_opt_copy_ctor(&copy);
    zval_ptr_dtor(&copy);
    char secret[8] = "secret";
    ZEND_SECURE_ZERO(secret, sizeof(secret));
    zend_string *key;
    zend_ulong h;
    zval *entry;
    ZEND_HASH_FOREACH_KEY_VAL(Z_ARRVAL_P(return_value), h, key, entry) {
        (void)h; (void)key; (void)entry;
    } ZEND_HASH_FOREACH_END();
    ZEND_HASH_REVERSE_FOREACH_VAL(Z_ARRVAL_P(return_value), entry) {
        (void)entry;
    } ZEND_HASH_FOREACH_END();
    smart_str buf = {0};
    smart_str_appends(&buf, "x");
    smart_str_appendc(&buf, 'y');
    smart_str_append_long(&buf, l);
    smart_str_0(&buf);
    add_assoc_str(return_value, "buf", smart_str_extract(&buf));
    if (ref) {
        ZEND_TRY_ASSIGN_REF_LONG(ref, 42);
    }
    if (m && ZSTR_LEN(m) == 0) {
        zend_argument_value_error(3, "cannot be empty");
        RETURN_THROWS();
    }
    for (uint32_t i = 0; i < rest_count; i++) {
        Z_TRY_ADDREF(rest[i]);
        add_next_index_zval(return_value, &rest[i]);
    }
    if (l < 0) {
        zend_throw_exception_ex(spl_ce_InvalidArgumentException, l, "negative: " ZEND_LONG_FMT, l);
        RETURN_THROWS();
    }
}

PHP_FUNCTION(smoke_classic)
{
    char *str;
    size_t len;
    zend_long n = 1;
    zval *arr = NULL;
    if (zend_parse_parameters(ZEND_NUM_ARGS(), "s|la!", &str, &len, &n, &arr) == FAILURE) {
        RETURN_THROWS();
    }
    if (len == 0) {
        php_error_docref(NULL, E_WARNING, "empty input of %zu bytes", len);
        RETURN_FALSE;
    }
    RETURN_STRINGL(str, len);
}

static const zend_function_entry smoke_functions[] = {
    PHP_FE(smoke_all, arginfo_smoke_all)
    PHP_FE(smoke_classic, arginfo_smoke_all)
    PHP_FE_END
};

static PHP_GINIT_FUNCTION(smoke)
{
    smoke_globals->limit = 0;
}

PHP_MINIT_FUNCTION(smoke)
{
    REGISTER_INI_ENTRIES();
    REGISTER_LONG_CONSTANT("SMOKE_ONE", 1, CONST_CS | CONST_PERSISTENT);
    REGISTER_STRING_CONSTANT("SMOKE_NAME", "smoke", CONST_CS | CONST_PERSISTENT);
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "SmokeException", NULL);
    smoke_ce = zend_register_internal_class_ex(&ce, spl_ce_RuntimeException);
    zend_declare_class_constant_long(smoke_ce, "CODE", sizeof("CODE") - 1, 7);
    return SUCCESS;
}

PHP_MINFO_FUNCTION(smoke)
{
    php_info_print_table_start();
    php_info_print_table_row(2, "smoke", "enabled");
    php_info_print_table_end();
    DISPLAY_INI_ENTRIES();
}

zend_module_entry smoke_module_entry = {
    STANDARD_MODULE_HEADER,
    "smoke",
    smoke_functions,
    PHP_MINIT(smoke),
    NULL,
    NULL,
    NULL,
    PHP_MINFO(smoke),
    "1.0",
    PHP_MODULE_GLOBALS(smoke),
    PHP_GINIT(smoke),
    NULL,
    NULL,
    STANDARD_MODULE_PROPERTIES_EX
};
"#;

/// Compiles a translation unit of commonly used extension macros against the headers as C or C++
/// with strict warnings; fails the test with the compiler's output when it does not compile.
fn compile_smoke(cxx: bool) {
    let dir = scratch(if cxx { "smoke_cxx" } else { "smoke_c" });
    let source = dir.join(if cxx { "smoke.cpp" } else { "smoke.c" });
    let text = if cxx { format!("extern \"C\" {{\n{MACRO_SMOKE}\n}}\n") } else { MACRO_SMOKE.to_string() };
    fs::write(&source, text).unwrap();
    let mut command = Command::new(compiler(cxx));
    command
        .args(include_args(&dir))
        .args(["-c", "-Wall", "-Werror=uninitialized", "-Werror=implicit-function-declaration"])
        .args(if cxx { ["-std=c++17"] } else { ["-std=gnu17"] })
        .arg(&source)
        .arg("-o")
        .arg(dir.join("smoke.o"));
    if !cxx {
        command.arg("-Werror=int-conversion");
    }
    let output = command.output().expect("run the compiler");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    fs::remove_dir_all(dir).unwrap();
}

/// Verifies the common extension macros compile cleanly as C.
#[test]
fn macros_expand_cleanly_in_c() {
    compile_smoke(false);
}

/// Verifies the common extension macros compile cleanly as C++.
#[test]
fn macros_expand_cleanly_in_cxx() {
    compile_smoke(true);
}

/// Verifies the table of known interned strings has one entry per known-string id and that sample
/// entries match their ids.
#[test]
fn known_strings_follow_their_ids() {
    assert_eq!(elephc_zend::string::KNOWN_STRINGS.len(), ZEND_STR_LAST_KNOWN as usize);
    assert_eq!(elephc_zend::string::KNOWN_STRINGS[ZEND_STR_MESSAGE as usize], "message");
    assert_eq!(elephc_zend::string::KNOWN_STRINGS[ZEND_STR_SENSITIVEPARAMETER as usize], "SensitiveParameter");
    assert_eq!(elephc_zend::string::KNOWN_STRINGS[ZEND_STR_8_DOT_5 as usize], "8.5");
}

/// No object-like macro renames a function the engine exports: a
/// `#define name other` for an exported `name` sends every extension call to
/// another function (it once sent `make_sha1_digest` to the 16-byte
/// `make_digest`). Function-like macros named like an export are php-src's own
/// (`zend_new_array(size)` → `_zend_new_array(size)`, `smart_str_free(s)` →
/// `smart_str_free_ex(s, 0)`): they forward with the same meaning and stay.
#[test]
fn macros_never_shadow_exported_functions() {
    let exported: std::collections::HashSet<&str> = elephc_zend::ABI_HEADER
        .lines()
        .filter(|line| line.ends_with(");") && !line.starts_with('#') && !line.starts_with(' '))
        .filter_map(|line| line.split('(').next()?.rsplit([' ', '*']).next())
        .collect();
    assert!(exported.contains("make_sha1_digest"), "prototypes are recognised");
    let shadowed: Vec<&str> = elephc_zend::MACROS_HEADER
        .lines()
        .filter_map(|line| line.strip_prefix("#define "))
        .filter_map(|rest| {
            let name_end = rest.find(['(', ' ']).unwrap_or(rest.len());
            // Object-like only: the name is followed by a space, not `(`.
            (rest[name_end..].starts_with(' ')).then_some(&rest[..name_end])
        })
        .filter(|name| exported.contains(name))
        .collect();
    assert!(shadowed.is_empty(), "macros shadowing exported functions: {shadowed:?}");
}
