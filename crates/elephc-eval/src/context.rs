//! Purpose:
//! Declares the opaque process-level eval context handle.
//! The full implementation will hold dynamic function, class, constant, and
//! class-like, constant, builtin registries plus runtime hooks.
//!
//! Called from:
//! - `crate::abi`
//! - `crate::__elephc_eval_execute()`
//!
//! Key details:
//! - The handle is intentionally opaque to generated code.
//! - No Rust-owned layout is promised across the C ABI.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;

use crate::abi::ABI_VERSION;
use crate::eval_ir::{
    EvalAttribute, EvalClass, EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalEnum,
    EvalFunction, EvalInterface, EvalInterfaceMethod, EvalInterfaceProperty, EvalTrait,
    EvalVisibility,
};
use crate::scope::ElephcEvalScope;
use crate::stream_resources::EvalStreamResources;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Native descriptor-invoker ABI registered by generated code for AOT functions.
pub type NativeFunctionInvoker =
    unsafe extern "C" fn(*mut c_void, *mut RuntimeCell) -> *mut RuntimeCell;

/// Snapshot of eval execution stacks used to restore caller-sensitive access checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElephcEvalExecutionScope {
    function_stack: Vec<String>,
    class_stack: Vec<String>,
    called_class_stack: Vec<String>,
}

/// Caller-side storage target that can remain linked to an eval object property.
#[derive(Clone)]
pub enum EvalReferenceTarget {
    Variable {
        scope: *mut ElephcEvalScope,
        name: String,
    },
    ArrayElement {
        scope: *mut ElephcEvalScope,
        array_name: String,
        index: RuntimeCellHandle,
    },
    ObjectProperty {
        object: RuntimeCellHandle,
        property: String,
        access_scope: ElephcEvalExecutionScope,
    },
    Cell {
        cell: RuntimeCellHandle,
    },
}

/// Native AOT function callback metadata visible to runtime eval fragments.
#[derive(Clone)]
pub struct NativeFunction {
    descriptor: *mut c_void,
    invoker: NativeFunctionInvoker,
    param_count: usize,
    param_names: Vec<String>,
}

impl NativeFunction {
    /// Creates callback metadata for a descriptor-compatible AOT function.
    pub fn new(
        descriptor: *mut c_void,
        invoker: NativeFunctionInvoker,
        param_count: usize,
    ) -> Self {
        Self {
            descriptor,
            invoker,
            param_count,
            param_names: Vec::new(),
        }
    }

    /// Returns the visible positional parameter count accepted by this callback.
    pub const fn param_count(&self) -> usize {
        self.param_count
    }

    /// Records the PHP parameter name for one positional callback slot.
    pub fn set_param_name(&mut self, index: usize, name: impl Into<String>) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_names.len() < self.param_count {
            self.param_names.resize(self.param_count, String::new());
        }
        self.param_names[index] = name.into();
        true
    }

    /// Returns the PHP-visible parameter names registered for this callback.
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }

    /// Invokes the descriptor-compatible callback with a boxed Mixed arg array.
    ///
    /// # Safety
    /// `arg_array` must be a boxed Mixed indexed array whose elements are boxed
    /// Mixed cells following the descriptor-invoker ABI.
    pub unsafe fn call(&self, arg_array: RuntimeCellHandle) -> RuntimeCellHandle {
        RuntimeCellHandle::from_raw((self.invoker)(self.descriptor, arg_array.as_ptr()))
    }
}

/// Native AOT method or constructor signature metadata visible to eval fragments.
#[derive(Clone)]
pub struct NativeCallableSignature {
    param_count: usize,
    param_names: Vec<String>,
}

impl NativeCallableSignature {
    /// Creates signature metadata with the visible positional parameter count.
    pub const fn new(param_count: usize) -> Self {
        Self {
            param_count,
            param_names: Vec::new(),
        }
    }

    /// Returns the visible positional parameter count accepted by this callable.
    pub const fn param_count(&self) -> usize {
        self.param_count
    }

    /// Records the PHP parameter name for one positional callable slot.
    pub fn set_param_name(&mut self, index: usize, name: impl Into<String>) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_names.len() < self.param_count {
            self.param_names.resize(self.param_count, String::new());
        }
        self.param_names[index] = name.into();
        true
    }

    /// Returns the PHP-visible parameter names registered for this callable.
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }
}

/// PHP class-like declaration kind targeted by a dynamic `class_alias()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvalClassAliasKind {
    Class,
    Interface,
    Trait,
    Enum,
}

/// Dynamic alias target and kind recorded for eval-visible class-like symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EvalClassAlias {
    target: String,
    kind: EvalClassAliasKind,
}

/// Process-level eval context passed opaquely across the C ABI.
///
/// Generated code never inspects this layout directly; it only passes pointers
/// back to the eval bridge. Keeping a concrete Rust type here lets the bridge
/// grow dynamic registries without exposing them to generated assembly.
pub struct ElephcEvalContext {
    abi_version: u32,
    classes: HashMap<String, EvalClass>,
    class_aliases: HashMap<String, EvalClassAlias>,
    declared_class_names: Vec<String>,
    interfaces: HashMap<String, EvalInterface>,
    declared_interface_names: Vec<String>,
    traits: HashMap<String, EvalTrait>,
    declared_trait_names: Vec<String>,
    enums: HashMap<String, EvalEnum>,
    declared_enum_names: Vec<String>,
    enum_cases: HashMap<(String, String), RuntimeCellHandle>,
    enum_case_values: HashMap<(String, String), RuntimeCellHandle>,
    constants: HashMap<String, RuntimeCellHandle>,
    functions: HashMap<String, EvalFunction>,
    native_functions: HashMap<String, NativeFunction>,
    native_methods: HashMap<(String, String), NativeCallableSignature>,
    native_static_methods: HashMap<(String, String), NativeCallableSignature>,
    native_constructors: HashMap<String, NativeCallableSignature>,
    static_locals: HashMap<(String, String), RuntimeCellHandle>,
    static_properties: HashMap<(String, String), RuntimeCellHandle>,
    class_constants: HashMap<(String, String), RuntimeCellHandle>,
    included_files: HashSet<String>,
    dynamic_objects: HashMap<u64, String>,
    dynamic_property_aliases: HashMap<(u64, String), EvalReferenceTarget>,
    eval_reflection_attributes: HashMap<u64, EvalAttribute>,
    eval_reflection_classes: HashMap<u64, String>,
    eval_reflection_functions: HashMap<u64, String>,
    eval_reflection_methods: HashMap<u64, (String, String)>,
    eval_reflection_properties: HashMap<u64, (String, String)>,
    global_scope: Option<*mut ElephcEvalScope>,
    function_stack: Vec<String>,
    class_stack: Vec<String>,
    called_class_stack: Vec<String>,
    pending_throw: Option<RuntimeCellHandle>,
    spl_autoload_extensions: String,
    streams: EvalStreamResources,
    json_last_error: i64,
    json_last_error_msg: String,
    call_file: String,
    call_dir: String,
    call_line: i64,
    file_magic_override: Option<String>,
}

