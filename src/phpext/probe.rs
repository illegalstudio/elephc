//! Purpose:
//! Generates real-header C probes for PHP build identity and the shared value ABI.
//!
//! Called from:
//! - `super::sdk::PhpSdk::discover()` before accepting a PHP SDK.
//!
//! Key details:
//! - Reuses elephc-zend's neutral layout contract, without importing its engine or constants.
//! - C expressions are evaluated against the selected host headers.

#[path = "../../crates/elephc-zend/src/abi/value_layout.rs"]
mod value_layout;

/// Lists C layout expressions from the allocator-independent elephc-zend value contract.
fn expressions() -> Vec<String> {
    let mut expressions = Vec::new();
    macro_rules! size {
        ($($ty:ident),*) => { $(expressions.push(format!("sizeof({})", stringify!($ty)));)* };
    }
    macro_rules! align {
        ($($ty:ident),*) => { $(expressions.push(format!("_Alignof({})", stringify!($ty)));)* };
    }
    macro_rules! offset {
        ($ty:ident: $($field:ident => $c_field:ident),*) => {
            $(expressions.push(format!("offsetof({}, {})", stringify!($ty), stringify!($c_field)));)*
        };
    }
    value_layout::value_layout!(size, align, offset);
    expressions
}

/// Builds a standalone metadata probe; this executable is never part of the PHP module.
pub(super) fn source() -> String {
    let mut source = String::from(include_str!("probe.c"));
    source.push_str("\nint main(void) {\n");
    source.push_str("printf(\"{\\\"php_version_id\\\":%d,\\\"module_api\\\":%d,\\\"zts\\\":%s,\\\"debug\\\":%s,\\\"pointer_size\\\":%zu,\\\"integer_size\\\":%zu,\\\"target\\\":\\\"%s\\\",\\\"build_id\\\":\", PHP_VERSION_ID, ZEND_MODULE_API_NO, USING_ZTS ? \"true\" : \"false\", ZEND_DEBUG ? \"true\" : \"false\", sizeof(void *), sizeof(zend_long), ELEPHC_HOST_TARGET);\n");
    source.push_str("json_string(ZEND_MODULE_BUILD_ID);\nprintf(\",\\\"layout\\\":{\");\n");
    for (index, expression) in expressions().iter().enumerate() {
        let prefix = if index == 0 { "" } else { "," };
        source.push_str(&format!("printf(\"{prefix}\\\"{expression}\\\":%zu\", (size_t)({expression}));\n"));
    }
    source.push_str("puts(\"}}\");\nreturn 0;\n}\n");
    source
}
