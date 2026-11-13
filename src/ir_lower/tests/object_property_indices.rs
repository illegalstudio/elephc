//! Purpose:
//! Verifies physical Mixed property ordinals are normalized at the introspection ABI.
//!
//! Called from:
//! - Supported-target EIR and emitter CI tests.
//!
//! Key details:
//! - Name and value accessors borrow an integer payload rather than its boxed address.

/// Both introspection helpers normalize their ordinal before materializing the object argument.
fn verify_boxed_object_property_indices(target: &str) {
    let source = "<?php class BoxedPropertyOrdinal { public int $number = 7; } \
        function boxedPropertyOrdinal(object $owner, mixed $index): mixed { \
            echo __elephc_object_prop_name($owner, $index); \
            return __elephc_object_prop_value($owner, $index); } \
        boxedPropertyOrdinal(new BoxedPropertyOrdinal(), 0);";
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let body = assembly.split_once("@fn name=boxedPropertyOrdinal symbol=").unwrap().1
        .split_once("@endfn name=boxedPropertyOrdinal").unwrap().0;
    let mut remaining = body;
    for helper in ["__rt_obj_prop_name", "__rt_obj_prop_value"] {
        let helper_at = remaining.find(helper).unwrap();
        assert!(remaining[..helper_at].contains("__rt_mixed_cast_int"),
            "{target}: {helper} must receive the integer ordinal rather than its box address");
        remaining = &remaining[helper_at + helper.len()..];
    }
}

supported_target_cases!(boxed_object_property_indices, verify_boxed_object_property_indices);