impl ElephcEvalContext {
    /// Creates a context using the current eval bridge ABI version.
    pub fn new() -> Self {
        Self {
            abi_version: ABI_VERSION,
            classes: HashMap::new(),
            class_aliases: HashMap::new(),
            declared_class_names: Vec::new(),
            interfaces: HashMap::new(),
            declared_interface_names: Vec::new(),
            traits: HashMap::new(),
            declared_trait_names: Vec::new(),
            enums: HashMap::new(),
            declared_enum_names: Vec::new(),
            enum_cases: HashMap::new(),
            enum_case_values: HashMap::new(),
            constants: HashMap::new(),
            functions: HashMap::new(),
            native_functions: HashMap::new(),
            native_methods: HashMap::new(),
            native_static_methods: HashMap::new(),
            native_constructors: HashMap::new(),
            static_locals: HashMap::new(),
            static_properties: HashMap::new(),
            class_constants: HashMap::new(),
            included_files: HashSet::new(),
            dynamic_objects: HashMap::new(),
            dynamic_property_aliases: HashMap::new(),
            eval_reflection_attributes: HashMap::new(),
            eval_reflection_classes: HashMap::new(),
            eval_reflection_functions: HashMap::new(),
            eval_reflection_methods: HashMap::new(),
            eval_reflection_properties: HashMap::new(),
            global_scope: None,
            function_stack: Vec::new(),
            class_stack: Vec::new(),
            called_class_stack: Vec::new(),
            pending_throw: None,
            spl_autoload_extensions: String::from(".inc,.php"),
            streams: EvalStreamResources::default(),
            json_last_error: 0,
            json_last_error_msg: String::from("No error"),
            call_file: String::new(),
            call_dir: String::new(),
            call_line: 0,
            file_magic_override: None,
        }
    }

    /// Creates a context with an explicit ABI version for compatibility tests.
    #[cfg(test)]
    pub fn for_abi_version(abi_version: u32) -> Self {
        Self {
            abi_version,
            classes: HashMap::new(),
            class_aliases: HashMap::new(),
            declared_class_names: Vec::new(),
            interfaces: HashMap::new(),
            declared_interface_names: Vec::new(),
            traits: HashMap::new(),
            declared_trait_names: Vec::new(),
            enums: HashMap::new(),
            declared_enum_names: Vec::new(),
            enum_cases: HashMap::new(),
            enum_case_values: HashMap::new(),
            constants: HashMap::new(),
            functions: HashMap::new(),
            native_functions: HashMap::new(),
            native_methods: HashMap::new(),
            native_static_methods: HashMap::new(),
            native_constructors: HashMap::new(),
            static_locals: HashMap::new(),
            static_properties: HashMap::new(),
            class_constants: HashMap::new(),
            included_files: HashSet::new(),
            dynamic_objects: HashMap::new(),
            dynamic_property_aliases: HashMap::new(),
            eval_reflection_attributes: HashMap::new(),
            eval_reflection_classes: HashMap::new(),
            eval_reflection_functions: HashMap::new(),
            eval_reflection_methods: HashMap::new(),
            eval_reflection_properties: HashMap::new(),
            global_scope: None,
            function_stack: Vec::new(),
            class_stack: Vec::new(),
            called_class_stack: Vec::new(),
            pending_throw: None,
            spl_autoload_extensions: String::from(".inc,.php"),
            streams: EvalStreamResources::default(),
            json_last_error: 0,
            json_last_error_msg: String::from("No error"),
            call_file: String::new(),
            call_dir: String::new(),
            call_line: 0,
            file_magic_override: None,
        }
    }

    /// Returns the ABI version this context was created for.
    pub const fn abi_version(&self) -> u32 {
        self.abi_version
    }

    /// Defines an eval-declared class, failing if this context already has it.
    pub fn define_class(&mut self, class: EvalClass) -> bool {
        let key = normalize_class_name(class.name());
        if self.classes.contains_key(&key)
            || self.class_aliases.contains_key(&key)
            || self.interfaces.contains_key(&key)
            || self.traits.contains_key(&key)
            || self.enums.contains_key(&key)
        {
            return false;
        }
        self.declared_class_names.push(class.name().to_string());
        self.classes.insert(key, class);
        true
    }

    /// Returns true when this eval context has a dynamic class or alias with the requested name.
    pub fn has_class(&self, name: &str) -> bool {
        let key = normalize_class_name(name);
        self.classes.contains_key(&key)
            || self.class_aliases.get(&key).is_some_and(|alias| {
                matches!(
                    alias.kind,
                    EvalClassAliasKind::Class | EvalClassAliasKind::Enum
                )
            })
    }

    /// Returns a dynamic eval class by PHP case-insensitive class name or alias.
    pub fn class(&self, name: &str) -> Option<&EvalClass> {
        let key = normalize_class_name(name);
        if let Some(class) = self.classes.get(&key) {
            return Some(class);
        }
        let alias = self.class_aliases.get(&key)?;
        if !matches!(
            alias.kind,
            EvalClassAliasKind::Class | EvalClassAliasKind::Enum
        ) {
            return None;
        }
        self.classes.get(&normalize_class_name(&alias.target))
    }

    /// Resolves a PHP class name or alias to the canonical target spelling stored by eval.
    pub fn resolve_class_name(&self, name: &str) -> Option<String> {
        let key = normalize_class_name(name);
        if let Some(class) = self.classes.get(&key) {
            return Some(class.name().to_string());
        }
        self.class_aliases.get(&key).and_then(|alias| {
            matches!(
                alias.kind,
                EvalClassAliasKind::Class | EvalClassAliasKind::Enum
            )
            .then(|| alias.target.clone())
        })
    }

    /// Resolves a PHP class-like name to eval class, interface, trait, or alias spelling.
    pub fn resolve_class_like_name(&self, name: &str) -> Option<String> {
        let key = normalize_class_name(name);
        if let Some(class) = self.classes.get(&key) {
            return Some(class.name().to_string());
        }
        if let Some(interface) = self.interfaces.get(&key) {
            return Some(interface.name().to_string());
        }
        if let Some(trait_decl) = self.traits.get(&key) {
            return Some(trait_decl.name().to_string());
        }
        if let Some(enum_decl) = self.enums.get(&key) {
            return Some(enum_decl.name().to_string());
        }
        self.class_aliases
            .get(&key)
            .map(|alias| alias.target.clone())
    }

    /// Defines an alias for an eval-declared class or an already known alias.
    pub fn define_class_alias(&mut self, original: &str, alias: &str) -> bool {
        let Some((target, kind)) = self.resolve_class_like_alias_target(original) else {
            return false;
        };
        self.define_class_alias_with_kind(&target, alias, kind)
    }

    /// Defines an alias for a runtime-visible class whose metadata lives outside eval.
    pub fn define_external_class_alias(&mut self, original: &str, alias: &str) -> bool {
        self.define_class_alias_with_kind(original, alias, EvalClassAliasKind::Class)
    }

    /// Defines an alias for a runtime-visible interface whose metadata lives outside eval.
    pub fn define_external_interface_alias(&mut self, original: &str, alias: &str) -> bool {
        self.define_class_alias_with_kind(original, alias, EvalClassAliasKind::Interface)
    }

    /// Defines an alias for a runtime-visible trait whose metadata lives outside eval.
    pub fn define_external_trait_alias(&mut self, original: &str, alias: &str) -> bool {
        self.define_class_alias_with_kind(original, alias, EvalClassAliasKind::Trait)
    }

    /// Defines an alias for a runtime-visible enum whose metadata lives outside eval.
    pub fn define_external_enum_alias(&mut self, original: &str, alias: &str) -> bool {
        self.define_class_alias_with_kind(original, alias, EvalClassAliasKind::Enum)
    }

