//! Purpose:
//! Checks native catalog allocation sharing, isolation and bounded retention.
//!
//! Called from:
//! - The Magician unit test harness.
//!
//! Key details:
//! - Tests inspect collection addresses to catch accidental deep copies on activation.

use super::*;

/// Starts with a deterministic cache local to this test thread.
fn reset_cache() {
    NATIVE_CATALOGS.with(|cache| cache.borrow_mut().clear());
}

/// Supplies an inert descriptor ABI callback; catalog tests never invoke it.
unsafe extern "C" fn invoker(_: *mut c_void, _: *mut RuntimeCell, _: i64) -> *mut RuntimeCell {
    std::ptr::null_mut()
}

/// Seeds metadata that must survive publication without retaining runtime cells.
fn seed(context: &mut ElephcEvalContext, generation: u64) {
    context.use_native_catalog(generation);
    context.native_functions.insert("probe".into(), NativeFunction::new(
        std::ptr::null_mut(), invoker, 0,
    ));
    context.declared_class_names.push("Probe".into());
    assert!(context.define_native_class_parent("Probe", "ParentProbe"));
    context.declared_interface_names.push("ProbeInterface".into());
    context.declared_trait_names.push("ProbeTrait".into());
    context.native_global_constants.insert("CORE_PROBE".into(), EvalNativeGlobalConstant::Int(3));
    context.native_user_constants.insert("USER_PROBE".into(),
        EvalNativeUserConstant::Scalar(EvalNativeGlobalConstant::Int(7)));
    let member = ("probe".to_owned(), "member".to_owned());
    context.native_methods.insert(member.clone(), NativeCallableSignature::new(1));
    context.native_static_methods.insert(member.clone(), NativeCallableSignature::new(1));
    context.native_constructors.insert("probe".into(), NativeCallableSignature::new(1));
    let attributes = vec![EvalAttribute::new("ProbeAttribute", None)];
    context.native_class_attributes.insert("probe".into(), attributes.clone());
    context.native_method_attributes.insert(member.clone(), attributes.clone());
    context.native_constant_attributes.insert(member.clone(), attributes.clone());
    context.native_property_attributes.insert(member.clone(), attributes);
    let property = vec![("member".into(), EvalInterfaceProperty::new("member", true, false))];
    context.native_interface_properties.insert("probeinterface".into(), property.clone());
    context.native_abstract_properties.insert("probe".into(), property);
    context.native_property_types.insert(member.clone(), EvalParameterType::new(Vec::new(), true));
    context.native_property_defaults.insert(member, NativeCallableDefault::Array(vec![
        NativeCallableArrayDefaultElement::positional(NativeCallableDefault::String("fresh".into())),
    ]));
}

/// Fresh activations share each frozen collection while execution state stays local.
#[test]
fn native_catalog_shares_metadata_without_sharing_activation_state() {
    reset_cache();
    let mut first = ElephcEvalContext::new();
    seed(&mut first, 101);
    assert!(first.publish_native_catalog());
    first.call_file = "first.php".into();
    first.included_files.insert("first.php".into());
    first.may_have_foreign_pcntl_callables = true;
    for _ in 0..100 {
        let mut next = ElephcEvalContext::new();
        next.use_native_catalog(101);
        assert!(next.native_catalog_is_ready());
        macro_rules! assert_shared {
            ($($field:ident),+ $(,)?) => { $(
                assert!(std::ptr::eq(&*first.$field, &*next.$field), stringify!($field));
            )+ };
        }
        assert_shared!(
            declared_class_names, declared_interface_names, declared_trait_names,
            native_global_constants, native_user_constants, native_functions,
            native_methods, native_static_methods, native_constructors,
            native_class_parents, native_class_attributes, native_method_attributes,
            native_constant_attributes, native_interface_properties, native_abstract_properties,
            native_property_types, native_property_defaults, native_property_attributes,
        );
        assert!(next.native_function("probe").is_some());
        assert_eq!(next.native_class_parent("Probe"), Some("ParentProbe"));
        assert!(next.call_file.is_empty());
        assert!(next.included_files.is_empty());
        assert!(!next.may_have_foreign_pcntl_callables);
        assert!(next.global_scope.is_none());
        assert!(next.pending_throw.is_none());
        assert!(next.static_properties.is_empty());
        assert!(next.closures.is_empty());
    }
}

/// Explicit metadata writes detach only their collection and cannot poison later hits.
#[test]
fn native_catalog_metadata_writes_are_context_local() {
    reset_cache();
    let mut first = ElephcEvalContext::new();
    seed(&mut first, 102);
    assert!(first.publish_native_catalog());
    let mut writer = ElephcEvalContext::new();
    writer.use_native_catalog(102);
    writer.native_functions.remove("probe");
    writer.declared_class_names.push("Dynamic".into());
    writer.native_class_parents.insert("probe".into(), "Different".into());
    assert!(first.native_function("probe").is_some());
    assert_eq!(first.declared_class_names.len(), 1);
    assert_eq!(first.native_class_parent("Probe"), Some("ParentProbe"));
    assert!(std::ptr::eq(&*first.native_property_defaults, &*writer.native_property_defaults));
    drop(first);
    let mut next = ElephcEvalContext::new();
    next.use_native_catalog(102);
    assert!(next.native_function("probe").is_some());
    assert_eq!(next.declared_class_names.len(), 1);
}

/// Cache eviction bounds retained catalogs without invalidating a live activation.
#[test]
fn native_catalog_eviction_keeps_live_contexts_valid() {
    reset_cache();
    let mut active = ElephcEvalContext::new();
    seed(&mut active, 200);
    assert!(active.publish_native_catalog());
    for generation in 201..220 {
        let mut context = ElephcEvalContext::new();
        seed(&mut context, generation);
        assert!(context.publish_native_catalog());
    }
    assert_eq!(NATIVE_CATALOGS.with(|cache| cache.borrow().len()), CATALOG_CACHE_LIMIT);
    let mut evicted = ElephcEvalContext::new();
    evicted.use_native_catalog(200);
    assert!(!evicted.native_catalog_is_ready());
    assert!(active.native_function("probe").is_some());
}

/// Incomplete catalogs are never reusable, including an otherwise valid partial map.
#[test]
fn native_catalog_failed_registration_is_not_published() {
    reset_cache();
    let mut context = ElephcEvalContext::new();
    seed(&mut context, 301);
    context.fail_native_catalog_registration();
    assert!(!context.publish_native_catalog());
    let mut next = ElephcEvalContext::new();
    next.use_native_catalog(301);
    assert!(!next.native_catalog_is_ready());
    assert!(next.native_functions.is_empty());
    assert!(!ElephcEvalContext::new().publish_native_catalog());
}

/// Dynamic declarations cannot become part of a later activation's native catalog.
#[test]
fn native_catalog_rejects_dynamic_declarations_at_publication() {
    reset_cache();
    let mut context = ElephcEvalContext::new();
    seed(&mut context, 401);
    assert!(context.define_class(EvalClass::new("DynamicProbe", Vec::new(), Vec::new())));
    assert!(!context.publish_native_catalog());
    let mut next = ElephcEvalContext::new();
    next.use_native_catalog(401);
    assert!(!next.native_catalog_is_ready());
    assert!(next.declared_class_names.is_empty());
    let mut context = ElephcEvalContext::new();
    seed(&mut context, 402);
    context.define_function("dynamic_probe", EvalFunction::new("dynamic_probe", Vec::new(), Vec::new()))
        .unwrap();
    assert!(!context.publish_native_catalog());
}

/// Initialization panics return a null ABI handle and leave the module reusable.
#[test]
fn native_catalog_constructor_contains_initialization_panics() {
    reset_cache();
    let mut token = 0u64;
    NATIVE_CATALOGS.with(|cache| {
        let _borrow = cache.borrow_mut();
        let context = unsafe {
            crate::ffi::__elephc_eval_context_new_with_native_catalog(&mut token)
        };
        assert!(context.is_null());
    });
    unsafe {
        let context = crate::ffi::__elephc_eval_context_new_with_native_catalog(&mut token);
        assert!(!context.is_null());
        assert_eq!(crate::ffi::__elephc_eval_context_publish_native_catalog(context), 1);
        crate::ffi::__elephc_eval_context_free(context);
    }
}