    /// Resolves the canonical target and declaration kind for a class-like alias source.
    fn resolve_class_like_alias_target(
        &self,
        original: &str,
    ) -> Option<(String, EvalClassAliasKind)> {
        let key = normalize_class_name(original);
        if let Some(enum_decl) = self.enums.get(&key) {
            return Some((enum_decl.name().to_string(), EvalClassAliasKind::Enum));
        }
        if let Some(class) = self.classes.get(&key) {
            return Some((class.name().to_string(), EvalClassAliasKind::Class));
        }
        if let Some(interface) = self.interfaces.get(&key) {
            return Some((interface.name().to_string(), EvalClassAliasKind::Interface));
        }
        if let Some(trait_decl) = self.traits.get(&key) {
            return Some((trait_decl.name().to_string(), EvalClassAliasKind::Trait));
        }
        self.class_aliases
            .get(&key)
            .map(|alias| (alias.target.clone(), alias.kind))
    }

    /// Defines one class-like alias after the caller has resolved the target kind.
    fn define_class_alias_with_kind(
        &mut self,
        original: &str,
        alias: &str,
        kind: EvalClassAliasKind,
    ) -> bool {
        let alias_key = normalize_class_name(alias);
        if alias_key.is_empty()
            || self.classes.contains_key(&alias_key)
            || self.interfaces.contains_key(&alias_key)
            || self.traits.contains_key(&alias_key)
            || self.enums.contains_key(&alias_key)
            || self.class_aliases.contains_key(&alias_key)
        {
            return false;
        }
        self.class_aliases.insert(
            alias_key,
            EvalClassAlias {
                target: original.trim_start_matches('\\').to_string(),
                kind,
            },
        );
        true
    }

    /// Returns class names declared or aliased through eval in PHP-visible order.
    pub fn declared_class_names(&self) -> &[String] {
        &self.declared_class_names
    }

    /// Defines an eval-declared interface, failing if this context already has the name.
    pub fn define_interface(&mut self, interface: EvalInterface) -> bool {
        let key = normalize_class_name(interface.name());
        if self.interfaces.contains_key(&key)
            || self.classes.contains_key(&key)
            || self.traits.contains_key(&key)
            || self.enums.contains_key(&key)
            || self.class_aliases.contains_key(&key)
        {
            return false;
        }
        self.declared_interface_names
            .push(interface.name().to_string());
        self.interfaces.insert(key, interface);
        true
    }

    /// Returns true when this eval context has a dynamic interface with the requested name.
    pub fn has_interface(&self, name: &str) -> bool {
        let key = normalize_class_name(name);
        self.interfaces.contains_key(&key)
            || self
                .class_aliases
                .get(&key)
                .is_some_and(|alias| alias.kind == EvalClassAliasKind::Interface)
    }

    /// Returns a dynamic eval interface by PHP case-insensitive interface name.
    pub fn interface(&self, name: &str) -> Option<&EvalInterface> {
        let key = normalize_class_name(name);
        if let Some(interface) = self.interfaces.get(&key) {
            return Some(interface);
        }
        let alias = self.class_aliases.get(&key)?;
        (alias.kind == EvalClassAliasKind::Interface)
            .then(|| self.interfaces.get(&normalize_class_name(&alias.target)))
            .flatten()
    }

    /// Returns interface names declared through eval in PHP-visible order.
    pub fn declared_interface_names(&self) -> &[String] {
        &self.declared_interface_names
    }

    /// Defines an eval-declared trait, failing if this context already has the name.
    pub fn define_trait(&mut self, trait_decl: EvalTrait) -> bool {
        let key = normalize_class_name(trait_decl.name());
        if self.traits.contains_key(&key)
            || self.classes.contains_key(&key)
            || self.interfaces.contains_key(&key)
            || self.enums.contains_key(&key)
            || self.class_aliases.contains_key(&key)
        {
            return false;
        }
        self.declared_trait_names
            .push(trait_decl.name().to_string());
        self.traits.insert(key, trait_decl);
        true
    }

    /// Returns true when this eval context has a dynamic trait with the requested name.
    pub fn has_trait(&self, name: &str) -> bool {
        let key = normalize_class_name(name);
        self.traits.contains_key(&key)
            || self
                .class_aliases
                .get(&key)
                .is_some_and(|alias| alias.kind == EvalClassAliasKind::Trait)
    }

    /// Returns a dynamic eval trait by PHP case-insensitive trait name.
    pub fn trait_decl(&self, name: &str) -> Option<&EvalTrait> {
        let key = normalize_class_name(name);
        if let Some(trait_decl) = self.traits.get(&key) {
            return Some(trait_decl);
        }
        let alias = self.class_aliases.get(&key)?;
        (alias.kind == EvalClassAliasKind::Trait)
            .then(|| self.traits.get(&normalize_class_name(&alias.target)))
            .flatten()
    }

    /// Returns trait names declared through eval in PHP-visible order.
    pub fn declared_trait_names(&self) -> &[String] {
        &self.declared_trait_names
    }

    /// Defines an eval-declared enum plus class-shaped metadata for dispatch.
    pub fn define_enum(&mut self, enum_decl: EvalEnum) -> bool {
        let key = normalize_class_name(enum_decl.name());
        if self.enums.contains_key(&key)
            || self.classes.contains_key(&key)
            || self.interfaces.contains_key(&key)
            || self.traits.contains_key(&key)
            || self.class_aliases.contains_key(&key)
        {
            return false;
        }
        self.declared_enum_names
            .push(enum_decl.name().trim_start_matches('\\').to_string());
        self.declared_class_names
            .push(enum_decl.name().trim_start_matches('\\').to_string());
        self.classes
            .insert(key.clone(), enum_decl.as_class_metadata());
        self.enums.insert(key, enum_decl);
        true
    }

    /// Returns true when this eval context has a dynamic enum with the requested name.
    pub fn has_enum(&self, name: &str) -> bool {
        let key = normalize_class_name(name);
        self.enums.contains_key(&key)
            || self
                .class_aliases
                .get(&key)
                .is_some_and(|alias| alias.kind == EvalClassAliasKind::Enum)
    }

    /// Returns a dynamic eval enum by PHP case-insensitive enum name.
    pub fn enum_decl(&self, name: &str) -> Option<&EvalEnum> {
        let key = normalize_class_name(name);
        if let Some(enum_decl) = self.enums.get(&key) {
            return Some(enum_decl);
        }
        let alias = self.class_aliases.get(&key)?;
        (alias.kind == EvalClassAliasKind::Enum)
            .then(|| self.enums.get(&normalize_class_name(&alias.target)))
            .flatten()
    }

    /// Resolves an enum name or enum alias to the canonical eval enum spelling.
    pub fn resolve_enum_name(&self, name: &str) -> Option<String> {
        let key = normalize_class_name(name);
        if let Some(enum_decl) = self.enums.get(&key) {
            return Some(enum_decl.name().to_string());
        }
        self.class_aliases.get(&key).and_then(|alias| {
            (alias.kind == EvalClassAliasKind::Enum).then(|| alias.target.clone())
        })
    }

    /// Returns enum names declared through eval in PHP-visible order.
    pub fn declared_enum_names(&self) -> &[String] {
        &self.declared_enum_names
    }

    /// Returns a materialized singleton case object for one eval enum case.
    pub fn enum_case(&self, enum_name: &str, case_name: &str) -> Option<RuntimeCellHandle> {
        let enum_name = self
            .resolve_enum_name(enum_name)
            .unwrap_or_else(|| enum_name.trim_start_matches('\\').to_string());
        self.enum_cases
            .get(&(
                normalize_class_name(&enum_name),
                normalize_enum_case_name(case_name),
            ))
            .copied()
    }

    /// Stores a materialized singleton case object and returns any replaced distinct cell.
    pub fn set_enum_case(
        &mut self,
        enum_name: &str,
        case_name: &str,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self.enum_cases.insert(
            (
                normalize_class_name(enum_name),
                normalize_enum_case_name(case_name),
            ),
            cell,
        );
        previous.filter(|previous| *previous != cell)
    }

    /// Returns a materialized backing value for one eval backed-enum case.
    pub fn enum_case_value(&self, enum_name: &str, case_name: &str) -> Option<RuntimeCellHandle> {
        let enum_name = self
            .resolve_enum_name(enum_name)
            .unwrap_or_else(|| enum_name.trim_start_matches('\\').to_string());
        self.enum_case_values
            .get(&(
                normalize_class_name(&enum_name),
                normalize_enum_case_name(case_name),
            ))
            .copied()
    }

    /// Stores a materialized backing value and returns any replaced distinct cell.
    pub fn set_enum_case_value(
        &mut self,
        enum_name: &str,
        case_name: &str,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self.enum_case_values.insert(
            (
                normalize_class_name(enum_name),
                normalize_enum_case_name(case_name),
            ),
            cell,
        );
        previous.filter(|previous| *previous != cell)
    }

    /// Records that one runtime object handle was created for an eval-declared class.
    pub fn register_dynamic_object(&mut self, identity: u64, class_name: &str) {
        let class_name = self
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.to_string());
        self.dynamic_objects
            .insert(identity, normalize_class_name(&class_name));
    }

    /// Returns the dynamic eval class metadata associated with one object identity.
    pub fn dynamic_object_class(&self, identity: u64) -> Option<&EvalClass> {
        self.dynamic_objects
            .get(&identity)
            .and_then(|class_key| self.classes.get(class_key))
    }

    /// Binds one eval object property slot to a persistent PHP reference target.
    pub fn bind_dynamic_property_alias(
        &mut self,
        identity: u64,
        storage_property_name: &str,
        target: EvalReferenceTarget,
    ) -> Option<EvalReferenceTarget> {
        self.dynamic_property_aliases
            .insert((identity, storage_property_name.to_string()), target)
    }

    /// Returns the persistent reference target bound to one eval object property slot.
    pub fn dynamic_property_alias(
        &self,
        identity: u64,
        storage_property_name: &str,
    ) -> Option<&EvalReferenceTarget> {
        self.dynamic_property_aliases
            .get(&(identity, storage_property_name.to_string()))
    }

    /// Removes the persistent reference target for one eval object property slot.
    pub fn remove_dynamic_property_alias(
        &mut self,
        identity: u64,
        storage_property_name: &str,
    ) -> Option<EvalReferenceTarget> {
        self.dynamic_property_aliases
            .remove(&(identity, storage_property_name.to_string()))
    }

    /// Copies persistent property aliases from a source object identity to a clone identity.
    pub fn clone_dynamic_property_aliases(&mut self, source_identity: u64, clone_identity: u64) {
        let aliases = self
            .dynamic_property_aliases
            .iter()
            .filter_map(|((identity, property), target)| {
                (*identity == source_identity).then(|| (property.clone(), target.clone()))
            })
            .collect::<Vec<_>>();
        for (property, target) in aliases {
            self.bind_dynamic_property_alias(clone_identity, &property, target);
        }
    }

    /// Records eval-declared attribute metadata for one synthetic ReflectionAttribute object.
    pub fn register_eval_reflection_attribute(&mut self, identity: u64, attribute: EvalAttribute) {
        self.eval_reflection_attributes.insert(identity, attribute);
    }

    /// Returns eval-declared attribute metadata attached to a synthetic ReflectionAttribute.
    pub fn eval_reflection_attribute(&self, identity: u64) -> Option<&EvalAttribute> {
        self.eval_reflection_attributes.get(&identity)
    }

    /// Records reflected class metadata for one synthetic ReflectionClass object.
    pub fn register_eval_reflection_class(&mut self, identity: u64, class_name: &str) {
        self.eval_reflection_classes
            .insert(identity, class_name.trim_start_matches('\\').to_string());
    }

    /// Returns the reflected class name attached to a synthetic ReflectionClass.
    pub fn eval_reflection_class_name(&self, identity: u64) -> Option<&str> {
        self.eval_reflection_classes
            .get(&identity)
            .map(String::as_str)
    }

    /// Records reflected function metadata for one synthetic ReflectionFunction object.
    pub fn register_eval_reflection_function(&mut self, identity: u64, function_name: &str) {
        self.eval_reflection_functions
            .insert(identity, function_name.trim_start_matches('\\').to_string());
    }

    /// Returns the reflected function name attached to a synthetic ReflectionFunction.
    pub fn eval_reflection_function_name(&self, identity: u64) -> Option<&str> {
        self.eval_reflection_functions
            .get(&identity)
            .map(String::as_str)
    }

    /// Records reflected method metadata for one synthetic ReflectionMethod object.
    pub fn register_eval_reflection_method(
        &mut self,
        identity: u64,
        declaring_class: &str,
        method_name: &str,
    ) {
        self.eval_reflection_methods.insert(
            identity,
            (
                declaring_class.trim_start_matches('\\').to_string(),
                method_name.to_string(),
            ),
        );
    }

    /// Returns the declaring class and method name attached to a synthetic ReflectionMethod.
    pub fn eval_reflection_method(&self, identity: u64) -> Option<(&str, &str)> {
        self.eval_reflection_methods
            .get(&identity)
            .map(|(class, method)| (class.as_str(), method.as_str()))
    }

    /// Records reflected property metadata for one synthetic ReflectionProperty object.
    pub fn register_eval_reflection_property(
        &mut self,
        identity: u64,
        declaring_class: &str,
        property_name: &str,
    ) {
        self.eval_reflection_properties.insert(
            identity,
            (
                declaring_class.trim_start_matches('\\').to_string(),
                property_name.to_string(),
            ),
        );
    }

    /// Returns the declaring class and property name attached to a synthetic ReflectionProperty.
    pub fn eval_reflection_property(&self, identity: u64) -> Option<(&str, &str)> {
        self.eval_reflection_properties
            .get(&identity)
            .map(|(class, property)| (class.as_str(), property.as_str()))
    }

    /// Returns eval-declared class metadata from parent to child for construction.
    pub fn class_chain(&self, name: &str) -> Vec<EvalClass> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        self.collect_class_chain(name, &mut chain, &mut seen);
        chain
    }

    /// Collects one eval-declared class ancestry chain without following cycles.
    fn collect_class_chain(
        &self,
        name: &str,
        chain: &mut Vec<EvalClass>,
        seen: &mut HashSet<String>,
    ) {
        let key = normalize_class_name(name);
        if !seen.insert(key.clone()) {
            return;
        }
        let Some(class) = self.classes.get(&key) else {
            return;
        };
        if let Some(parent) = class.parent() {
            self.collect_class_chain(parent, chain, seen);
        }
        chain.push(class.clone());
    }

    /// Finds a method in an eval-declared class or its eval-declared parents.
    pub fn class_method(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<(String, EvalClassMethod)> {
        let mut current_name = self.resolve_class_name(class_name)?;
        let mut seen = HashSet::new();
        loop {
            let key = normalize_class_name(&current_name);
            if !seen.insert(key.clone()) {
                return None;
            }
            let class = self.classes.get(&key)?;
            if let Some(method) = class.method(method_name) {
                return Some((class.name().to_string(), method.clone()));
            }
            current_name = class.parent()?.to_string();
        }
    }

    /// Finds a method declared directly by one eval-declared class.
    pub fn class_own_method(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<(String, EvalClassMethod)> {
        let class = self.class(class_name)?;
        class
            .method(method_name)
            .map(|method| (class.name().to_string(), method.clone()))
    }

    /// Finds a class-like constant on an eval class, interface, trait, or inherited relation.
    pub fn class_constant(
        &self,
        class_name: &str,
        constant_name: &str,
    ) -> Option<(String, EvalClassConstant)> {
        if self.has_class(class_name) {
            return self.class_or_interface_constant(class_name, constant_name);
        }
        if self.has_interface(class_name) {
            return self.interface_constant(class_name, constant_name);
        }
        if let Some(trait_decl) = self.trait_decl(class_name) {
            if let Some(constant) = trait_decl.constant(constant_name) {
                return Some((trait_decl.name().to_string(), constant.clone()));
            }
        }
        None
    }

    /// Finds a class constant in an eval-declared class, parents, or implemented interfaces.
    fn class_or_interface_constant(
        &self,
        class_name: &str,
        constant_name: &str,
    ) -> Option<(String, EvalClassConstant)> {
        let mut current_name = self.resolve_class_name(class_name)?;
        let mut seen = HashSet::new();
        loop {
            let key = normalize_class_name(&current_name);
            if !seen.insert(key.clone()) {
                return None;
            }
            let class = self.classes.get(&key)?;
            if let Some(constant) = class.constant(constant_name) {
                return Some((class.name().to_string(), constant.clone()));
            }
            if let Some(parent) = class.parent() {
                current_name = parent.to_string();
            } else {
                break;
            }
        }
        for interface_name in self.class_interface_names(class_name) {
            if let Some(found) = self.interface_constant(&interface_name, constant_name) {
                return Some(found);
            }
        }
        None
    }

    /// Finds a constant declared on an eval interface or inherited parent interface.
    pub fn interface_constant(
        &self,
        interface_name: &str,
        constant_name: &str,
    ) -> Option<(String, EvalClassConstant)> {
        let interface = self.interface(interface_name)?;
        if let Some(constant) = interface.constant(constant_name) {
            return Some((interface.name().to_string(), constant.clone()));
        }
        for parent in interface.parents() {
            if let Some(found) = self.interface_constant(parent, constant_name) {
                return Some(found);
            }
        }
        None
    }

    /// Finds a class constant declared directly by one eval-declared class.
    pub fn class_own_constant(
        &self,
        class_name: &str,
        constant_name: &str,
    ) -> Option<(String, EvalClassConstant)> {
        let class = self.class(class_name)?;
        class
            .constant(constant_name)
            .map(|constant| (class.name().to_string(), constant.clone()))
    }

    /// Finds a property in an eval-declared class or its eval-declared parents.
    pub fn class_property(
        &self,
        class_name: &str,
        property_name: &str,
    ) -> Option<(String, EvalClassProperty)> {
        let mut current_name = self.resolve_class_name(class_name)?;
        let mut seen = HashSet::new();
        loop {
            let key = normalize_class_name(&current_name);
            if !seen.insert(key.clone()) {
                return None;
            }
            let class = self.classes.get(&key)?;
            if let Some(property) = class
                .properties()
                .iter()
                .find(|property| property.name() == property_name)
            {
                return Some((class.name().to_string(), property.clone()));
            }
            current_name = class.parent()?.to_string();
        }
    }

    /// Finds a property declared directly by one eval-declared class.
    pub fn class_own_property(
        &self,
        class_name: &str,
        property_name: &str,
    ) -> Option<(String, EvalClassProperty)> {
        let class = self.class(class_name)?;
        class
            .properties()
            .iter()
            .find(|property| property.name() == property_name)
            .map(|property| (class.name().to_string(), property.clone()))
    }

    /// Returns direct and inherited parent class names for an eval-declared class.
    pub fn class_parent_names(&self, class_name: &str) -> Vec<String> {
        let mut parents = Vec::new();
        let mut current = self.class(class_name).and_then(EvalClass::parent);
        let mut seen = HashSet::new();
        while let Some(parent) = current {
            let Some(parent_class) = self.class(parent) else {
                break;
            };
            let key = normalize_class_name(parent_class.name());
            if !seen.insert(key) {
                break;
            }
            parents.push(parent_class.name().trim_start_matches('\\').to_string());
            current = parent_class.parent();
        }
        parents
    }

    /// Returns direct and inherited interface names for an eval-declared class.
    pub fn class_interface_names(&self, class_name: &str) -> Vec<String> {
        let mut interfaces = Vec::new();
        let mut seen = HashSet::new();
        for class in self.class_chain(class_name) {
            for interface in class.interfaces() {
                push_unique_class_name(interface, &mut interfaces, &mut seen);
                self.collect_interface_parent_names(interface, &mut interfaces, &mut seen);
            }
        }
        interfaces
    }

    /// Returns trait names used directly by an eval-declared class.
    pub fn class_trait_names(&self, class_name: &str) -> Vec<String> {
        self.class(class_name).map_or_else(Vec::new, |class| {
            let mut traits = Vec::new();
            let mut seen = HashSet::new();
            for trait_name in class.traits() {
                push_unique_class_name(trait_name, &mut traits, &mut seen);
            }
            traits
        })
    }

    /// Returns PHP case-insensitive method names visible to `ReflectionClass::hasMethod()`.
    pub fn class_method_names(&self, class_name: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for class in self.class_chain(class_name).into_iter().rev() {
            for method in class.methods() {
                push_unique_method_name(method.name(), &mut names, &mut seen);
            }
            if let Some(enum_decl) = self.enum_decl(class.name()) {
                push_unique_method_name("cases", &mut names, &mut seen);
                if enum_decl.backing_type().is_some() {
                    push_unique_method_name("from", &mut names, &mut seen);
                    push_unique_method_name("tryFrom", &mut names, &mut seen);
                }
            }
        }
        names
    }

    /// Returns PHP case-sensitive property names visible to `ReflectionClass::hasProperty()`.
    pub fn class_property_names(&self, class_name: &str) -> Vec<String> {
        let reflected_name = self
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        if let Some(enum_decl) = self.enum_decl(&reflected_name) {
            push_unique_property_name("name", &mut names, &mut seen);
            if enum_decl.backing_type().is_some() {
                push_unique_property_name("value", &mut names, &mut seen);
            }
        }
        for class in self.class_chain(&reflected_name) {
            let declaring_is_reflected = same_class_name(class.name(), &reflected_name);
            for property in class.properties() {
                if property.visibility() == EvalVisibility::Private && !declaring_is_reflected {
                    continue;
                }
                push_unique_property_name(property.name(), &mut names, &mut seen);
            }
        }
        names
    }

    /// Returns PHP case-sensitive constant names visible to `ReflectionClass::hasConstant()`.
    pub fn class_constant_names(&self, class_name: &str) -> Vec<String> {
        let reflected_name = self
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        if let Some(enum_decl) = self.enum_decl(&reflected_name) {
            for case in enum_decl.cases() {
                push_unique_constant_name(case.name(), &mut names, &mut seen);
            }
        }
        for class in self.class_chain(&reflected_name).into_iter().rev() {
            for constant in class.constants() {
                push_unique_constant_name(constant.name(), &mut names, &mut seen);
            }
            for interface_name in class.interfaces() {
                for constant in self.interface_constant_names(interface_name) {
                    push_unique_constant_name(&constant, &mut names, &mut seen);
                }
            }
        }
        names
    }

    /// Returns PHP case-insensitive method names declared by an eval interface hierarchy.
    pub fn interface_method_names(&self, interface_name: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for method in self.interface_method_requirements(interface_name) {
            push_unique_method_name(method.name(), &mut names, &mut seen);
        }
        names
    }

    /// Returns PHP case-sensitive property names declared by an eval interface hierarchy.
    pub fn interface_property_names(&self, interface_name: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for property in self.interface_property_requirements(interface_name) {
            push_unique_property_name(property.name(), &mut names, &mut seen);
        }
        names
    }

    /// Returns PHP case-sensitive constant names declared by an eval interface hierarchy.
    pub fn interface_constant_names(&self, interface_name: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        self.collect_interface_constant_names(interface_name, &mut names, &mut seen);
        names
    }

    /// Collects eval interface constants without duplicating inherited names.
    fn collect_interface_constant_names(
        &self,
        interface_name: &str,
        names: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        let Some(interface) = self.interface(interface_name) else {
            return;
        };
        for parent in interface.parents() {
            self.collect_interface_constant_names(parent, names, seen);
        }
        for constant in interface.constants() {
            push_unique_constant_name(constant.name(), names, seen);
        }
    }

    /// Returns PHP case-insensitive direct method names declared by an eval trait.
    pub fn trait_method_names(&self, trait_name: &str) -> Vec<String> {
        let Some(trait_decl) = self.trait_decl(trait_name) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for method in trait_decl.methods() {
            push_unique_method_name(method.name(), &mut names, &mut seen);
        }
        names
    }

    /// Returns PHP case-sensitive direct property names declared by an eval trait.
    pub fn trait_property_names(&self, trait_name: &str) -> Vec<String> {
        let Some(trait_decl) = self.trait_decl(trait_name) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for property in trait_decl.properties() {
            push_unique_property_name(property.name(), &mut names, &mut seen);
        }
        names
    }

    /// Returns PHP case-sensitive direct constant names declared by an eval trait.
    pub fn trait_constant_names(&self, trait_name: &str) -> Vec<String> {
        let Some(trait_decl) = self.trait_decl(trait_name) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for constant in trait_decl.constants() {
            push_unique_constant_name(constant.name(), &mut names, &mut seen);
        }
        names
    }

    /// Returns parent interface names for an eval-declared interface.
    pub fn interface_parent_names(&self, interface_name: &str) -> Vec<String> {
        let mut parents = Vec::new();
        let mut seen = HashSet::new();
        self.collect_interface_parent_names(interface_name, &mut parents, &mut seen);
        parents
    }

    /// Collects eval-declared interface parents without following cycles.
    fn collect_interface_parent_names(
        &self,
        interface_name: &str,
        names: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        let Some(interface) = self.interface(interface_name) else {
            return;
        };
        for parent in interface.parents() {
            push_unique_class_name(parent, names, seen);
            self.collect_interface_parent_names(parent, names, seen);
        }
    }

    /// Returns direct and inherited method requirements for an eval interface.
    pub fn interface_method_requirements(&self, interface_name: &str) -> Vec<EvalInterfaceMethod> {
        self.interface_method_requirements_with_owners(interface_name)
            .into_iter()
            .map(|(_, method)| method)
            .collect()
    }

    /// Returns direct and inherited method requirements with their declaring interface.
    pub fn interface_method_requirements_with_owners(
        &self,
        interface_name: &str,
    ) -> Vec<(String, EvalInterfaceMethod)> {
        let mut methods = Vec::new();
        let mut seen_interfaces = HashSet::new();
        let mut seen_methods = HashSet::new();
        self.collect_interface_method_requirements(
            interface_name,
            &mut methods,
            &mut seen_interfaces,
            &mut seen_methods,
        );
        methods
    }

    /// Collects eval interface methods without duplicating inherited method names.
    fn collect_interface_method_requirements(
        &self,
        interface_name: &str,
        methods: &mut Vec<(String, EvalInterfaceMethod)>,
        seen_interfaces: &mut HashSet<String>,
        seen_methods: &mut HashSet<String>,
    ) {
        let key = normalize_class_name(interface_name);
        if !seen_interfaces.insert(key) {
            return;
        }
        let Some(interface) = self.interface(interface_name) else {
            return;
        };
        for parent in interface.parents() {
            self.collect_interface_method_requirements(
                parent,
                methods,
                seen_interfaces,
                seen_methods,
            );
        }
        for method in interface.methods() {
            let key = method.name().to_ascii_lowercase();
            if seen_methods.insert(key) {
                methods.push((interface.name().to_string(), method.clone()));
            }
        }
    }

    /// Returns direct and inherited property contracts for an eval interface.
    pub fn interface_property_requirements(
        &self,
        interface_name: &str,
    ) -> Vec<EvalInterfaceProperty> {
        let mut properties = Vec::new();
        let mut seen_interfaces = HashSet::new();
        self.collect_interface_property_requirements(
            interface_name,
            &mut properties,
            &mut seen_interfaces,
        );
        properties
    }

    /// Collects eval interface property contracts, merging duplicate inherited names.
    fn collect_interface_property_requirements(
        &self,
        interface_name: &str,
        properties: &mut Vec<EvalInterfaceProperty>,
        seen_interfaces: &mut HashSet<String>,
    ) {
        let key = normalize_class_name(interface_name);
        if !seen_interfaces.insert(key) {
            return;
        }
        let Some(interface) = self.interface(interface_name) else {
            return;
        };
        for parent in interface.parents() {
            self.collect_interface_property_requirements(parent, properties, seen_interfaces);
        }
        for property in interface.properties() {
            if let Some(existing) = properties
                .iter_mut()
                .find(|existing| existing.name() == property.name())
            {
                *existing = existing.merged_with(property);
            } else {
                properties.push(property.clone());
            }
        }
    }

    /// Returns whether an eval-declared class satisfies one class/interface target.
    pub fn class_is_a(&self, class_name: &str, target: &str, exclude_self: bool) -> bool {
        let Some(class) = self.class(class_name) else {
            return false;
        };
        let target = normalize_class_name(
            &self
                .resolve_class_like_name(target)
                .unwrap_or_else(|| target.trim_start_matches('\\').to_string()),
        );
        if !exclude_self && normalize_class_name(class.name()) == target {
            return true;
        }
        if target == normalize_class_name("Stringable")
            && self.class_has_valid_tostring(class.name())
        {
            return true;
        }
        self.class_parent_names(class.name())
            .iter()
            .any(|parent| normalize_class_name(parent) == target)
            || self
                .class_interface_names(class.name())
                .iter()
                .any(|interface| normalize_class_name(interface) == target)
    }

    /// Returns whether one eval class exposes a PHP-compatible `__toString()` method.
    fn class_has_valid_tostring(&self, class_name: &str) -> bool {
        self.class_method(class_name, "__toString")
            .is_some_and(|(_, method)| {
                method.visibility() == EvalVisibility::Public
                    && !method.is_static()
                    && !method.is_abstract()
                    && method.params().is_empty()
            })
    }

    /// Defines an eval dynamic constant value, failing if the name is invalid or already present.
    pub fn define_constant(&mut self, name: &str, value: RuntimeCellHandle) -> bool {
        let key = normalize_constant_name(name);
        if key.is_empty() || self.constants.contains_key(&key) {
            return false;
        }
        self.constants.insert(key, value);
        true
    }

    /// Returns true when this eval context has a dynamic constant with the requested name.
    pub fn has_constant(&self, name: &str) -> bool {
        self.constants.contains_key(&normalize_constant_name(name))
    }

    /// Returns an eval dynamic constant value by case-sensitive PHP constant name.
    pub fn constant(&self, name: &str) -> Option<RuntimeCellHandle> {
        self.constants.get(&normalize_constant_name(name)).copied()
    }

    /// Defines a dynamic user function, failing if the name already exists.
    pub fn define_function(
        &mut self,
        name: impl Into<String>,
        function: EvalFunction,
    ) -> Result<(), EvalFunction> {
        let name = name.into();
        if self.functions.contains_key(&name) || self.native_functions.contains_key(&name) {
            return Err(function);
        }
        self.functions.insert(name, function);
        Ok(())
    }

    /// Defines a generated native function callback, failing if the name already exists.
    pub fn define_native_function(
        &mut self,
        name: impl Into<String>,
        function: NativeFunction,
    ) -> Result<(), NativeFunction> {
        let name = name.into();
        if self.functions.contains_key(&name) || self.native_functions.contains_key(&name) {
            return Err(function);
        }
        self.native_functions.insert(name, function);
        Ok(())
    }

    /// Returns a dynamic user function by its lowercase PHP function name.
    pub fn function(&self, name: &str) -> Option<&EvalFunction> {
        self.functions.get(name)
    }

    /// Returns a native AOT function callback by its lowercase PHP function name.
    pub fn native_function(&self, name: &str) -> Option<NativeFunction> {
        self.native_functions.get(name).cloned()
    }

    /// Records one parameter name for an already registered native AOT callback.
    pub fn define_native_function_param(
        &mut self,
        function_name: &str,
        index: usize,
        param_name: impl Into<String>,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| function.set_param_name(index, param_name))
    }

    /// Defines native AOT instance-method signature metadata for eval named-argument binding.
    pub fn define_native_method_signature(
        &mut self,
        class_name: &str,
        method_name: &str,
        signature: NativeCallableSignature,
    ) -> bool {
        self.native_methods
            .insert(native_method_key(class_name, method_name), signature)
            .is_none()
    }

    /// Defines native AOT static-method signature metadata for eval named-argument binding.
    pub fn define_native_static_method_signature(
        &mut self,
        class_name: &str,
        method_name: &str,
        signature: NativeCallableSignature,
    ) -> bool {
        self.native_static_methods
            .insert(native_method_key(class_name, method_name), signature)
            .is_none()
    }

    /// Defines native AOT constructor signature metadata for eval named-argument binding.
    pub fn define_native_constructor_signature(
        &mut self,
        class_name: &str,
        signature: NativeCallableSignature,
    ) -> bool {
        self.native_constructors
            .insert(normalize_class_name(class_name), signature)
            .is_none()
    }

    /// Records one parameter name for registered native AOT instance-method metadata.
    pub fn define_native_method_param(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        param_name: impl Into<String>,
    ) -> bool {
        self.native_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_name(index, param_name))
    }

    /// Records one parameter name for registered native AOT static-method metadata.
    pub fn define_native_static_method_param(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        param_name: impl Into<String>,
    ) -> bool {
        self.native_static_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_name(index, param_name))
    }

    /// Records one parameter name for registered native AOT constructor metadata.
    pub fn define_native_constructor_param(
        &mut self,
        class_name: &str,
        index: usize,
        param_name: impl Into<String>,
    ) -> bool {
        self.native_constructors
            .get_mut(&normalize_class_name(class_name))
            .is_some_and(|signature| signature.set_param_name(index, param_name))
    }

    /// Returns native AOT instance-method signature metadata by PHP class and method name.
    pub fn native_method_signature(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<NativeCallableSignature> {
        self.native_methods
            .get(&native_method_key(class_name, method_name))
            .cloned()
    }

    /// Returns native AOT static-method signature metadata by PHP class and method name.
    pub fn native_static_method_signature(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<NativeCallableSignature> {
        self.native_static_methods
            .get(&native_method_key(class_name, method_name))
            .cloned()
    }

    /// Returns native AOT constructor signature metadata by PHP class name.
    pub fn native_constructor_signature(
        &self,
        class_name: &str,
    ) -> Option<NativeCallableSignature> {
        self.native_constructors
            .get(&normalize_class_name(class_name))
            .cloned()
    }

    /// Returns true when the context has a dynamic or native function with this lowercase PHP name.
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name) || self.native_functions.contains_key(name)
    }

    /// Returns a stored static local cell for an eval-declared function.
    pub fn static_local(&self, function_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.static_locals
            .get(&(function_name.to_string(), name.to_string()))
            .copied()
    }

    /// Stores one static local cell and returns any replaced distinct cell.
    pub fn set_static_local(
        &mut self,
        function_name: impl Into<String>,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .static_locals
            .insert((function_name.into(), name.into()), cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Returns a stored static property cell for an eval-declared class.
    pub fn static_property(&self, class_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.static_properties
            .get(&(normalize_class_name(class_name), name.to_string()))
            .copied()
    }

    /// Stores one eval static property cell and returns any replaced distinct cell.
    pub fn set_static_property(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .static_properties
            .insert((normalize_class_name(class_name), name.into()), cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Returns a materialized eval class constant cell.
    pub fn class_constant_cell(&self, class_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.class_constants
            .get(&(normalize_class_name(class_name), name.to_string()))
            .copied()
    }

    /// Stores one eval class constant cell and returns any replaced distinct cell.
    pub fn set_class_constant_cell(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .class_constants
            .insert((normalize_class_name(class_name), name.into()), cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Returns true when an eval include key was already loaded by this context.
    pub fn has_included_file(&self, path: &str) -> bool {
        self.included_files.contains(path)
    }

    /// Records one successfully loaded eval include key for include_once/require_once.
    pub fn mark_included_file(&mut self, path: impl Into<String>) {
        self.included_files.insert(path.into());
    }

    /// Stores the non-owned global scope handle used by eval `global` aliases.
    pub fn set_global_scope(&mut self, scope: *mut ElephcEvalScope) -> bool {
        if scope.is_null() {
            self.global_scope = None;
            false
        } else {
            self.global_scope = Some(scope);
            true
        }
    }

    /// Returns the non-owned global scope handle for eval `global` aliases.
    pub fn global_scope_ptr(&self) -> Option<*mut ElephcEvalScope> {
        self.global_scope
    }

    /// Pushes an eval-executed function name for magic-constant resolution.
    pub fn push_function(&mut self, name: impl Into<String>) {
        self.function_stack.push(name.into());
    }

    /// Pops the current eval-executed function name after its body completes.
    pub fn pop_function(&mut self) {
        self.function_stack.pop();
    }

    /// Returns the current eval-executed function name, if execution is inside one.
    pub fn current_function(&self) -> Option<&str> {
        self.function_stack.last().map(String::as_str)
    }

    /// Pushes the eval class whose method is currently executing.
    pub fn push_class_scope(&mut self, name: impl Into<String>) {
        self.class_stack.push(name.into());
    }

    /// Pops the current eval class method scope.
    pub fn pop_class_scope(&mut self) {
        self.class_stack.pop();
    }

    /// Returns the current eval class scope, if execution is inside a method.
    pub fn current_class_scope(&self) -> Option<&str> {
        self.class_stack.last().map(String::as_str)
    }

    /// Pushes the class name used to dispatch the current eval method call.
    pub fn push_called_class_scope(&mut self, name: impl Into<String>) {
        self.called_class_stack.push(name.into());
    }

    /// Pops the current late-static-bound eval class scope.
    pub fn pop_called_class_scope(&mut self) {
        self.called_class_stack.pop();
    }

    /// Returns the current late-static-bound eval class scope, if execution is inside a method.
    pub fn current_called_class_scope(&self) -> Option<&str> {
        self.called_class_stack.last().map(String::as_str)
    }

    /// Captures the current eval execution stacks for later caller-context-sensitive work.
    pub fn execution_scope(&self) -> ElephcEvalExecutionScope {
        ElephcEvalExecutionScope {
            function_stack: self.function_stack.clone(),
            class_stack: self.class_stack.clone(),
            called_class_stack: self.called_class_stack.clone(),
        }
    }

    /// Replaces eval execution stacks and returns the previous stacks for restoration.
    pub fn replace_execution_scope(
        &mut self,
        scope: ElephcEvalExecutionScope,
    ) -> ElephcEvalExecutionScope {
        ElephcEvalExecutionScope {
            function_stack: std::mem::replace(&mut self.function_stack, scope.function_stack),
            class_stack: std::mem::replace(&mut self.class_stack, scope.class_stack),
            called_class_stack: std::mem::replace(
                &mut self.called_class_stack,
                scope.called_class_stack,
            ),
        }
    }

    /// Records a Throwable cell that escaped from an eval-executed function call.
    pub fn set_pending_throw(&mut self, value: RuntimeCellHandle) {
        self.pending_throw = Some(value);
    }

    /// Returns and clears the Throwable cell currently escaping through eval.
    pub fn take_pending_throw(&mut self) -> Option<RuntimeCellHandle> {
        self.pending_throw.take()
    }

    /// Returns the eval-local SPL autoload extension list.
    pub fn spl_autoload_extensions(&self) -> &str {
        &self.spl_autoload_extensions
    }

    /// Replaces the eval-local SPL autoload extension list.
    pub fn set_spl_autoload_extensions(&mut self, extensions: impl Into<String>) {
        self.spl_autoload_extensions = extensions.into();
    }

    /// Returns the eval-local stream resource table.
    pub(crate) fn stream_resources(&self) -> &EvalStreamResources {
        &self.streams
    }

    /// Returns mutable access to the eval-local stream resource table.
    pub(crate) fn stream_resources_mut(&mut self) -> &mut EvalStreamResources {
        &mut self.streams
    }

    /// Clears the eval-local JSON error state after a successful JSON operation.
    pub fn clear_json_error(&mut self) {
        self.json_last_error = 0;
        self.json_last_error_msg.clear();
        self.json_last_error_msg.push_str("No error");
    }

    /// Records the eval-local JSON error state for `json_last_error*()` calls.
    pub fn set_json_error(&mut self, code: i64, message: impl Into<String>) {
        self.json_last_error = code;
        self.json_last_error_msg = message.into();
    }

    /// Returns the PHP `JSON_ERROR_*` code for the last eval JSON operation.
    pub const fn json_last_error(&self) -> i64 {
        self.json_last_error
    }

    /// Returns the PHP message for the last eval JSON operation.
    pub fn json_last_error_msg(&self) -> &str {
        &self.json_last_error_msg
    }

    /// Updates the source file, directory, and line for the current eval call site.
    pub fn set_call_site(&mut self, file: impl Into<String>, dir: impl Into<String>, line: i64) {
        self.call_file = file.into();
        self.call_dir = dir.into();
        self.call_line = line;
        self.file_magic_override = None;
    }

    /// Returns a copy of the current call-site metadata for temporary overrides.
    pub fn call_site(&self) -> (String, String, i64, Option<String>) {
        (
            self.call_file.clone(),
            self.call_dir.clone(),
            self.call_line,
            self.file_magic_override.clone(),
        )
    }

    /// Overrides `__FILE__` while executing an actual file through eval include.
    pub fn set_file_magic_override(&mut self, file: Option<String>) {
        self.file_magic_override = file;
    }

    /// Returns the source directory associated with the current eval call site.
    pub fn call_dir(&self) -> &str {
        &self.call_dir
    }

    /// Returns PHP's `__FILE__` string for code currently running inside eval.
    pub fn eval_file_magic(&self) -> String {
        if let Some(file) = &self.file_magic_override {
            return file.clone();
        }
        if self.call_file.is_empty() {
            return String::new();
        }
        format!("{}({}) : eval()'d code", self.call_file, self.call_line)
    }
}

impl Default for ElephcEvalContext {
    /// Creates the default process-level eval context.
    fn default() -> Self {
        Self::new()
    }
}

/// Normalizes PHP class names for the eval dynamic class registry.
fn normalize_class_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

/// Normalizes PHP enum case names for case-sensitive eval enum lookup.
fn normalize_enum_case_name(name: &str) -> String {
    name.to_string()
}

/// Normalizes PHP method names for case-insensitive native metadata lookup.
fn normalize_method_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Builds the folded native method metadata key used for eval argument binding.
fn native_method_key(class_name: &str, method_name: &str) -> (String, String) {
    (
        normalize_class_name(class_name),
        normalize_method_name(method_name),
    )
}

/// Pushes a PHP class-like name once, preserving the first visible spelling.
fn push_unique_class_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    let key = normalize_class_name(name);
    if seen.insert(key) {
        names.push(name.trim_start_matches('\\').to_string());
    }
}

/// Returns whether two PHP class-like names resolve to the same normalized spelling.
fn same_class_name(left: &str, right: &str) -> bool {
    normalize_class_name(left) == normalize_class_name(right)
}

/// Pushes a case-insensitive PHP method name once for ReflectionClass metadata.
fn push_unique_method_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    let key = normalize_method_name(name);
    if seen.insert(key) {
        names.push(name.trim_start_matches('\\').to_string());
    }
}

/// Pushes a case-sensitive PHP property name once for ReflectionClass metadata.
fn push_unique_property_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    if seen.insert(name.to_string()) {
        names.push(name.to_string());
    }
}

/// Pushes a case-sensitive PHP class constant name once for ReflectionClass metadata.
fn push_unique_constant_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    if seen.insert(name.to_string()) {
        names.push(name.to_string());
    }
}

/// Normalizes PHP constant names for case-sensitive eval dynamic probes.
fn normalize_constant_name(name: &str) -> String {
    name.trim_start_matches('\\').to_string()
}
