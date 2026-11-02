//! Purpose:
//! Defines the top-level EIR module, data pool, extern declarations, and
//! metadata tables needed by later lowering/codegen phases.
//!
//! Called from:
//! - Future AST-to-EIR lowering and the EIR-to-ASM backend.
//!
//! Key details:
//! - Runtime helper bodies remain outside EIR; modules reference runtime
//!   features and metadata needed to select/link helpers.

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Target;
use crate::codegen::RuntimeFeatures;
use crate::ir::function::{Function, FunctionId};
use crate::ir::types::IrType;
use crate::parser::ast::{Expr, ExprKind, Visibility};
use crate::types::{
    ClassInfo, EnumInfo, ExternAbiType, ExternClassInfo, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType,
};

/// Data-pool identifier shared by string, float, and name tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataId(u32);

impl DataId {
    /// Creates a data identifier from its raw zero-based table index.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw zero-based table index represented by this identifier.
    pub fn as_raw(self) -> u32 {
        self.0
    }
}

/// Scalar value materialized into the closed-world constant registry.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstScalar {
    /// Integer constant.
    Int(i64),
    /// Floating-point constant.
    Float(f64),
    /// Boolean constant.
    Bool(bool),
    /// String constant.
    Str(String),
    /// Null constant.
    Null,
}

/// Method metadata retained for standalone trait reflection.
#[derive(Debug, Clone)]
pub struct TraitMethodInfo {
    /// The method's spelling as declared in the trait, for reflected metadata.
    ///
    /// Lookup is keyed case-insensitively, but `ReflectionMethod::getName()` must report the
    /// declaration rather than the lookup key (issue #571), and a trait's declarations are not
    /// reachable from the using class's `method_decls`.
    pub declared_name: String,
    /// Position in the trait's declaration, independent of hash-map iteration order.
    pub declaration_order: usize,
    pub signature: FunctionSig,
    /// Source defaults preserve lexical constant names after semantic defaults are normalized.
    pub source_defaults: Vec<Option<Expr>>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_final: bool,
    pub is_abstract: bool,
}

/// One generated PHP 8.5 `clone()` property-override applicator.
///
/// Each entry names the synthetic EIR function that applies an override array to a clone of
/// exactly one runtime class, resolved from one profile of PHP invocation scopes. Backend clone
/// lowering dispatches on the clone's runtime class id and the invocation-site scope id and then
/// calls `function_name` with `(clone, overrides)`.
#[derive(Debug, Clone)]
pub struct CloneOverrideApplicator {
    /// Dense runtime class id of the cloned object this applicator serves.
    pub class_id: u64,
    /// PHP class name matching `class_id`, used for receiver typing in backend call staging.
    pub class_name: String,
    /// Dense class ids of the invocation scopes this applicator body is exact for.
    ///
    /// Several scopes share one body whenever they resolve every override name identically.
    pub scope_class_ids: Vec<u64>,
    /// Whether this body also serves global scope and every scope unrelated to `class_name`.
    pub is_default_scope: bool,
    /// EIR function name of the applicator body.
    pub function_name: String,
}

/// One native closure entry specialized for an explicit binding visibility scope.
#[derive(Debug, Clone)]
pub struct ClosureScopeVariant {
    /// Requested runtime class id, or global visibility when absent.
    pub scope_class_id: Option<u32>,
    /// Original and previously rebound entries with the same capture layout and body.
    pub source_names: Vec<String>,
    /// Function entry compiled with the requested lexical scope.
    pub function_name: String,
    /// Number of trailing capture parameters in the callable descriptor.
    pub capture_count: usize,
}

/// Complete EIR module for one compile target.
#[derive(Debug, Clone)]
pub struct Module {
    pub target: Target,
    pub source_path: Option<String>,
    /// Immutable inputs for compiled source entries, not runtime inclusion state.
    source_catalog: Option<std::sync::Arc<super::SourceCatalog>>,
    /// Catalog sources the COMPILER already included, so the runtime must not include them again.
    ///
    /// The autoload pass performs, at compile time, the inclusions PHP's autoloader would have
    /// performed at runtime: it opens the class file and splices its declarations into the
    /// program. Those declarations exist from process start, so a later `include_once` of the
    /// same path has nothing left to do -- and everything to break, because re-running the file
    /// redeclares a symbol that is already there. This is the file set that answers "already
    /// included" at runtime; a statically-resolved `include` is deliberately NOT in it, because
    /// its body is emitted inline and its guard is set when that code actually runs.
    pub preincluded_sources: std::collections::BTreeSet<std::path::PathBuf>,
    /// Files the closed world compiled as callable units (`opcache_compile_file()` in the preload
    /// script), so a runtime `include` of one calls compiled code instead of interpreting it.
    pub include_units: Vec<IncludeUnit>,
    /// The build links no interpreter (`--closed-world`): a runtime `include`/`require` is
    /// answered from the compiled-source table (`__rt_closed_include`) and never reaches the
    /// eval bridge.
    pub closed_world: bool,
    /// What a runtime include of each compiled source does in a closed-world build; `None` when
    /// the program has no runtime include (or is not closed-world). One record per catalog source.
    pub closed_includes: Option<Vec<ClosedInclude>>,
    /// LOWERCASE names of compiled classes that must still answer `class_exists($n, false)` false.
    ///
    /// A closed-world build declares everything it compiled from the first instruction. For a
    /// class the autoload pass pulled in ONLY so an existence probe could be answered, that is
    /// not what php does: php loads a class when something uses it, and `$autoload = false` asks
    /// precisely whether that has happened yet. Each name here gets a request-scoped flag that
    /// starts clear and is raised by the first probe that allows autoloading.
    pub deferred_class_loads: std::collections::BTreeSet<String>,
    /// Public function names whose implementation is selected during execution.
    pub(crate) runtime_bound_functions: std::sync::Arc<std::collections::HashSet<String>>,
    /// Lowercase bare names of functions and methods whose by-reference parameter a closure
    /// binds by reference, so an omitted argument's cell must outlive the call
    /// (`RefArgCellLifetime::MayOutliveCall`). Matched by name alone: over-matching only costs a
    /// heap cell where a stack one would have done.
    pub(crate) ref_param_escaping_callees: std::collections::HashSet<String>,
    /// Names some `define()` call creates: `defined()` reads their `_define_seen` flag, and
    /// `__rt_web_reset` clears it so each request starts with them undefined, as php does.
    pub(crate) runtime_define_names: std::collections::BTreeSet<String>,
    /// Optional mbstring startup arguments: effective core encodings followed by raw INI pairs.
    /// These belong to user assembly, never the reusable runtime object.
    pub mbstring_startup: Option<Vec<Vec<u8>>>,
    /// Canonical PHP source paths compiled into this binary, in include order.
    pub included_files: Vec<String>,
    /// `--probe` build key, embedded as `_elephc_probe_key` so the probe endpoint
    /// can prove the binary's identity through the HMAC handshake. `None` unless
    /// `--probe` is set.
    pub probe_key: Option<[u8; 32]>,
    pub functions: Vec<Function>,
    pub class_methods: Vec<Function>,
    pub closures: Vec<Function>,
    /// Entry remapping for explicit-scope binds whose source closure is selected at runtime.
    pub closure_scope_variants: Vec<ClosureScopeVariant>,
    pub fiber_wrappers: Vec<Function>,
    pub callback_wrappers: Vec<Function>,
    pub extern_callback_trampolines: Vec<Function>,
    pub runtime_callable_invokers: Vec<Function>,
    pub data: DataPool,
    pub extern_decls: Vec<ExternDecl>,
    pub callable_param_sigs: HashMap<(String, String), FunctionSig>,
    pub class_table: ClassTable,
    pub enum_table: EnumTable,
    pub interface_table: InterfaceTable,
    pub trait_table: TraitTable,
    pub declared_class_names: Vec<String>,
    pub declared_interface_names: Vec<String>,
    pub declared_trait_names: Vec<String>,
    pub declared_trait_source_lines: HashMap<String, u32>,
    pub declared_function_source_lines: HashMap<String, (u32, u32)>,
    pub declared_class_source_files: HashMap<String, String>,
    pub declared_function_source_files: HashMap<String, String>,
    pub declared_trait_uses: HashMap<String, Vec<String>>,
    pub declared_trait_method_names: HashMap<String, Vec<String>>,
    /// Class methods imported from traits, including aliases, before lexical provenance is lost.
    pub imported_trait_method_names: HashMap<String, HashSet<String>>,
    pub declared_trait_methods: HashMap<String, HashMap<String, TraitMethodInfo>>,
    pub declared_trait_property_names: HashMap<String, Vec<String>>,
    pub declared_trait_properties: HashMap<String, Vec<crate::parser::ast::ClassProperty>>,
    pub declared_trait_constant_names: HashMap<String, Vec<String>>,
    pub declared_trait_constants: HashMap<String, HashMap<String, crate::parser::ast::Expr>>,
    pub declared_trait_constant_types:
        HashMap<String, HashMap<String, crate::parser::ast::TypeExpr>>,
    pub declared_trait_constant_visibilities: HashMap<String, HashMap<String, Visibility>>,
    pub declared_trait_final_constants: HashMap<String, HashSet<String>>,
    /// Prescanned global constant values used by EIR lowering and eval metadata registration.
    pub global_constants: HashMap<String, (ExprKind, PhpType)>,
    /// User-declared global constant names, separated from the seeded builtin inventory.
    pub user_defined_constants: Vec<String>,
    /// Path of every included or autoloaded file, by the source identity its spans carry
    /// (`Span::source_id`). Identity 0 is the entry file, `source_path`.
    pub source_files: std::collections::BTreeMap<u32, String>,
    pub class_infos: crate::fast_hash::FastMap<String, ClassInfo>,
    pub interface_infos: HashMap<String, InterfaceInfo>,
    pub enum_infos: HashMap<String, EnumInfo>,
    pub extern_class_infos: HashMap<String, ExternClassInfo>,
    pub packed_class_infos: HashMap<String, PackedClassInfo>,
    pub packed_layouts: PackedLayoutTable,
    pub extern_globals: HashMap<String, PhpType>,
    pub required_runtime_features: RuntimeFeatures,
    /// True when this module is being lowered for a `--web` compile. Threaded
    /// down from the CLI flag (`CliConfig.web`, mirroring what
    /// `codegen_ir::block_emit::emit_module` receives) so lowering can gate
    /// request-superglobal (`$_SERVER`/`$_SESSION`/…) type seeding: only
    /// `--web` builds pre-initialize the shared `_eir_global_*` storage for
    /// those names, so a non-web read/write must not assume a live Hash
    /// pointer is already there.
    pub web: bool,
    /// The web entry bootstraps a retained application after fork rather than per request.
    pub web_application: bool,
    /// Generated `clone()` property-override applicators, sorted by class id then scope id.
    pub clone_override_applicators: Vec<CloneOverrideApplicator>,
    /// Lazily built `(implementing class, method key, is_static) -> index into class_methods`,
    /// tagged with the `class_methods` length it was built for (see [`Module::class_method`]).
    class_method_lookup: ClassMethodIndex,
    /// Lazily numbered attribute factories, tagged with the `(class_infos, functions)` lengths
    /// they were numbered from (see [`Module::attribute_factory_ids`]).
    attribute_factory_ids:
        std::sync::OnceLock<((usize, usize), crate::codegen::reflection::AttributeFactoryIds)>,
    /// Whole-module answers codegen asked per call site; see [`Module::cached_fact`].
    fact_cache: FactCache,
}

/// Module-wide facts codegen derives from the whole program and used to recompute per call site
/// (each scanned every function body, or all 37k data strings, per question). Each is tagged with
/// the module shape it was computed for and recomputed if the module grew since.
#[derive(Debug, Clone, Default)]
struct FactCache {
    variant_groups: FactCell<crate::fast_hash::FastMap<String, String>>,
    interface_activation_events: FactCell<bool>,
    extern_function_keys: FactCell<crate::fast_hash::FastSet<String>>,
    classes_by_method:
        FactCell<crate::fast_hash::FastMap<String, Vec<String>>>,
    strict_descendants:
        FactCell<crate::fast_hash::FastMap<String, Vec<String>>>,
    name_addressable_hash_classes: FactCell<Vec<String>>,
    dispatch_groups:
        FactCell<Vec<crate::ir::function_variants::FunctionVariantLabel>>,
    function_indexes_by_key: FactCell<crate::fast_hash::FastMap<String, usize>>,
    functions_or_closures_by_key:
        FactCell<crate::fast_hash::FastMap<String, usize>>,
    shared_ref_cell_globals: FactCell<crate::fast_hash::FastSet<String>>,
    runtime_referenced_class_names: FactCell<HashSet<String>>,
    class_names_by_key: FactCell<crate::fast_hash::FastMap<String, String>>,
    interface_names_by_key:
        FactCell<crate::fast_hash::FastMap<String, String>>,
    callable_defaults_by_span:
        FactCell<crate::fast_hash::FastMap<crate::span::Span, Vec<usize>>>,
    backtrace_frames: FactCell<bool>,
    materialized_backtrace: FactCell<bool>,
}

/// Folded name -> the first name, in iteration order, that folds to it.
fn folded_name_index<'a>(names: impl Iterator<Item = &'a String>) -> crate::fast_hash::FastMap<String, String> {
    let mut index = crate::fast_hash::FastMap::default();
    for name in names {
        index
            .entry(crate::names::php_symbol_key(name.trim_start_matches('\\')))
            .or_insert_with(|| name.clone());
    }
    index
}

/// One cached whole-module fact and the module shape it was computed for.
///
/// Replaceable, not a `OnceLock`: a module copy that grows (runtime call wrappers intern data
/// into theirs) must be able to store the newer answer. With a `OnceLock` the first answer
/// stuck and every later question on the grown copy rebuilt the fact from scratch.
#[derive(Debug, Default)]
struct FactCell<T>(std::sync::RwLock<Option<(ModuleShape, std::sync::Arc<T>)>>);

impl<T> Clone for FactCell<T> {
    fn clone(&self) -> Self {
        Self(std::sync::RwLock::new(self.0.read().map(|cell| cell.clone()).unwrap_or(None)))
    }
}

/// The sizes of the module tables a cached whole-module fact is derived from.
type ModuleShape = [usize; 12];

/// `class_methods` indexed by `(implementing class, lowercased method name, is_static)`, covering
/// the first `len` bodies and extended as the table grows.
///
/// Extended, not rebuilt or bypassed: lowering appends bodies while it asks (the SPL discovery
/// fixpoint asks whether each candidate is lowered yet), and an index frozen at its first size
/// left every later question to a scan of the whole table.
#[derive(Debug, Default)]
struct ClassMethodIndex(std::sync::RwLock<(usize, ClassMethodLookup)>);

impl Clone for ClassMethodIndex {
    fn clone(&self) -> Self {
        Self(std::sync::RwLock::new(
            self.0.read().map(|index| index.clone()).unwrap_or_default(),
        ))
    }
}

impl ClassMethodIndex {
    /// The slot of the first body of `methods` matching the key, extending the index first.
    fn index(&self, methods: &[Function], impl_class: &str, method_key: &str, is_static: bool) -> Option<usize> {
        let key = (impl_class.to_string(), method_key.to_string(), is_static);
        if let Ok(index) = self.0.read() {
            if index.0 == methods.len() {
                return index.1.get(&key).copied();
            }
        }
        let mut index = self.0.write().ok()?;
        // A table that shrank (bodies removed) is re-indexed from the start.
        if index.0 > methods.len() {
            *index = (0, ClassMethodLookup::default());
        }
        let (covered, lookup) = &mut *index;
        for (slot, function) in methods.iter().enumerate().skip(*covered) {
            if let Some((class, method)) = function.name.rsplit_once("::") {
                lookup
                    .entry((class.to_string(), crate::names::php_symbol_key(method), function.flags.is_static))
                    .or_insert(slot);
            }
        }
        *covered = methods.len();
        lookup.get(&key).copied()
    }
}

/// Index from `(implementing class, lowercased method name, is_static)` to a `class_methods` slot.
type ClassMethodLookup = crate::fast_hash::FastMap<(String, String, bool), usize>;

/// How a closed-world build answers a runtime include of one compiled source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosedIncludeKind {
    /// The file is declarations the binary already carries: including it runs nothing and
    /// evaluates to `1` (`true` for an `_once` form that finds it already included).
    Declarations,
    /// A unit function (the PHP name) runs the file's top level and returns its value.
    Unit(String),
    /// The file is part of the closed world but its top level has effects the binary cannot
    /// replay from an include (the compiler ran it at its static include site): a runtime
    /// include that needs it to run again is a fatal error naming the file.
    NotCompiled,
}

/// One compiled source and what a runtime include of its canonical path does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedInclude {
    pub canonical_path: std::path::PathBuf,
    pub kind: ClosedIncludeKind,
}

/// One file the compiler turned into a function, and the path a runtime include reaches it by.
///
/// php's `opcache_compile_file()` compiles a file without running it, and a later `include` runs
/// the cached code. The compiled equivalent is a function whose body is the file's top level:
/// `canonical_path` is what a runtime include resolves to, `function` the PHP name of that
/// function, which returns what the include evaluates to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeUnit {
    pub canonical_path: std::path::PathBuf,
    pub function: String,
}

impl Module {
    /// Creates an empty module for the given target.
    pub fn new(target: Target) -> Self {
        Self {
            source_catalog: None,
            preincluded_sources: Default::default(),
            include_units: Vec::new(),
            closed_world: false,
            closed_includes: None,
            deferred_class_loads: Default::default(),
            runtime_bound_functions: Default::default(),
            ref_param_escaping_callees: Default::default(),
            runtime_define_names: Default::default(),
            target,
            source_path: None,
            mbstring_startup: None,
            included_files: Vec::new(),
            probe_key: None,
            functions: Vec::new(),
            class_methods: Vec::new(),
            closures: Vec::new(),
            closure_scope_variants: Vec::new(),
            fiber_wrappers: Vec::new(),
            callback_wrappers: Vec::new(),
            extern_callback_trampolines: Vec::new(),
            runtime_callable_invokers: Vec::new(),
            data: DataPool::default(),
            extern_decls: Vec::new(),
            callable_param_sigs: HashMap::new(),
            class_table: ClassTable::default(),
            enum_table: EnumTable::default(),
            interface_table: InterfaceTable::default(),
            trait_table: TraitTable::default(),
            declared_class_names: Vec::new(),
            declared_interface_names: Vec::new(),
            declared_trait_names: Vec::new(),
            declared_trait_source_lines: HashMap::new(),
            declared_function_source_lines: HashMap::new(),
            declared_class_source_files: HashMap::new(),
            declared_function_source_files: HashMap::new(),
            declared_trait_uses: HashMap::new(),
            declared_trait_method_names: HashMap::new(),
            imported_trait_method_names: HashMap::new(),
            declared_trait_methods: HashMap::new(),
            declared_trait_property_names: HashMap::new(),
            declared_trait_properties: HashMap::new(),
            declared_trait_constant_names: HashMap::new(),
            declared_trait_constants: HashMap::new(),
            declared_trait_constant_types: HashMap::new(),
            declared_trait_constant_visibilities: HashMap::new(),
            declared_trait_final_constants: HashMap::new(),
            global_constants: HashMap::new(),
            class_infos: crate::fast_hash::FastMap::default(),
            user_defined_constants: Vec::new(),
            source_files: std::collections::BTreeMap::new(),
            interface_infos: HashMap::new(),
            enum_infos: HashMap::new(),
            extern_class_infos: HashMap::new(),
            packed_class_infos: HashMap::new(),
            packed_layouts: PackedLayoutTable::default(),
            extern_globals: HashMap::new(),
            required_runtime_features: RuntimeFeatures::none(),
            web: false,
            web_application: false,
            clone_override_applicators: Vec::new(),
            class_method_lookup: ClassMethodIndex::default(),
            attribute_factory_ids: std::sync::OnceLock::new(),
            fact_cache: FactCache::default(),
        }
    }

    /// The sizes of every table a cached whole-module fact reads.
    fn shape(&self) -> ModuleShape {
        [
            self.functions.len(),
            self.class_methods.len(),
            self.closures.len(),
            self.fiber_wrappers.len(),
            self.callback_wrappers.len(),
            self.runtime_callable_invokers.len(),
            self.interface_infos.len(),
            self.data.strings.len(),
            self.extern_decls.len(),
            self.class_infos.len(),
            // Read by `function_bodies()` and the backtrace facts; both are set by lowering
            // today, kept in the shape so a later writer cannot leave a cached fact stale.
            self.extern_callback_trampolines.len(),
            usize::from(self.required_runtime_features.eval_bridge),
        ]
    }

    /// Returns a cached whole-module fact, computing it on first use; a module that grew since
    /// gets a fresh answer instead of the stale one.
    fn cached_fact<T>(&self, cell: &FactCell<T>, build: impl Fn() -> T) -> std::sync::Arc<T> {
        let shape = self.shape();
        if let Ok(cached) = cell.0.read() {
            if let Some((built_for, value)) = cached.as_ref() {
                if *built_for == shape {
                    return std::sync::Arc::clone(value);
                }
            }
        }
        // Built outside the lock: a fact may ask another one.
        let value = std::sync::Arc::new(build());
        if let Ok(mut cached) = cell.0.write() {
            *cached = Some((shape, std::sync::Arc::clone(&value)));
        }
        value
    }

    /// Folded public name -> include-variant dispatcher group name (codegen's
    /// `function_variants::collect_dispatch_groups`, first group per name).
    pub(crate) fn variant_group_names(
        &self,
        build: impl Fn() -> crate::fast_hash::FastMap<String, String>,
    ) -> std::sync::Arc<crate::fast_hash::FastMap<String, String>> {
        self.cached_fact(&self.fact_cache.variant_groups, build)
    }

    /// The include-variant dispatcher groups (`function_variants::collect_dispatch_groups`).
    pub(crate) fn dispatch_groups(
        &self,
        build: impl Fn() -> Vec<crate::ir::function_variants::FunctionVariantLabel>,
    ) -> std::sync::Arc<Vec<crate::ir::function_variants::FunctionVariantLabel>> {
        self.cached_fact(&self.fact_cache.dispatch_groups, build)
    }

    /// Normalized function key -> the first `functions` index carrying it.
    pub(crate) fn function_indexes_by_key(
        &self,
        build: impl Fn() -> crate::fast_hash::FastMap<String, usize>,
    ) -> std::sync::Arc<crate::fast_hash::FastMap<String, usize>> {
        self.cached_fact(&self.fact_cache.function_indexes_by_key, build)
    }

    /// The first function, then closure, whose PHP name folds to `key` (`php_symbol_key` of the
    /// name without its leading `\\`): codegen asks per direct call.
    pub(crate) fn function_or_closure_by_key(&self, key: &str) -> Option<&crate::ir::Function> {
        let index = *self
            .cached_fact(&self.fact_cache.functions_or_closures_by_key, || {
                let mut indexes = crate::fast_hash::FastMap::default();
                for (index, function) in self.functions.iter().chain(self.closures.iter()).enumerate() {
                    indexes
                        .entry(crate::names::php_symbol_key(function.name.trim_start_matches('\\')))
                        .or_insert(index);
                }
                indexes
            })
            .get(key)?;
        self.functions.get(index).or_else(|| self.closures.get(index - self.functions.len()))
    }

    /// Functions and closures whose `callable_default` sits at `span`, in the
    /// `functions`-then-`closures` order a linear scan visits them: Reflection asks per reflected
    /// parameter default, and scanning every body per question dominated codegen on Symfony.
    pub(crate) fn callable_default_helpers_at(
        &self,
        span: crate::span::Span,
    ) -> impl Iterator<Item = &crate::ir::Function> {
        let index = self.cached_fact(&self.fact_cache.callable_defaults_by_span, || {
            let mut index: crate::fast_hash::FastMap<crate::span::Span, Vec<usize>> =
                crate::fast_hash::FastMap::default();
            for (position, function) in self.functions.iter().chain(self.closures.iter()).enumerate() {
                if let Some(default) = &function.callable_default {
                    index.entry(default.span).or_default().push(position);
                }
            }
            index
        });
        let positions = index.get(&span).cloned().unwrap_or_default();
        positions.into_iter().filter_map(move |position| {
            self.functions.get(position).or_else(|| self.closures.get(position - self.functions.len()))
        })
    }

    /// The `class_infos` entry whose name folds to `key` (`php_symbol_key` without the leading
    /// `\\`), the first one in iteration order as the linear scan this replaces found it.
    pub(crate) fn class_info_by_key(&self, key: &str) -> Option<(&str, &crate::types::ClassInfo)> {
        let name = self
            .cached_fact(&self.fact_cache.class_names_by_key, || {
                folded_name_index(self.class_infos.keys())
            })
            .get(key)
            .cloned()?;
        self.class_infos.get_key_value(&name).map(|(name, info)| (name.as_str(), info))
    }

    /// The `interface_infos` entry whose name folds to `key`, as [`Self::class_info_by_key`].
    pub(crate) fn interface_info_by_key(
        &self,
        key: &str,
    ) -> Option<(&str, &crate::types::InterfaceInfo)> {
        let name = self
            .cached_fact(&self.fact_cache.interface_names_by_key, || {
                folded_name_index(self.interface_infos.keys())
            })
            .get(key)
            .cloned()?;
        self.interface_infos.get_key_value(&name).map(|(name, info)| (name.as_str(), info))
    }

    /// The classes allocation, `instanceof` and static access can name at run time
    /// (`codegen::runtime_metadata::runtime_referenced_class_names`, asked by three emitters, each
    /// answer several walks of every body).
    pub(crate) fn runtime_referenced_class_names(
        &self,
        build: impl Fn() -> HashSet<String>,
    ) -> std::sync::Arc<HashSet<String>> {
        self.cached_fact(&self.fact_cache.runtime_referenced_class_names, build)
    }

    /// The global names some body binds through a shared reference cell
    /// (`superglobals::uses_shared_ref_cell`, asked once per superglobal).
    pub(crate) fn shared_ref_cell_globals(
        &self,
        build: impl Fn() -> crate::fast_hash::FastSet<String>,
    ) -> std::sync::Arc<crate::fast_hash::FastSet<String>> {
        self.cached_fact(&self.fact_cache.shared_ref_cell_globals, build)
    }

    /// Whether emitted frames publish backtrace activations (codegen's
    /// `frame::module_uses_backtrace`), asked per emitted body and per function context.
    pub(crate) fn backtrace_frames(&self, build: impl Fn() -> bool) -> bool {
        *self.cached_fact(&self.fact_cache.backtrace_frames, build)
    }

    /// Whether a body can materialize a `debug_backtrace()` array (codegen's
    /// `frame::module_materializes_backtrace`), asked per Throwable construction site.
    pub(crate) fn materialized_backtrace(&self, build: impl Fn() -> bool) -> bool {
        *self.cached_fact(&self.fact_cache.materialized_backtrace, build)
    }

    /// Whether any interface's visibility is selected by a request-time activation event.
    pub(crate) fn interface_activation_events(&self, build: impl Fn() -> bool) -> bool {
        *self.cached_fact(&self.fact_cache.interface_activation_events, build)
    }

    /// Instance-method key -> the classes whose `methods` table declares it, in `class_infos`
    /// iteration order: what a per-call-site scan of every class would visit, without the scan.
    pub(crate) fn classes_by_method(
        &self,
    ) -> std::sync::Arc<crate::fast_hash::FastMap<String, Vec<String>>> {
        self.cached_fact(&self.fact_cache.classes_by_method, || {
            let mut index: crate::fast_hash::FastMap<String, Vec<String>> = crate::fast_hash::FastMap::default();
            for (class_name, class_info) in &self.class_infos {
                for method_key in class_info.methods.keys() {
                    index.entry(method_key.clone()).or_default().push(class_name.clone());
                }
            }
            index
        })
    }

    /// Class name -> every OTHER class whose declared parent chain reaches it (exactly the
    /// classes `types::class_inherits_from(classes, candidate, name)` answers true for), in
    /// `class_infos` iteration order. Property lowering asked that of every class of the module
    /// per dynamic property access (a top codegen cost on Symfony).
    pub(crate) fn strict_descendants(
        &self,
    ) -> std::sync::Arc<crate::fast_hash::FastMap<String, Vec<String>>> {
        self.cached_fact(&self.fact_cache.strict_descendants, || {
            let mut index: crate::fast_hash::FastMap<String, Vec<String>> = crate::fast_hash::FastMap::default();
            for (candidate, info) in &self.class_infos {
                let mut current = info.parent.as_deref();
                let mut guard = 0usize;
                while let Some(ancestor) = current {
                    let descendants = index.entry(ancestor.to_string()).or_default();
                    if descendants.last() != Some(candidate) {
                        descendants.push(candidate.clone());
                    }
                    guard += 1;
                    if guard > self.class_infos.len() + 1 {
                        break;
                    }
                    current = self.class_infos.get(ancestor).and_then(|info| info.parent.as_deref());
                }
            }
            index
        })
    }

    /// The classes whose dynamic-property hash is addressable by name, in `class_infos` order:
    /// the only ones a `Mixed` property read can answer from a hash arm. Every such read used to
    /// ask that of every class of the module.
    pub(crate) fn name_addressable_hash_classes(&self) -> std::sync::Arc<Vec<String>> {
        self.cached_fact(&self.fact_cache.name_addressable_hash_classes, || {
            self.class_infos
                .iter()
                .filter(|(_, info)| info.dynamic_property_hash_is_name_addressable())
                .map(|(name, _)| name.clone())
                .collect()
        })
    }

    /// Folded names of every extern function declaration.
    pub(crate) fn extern_function_keys(&self) -> std::sync::Arc<crate::fast_hash::FastSet<String>> {
        self.cached_fact(&self.fact_cache.extern_function_keys, || {
            self.extern_decls
                .iter()
                .map(|function| crate::names::php_symbol_key(function.name.trim_start_matches('\\')))
                .collect()
        })
    }

    /// The program's `ReflectionAttribute` factory ids, numbered over every class and every
    /// lowered function exactly as the synthetic `newInstance()` body numbers them.
    ///
    /// Every reflected member with attributes asks for them; numbering per member walked the
    /// whole program each time. The table is built on first use; if classes or functions were
    /// added since, it is not trusted and a fresh one answers.
    pub(crate) fn attribute_factory_ids(
        &self,
    ) -> std::borrow::Cow<'_, crate::codegen::reflection::AttributeFactoryIds> {
        let shape = (self.class_infos.len(), self.functions.len());
        let number = || {
            let function_attrs =
                crate::codegen::reflection::function_attribute_metadata(&self.functions);
            crate::codegen::reflection::AttributeFactoryIds::new(
                &self.class_infos,
                &crate::codegen::reflection::borrow_attribute_metadata(&function_attrs),
            )
        };
        let (built_for, ids) = self.attribute_factory_ids.get_or_init(|| (shape, number()));
        if *built_for == shape {
            std::borrow::Cow::Borrowed(ids)
        } else {
            std::borrow::Cow::Owned(number())
        }
    }

    /// The lowered body implementing `impl_class::method_key` (method key already lowercased).
    ///
    /// Codegen asks this from every descriptor template; a linear scan of `class_methods` (47 k
    /// bodies on the Symfony demo, each split and lowercased per probe) dominated parallel
    /// codegen. The index is built on first use; if bodies were added since, it is not trusted
    /// and the scan answers.
    pub fn class_method(
        &self,
        impl_class: &str,
        method_key: &str,
        is_static: bool,
    ) -> Option<&Function> {
        let index = self.class_method_lookup.index(&self.class_methods, impl_class, method_key, is_static)?;
        self.class_methods.get(index)
    }

    /// Creates a module whose source identities are frozen before any body is lowered.
    pub fn with_source_catalog(target: Target, catalog: super::SourceCatalog) -> Self {
        let mut module = Self::new(target);
        module.source_catalog = Some(std::sync::Arc::new(catalog));
        module
    }

    /// Freezes the module's complete source catalog exactly once.
    pub fn bind_source_units(
        &mut self,
        units: impl IntoIterator<Item = crate::resolver::SourceUnit>,
    ) -> Result<(), crate::errors::CompileError> {
        if self.source_catalog.is_some() {
            return Err(crate::errors::CompileError::new(
                crate::span::Span::dummy(), "Source catalog is already bound",
            ));
        }
        self.source_catalog = Some(std::sync::Arc::new(super::SourceCatalog::from_units(units)?));
        Ok(())
    }

    /// Returns source metadata without exposing mutable ID assignment.
    pub fn source_catalog(&self) -> Option<&super::SourceCatalog> { self.source_catalog.as_deref() }

    /// Shares immutable lookup with a lowering body without copying source maps or text.
    pub(crate) fn shared_source_catalog(&self) -> Option<std::sync::Arc<super::SourceCatalog>> {
        self.source_catalog.clone()
    }

    /// Adds a user function and returns its module-local identifier.
    pub fn add_function(&mut self, mut function: Function) -> FunctionId {
        let id = FunctionId::from_raw(self.functions.len() as u32);
        function.set_id(id);
        self.functions.push(function);
        id
    }

    /// Adds a closure function and returns its closure-table identifier.
    pub fn add_closure(&mut self, mut function: Function) -> FunctionId {
        let id = FunctionId::from_raw(self.closures.len() as u32);
        function.set_id(id);
        self.closures.push(function);
        id
    }

    /// Returns whether any function-like body calls one typed PCNTL runtime operation.
    ///
    /// `pcntl_async_signals()` makes the backend dispatch pending signal handlers after every
    /// instruction, so optimizations that assume no user code runs between two instructions
    /// must check it module-wide.
    pub(crate) fn uses_pcntl_runtime(&self, target: crate::ir::PcntlRuntime) -> bool {
        self.functions
            .iter()
            .chain(self.class_methods.iter())
            .chain(self.closures.iter())
            .chain(self.fiber_wrappers.iter())
            .chain(self.callback_wrappers.iter())
            .chain(self.extern_callback_trampolines.iter())
            .chain(self.runtime_callable_invokers.iter())
            .any(|function| {
                function.instructions.iter().any(|inst| {
                    matches!(
                        inst.immediate,
                        Some(crate::ir::Immediate::RuntimeCall(
                            crate::ir::RuntimeCallTarget::Pcntl(found)
                        )) if found == target
                    )
                })
            })
    }

    /// Iterates every function-like body: functions, methods, closures and generated wrappers.
    fn function_bodies(&self) -> impl Iterator<Item = &Function> + '_ {
        self.functions
            .iter()
            .chain(self.class_methods.iter())
            .chain(self.closures.iter())
            .chain(self.fiber_wrappers.iter())
            .chain(self.callback_wrappers.iter())
            .chain(self.extern_callback_trampolines.iter())
            .chain(self.runtime_callable_invokers.iter())
    }

    /// Returns whether any emitted EIR body can request a PHP Core backtrace.
    ///
    /// Codegen publishes per-frame backtrace activations only when this holds. It is
    /// deliberately conservative: a frame that only collects surplus arguments for
    /// `func_get_args()` (the core prelude's `ErrorException::__construct` does) counts too.
    pub(crate) fn uses_php_backtrace(&self) -> bool {
        self.calls_php_backtrace()
            || self
                .function_bodies()
                .any(Function::keeps_backtrace_argument_snapshot)
    }

    /// Returns whether a backtrace or generated diagnostic observes the physical PHP call chain.
    ///
    /// A backtrace requested any other way (a `debug_backtrace` callable string, a descriptor)
    /// makes the argument-introspection pass give EVERY PHP frame an argument snapshot, which
    /// `Function::keeps_backtrace_argument_snapshot` reports per body. The inliner combines the
    /// two: it keeps every method call when this holds, and never splices a body that keeps a
    /// snapshot, so a method frame a backtrace could list always stays a real frame.
    pub(crate) fn calls_php_backtrace(&self) -> bool {
        if self.required_runtime_features.eval_bridge {
            return true;
        }
        self.function_bodies()
            .flat_map(|function| function.instructions.iter())
            .any(|inst| {
                inst.op == crate::ir::Op::CoreBuiltin
                    && matches!(
                        inst.immediate,
                        Some(crate::ir::Immediate::I64(selector))
                            if matches!(
                                crate::ir::CoreBuiltinOp::from_i64(selector),
                                Some(
                                    crate::ir::CoreBuiltinOp::DebugBacktrace
                                        | crate::ir::CoreBuiltinOp::DebugPrintBacktrace
                                        | crate::ir::CoreBuiltinOp::DiagnosticCallerLine
                                )
                            )
                    )
            })
    }
}

/// Deterministic literal/name pool used by IR immediates and printer output.
#[derive(Debug, Clone, Default)]
pub struct DataPool {
    pub strings: Vec<String>,
    pub float_literals: Vec<f64>,
    pub global_names: Vec<String>,
    pub function_names: Vec<String>,
    pub class_names: Vec<String>,
    pub method_names: Vec<String>,
    pub property_names: Vec<String>,
    /// Where each interned name sits in its vector. See `intern_indexed`.
    index: InternIndex,
}

/// Position of every interned value, per interned vector.
#[derive(Debug, Clone, Default)]
struct InternIndex {
    strings: crate::fast_hash::FastMap<String, u32>,
    global_names: crate::fast_hash::FastMap<String, u32>,
    function_names: crate::fast_hash::FastMap<String, u32>,
    class_names: crate::fast_hash::FastMap<String, u32>,
}

impl DataPool {
    /// Interns a string literal and returns its stable data identifier.
    pub fn intern_string(&mut self, value: &str) -> DataId {
        intern_indexed(&mut self.strings, &mut self.index.strings, value)
    }

    /// Interns a floating-point literal by exact bit pattern.
    pub fn intern_float(&mut self, value: f64) -> DataId {
        if let Some(idx) = self
            .float_literals
            .iter()
            .position(|existing| existing.to_bits() == value.to_bits())
        {
            return DataId::from_raw(idx as u32);
        }
        let id = DataId::from_raw(self.float_literals.len() as u32);
        self.float_literals.push(value);
        id
    }

    /// Interns a global symbol name and returns its stable data identifier.
    pub fn intern_global_name(&mut self, value: &str) -> DataId {
        intern_indexed(&mut self.global_names, &mut self.index.global_names, value)
    }

    /// Interns a function name and returns its stable data identifier.
    pub fn intern_function_name(&mut self, value: &str) -> DataId {
        intern_indexed(&mut self.function_names, &mut self.index.function_names, value)
    }

    /// Returns the identifier of an already interned function name, without interning it.
    pub fn function_name_id(&self, value: &str) -> Option<DataId> {
        indexed_id(&self.function_names, &self.index.function_names, value)
    }

    /// Returns the identifier of an already-interned string literal, without interning it.
    pub fn string_id(&self, value: &str) -> Option<DataId> {
        indexed_id(&self.strings, &self.index.strings, value)
    }

    /// Interns a class name and returns its stable data identifier.
    pub fn intern_class_name(&mut self, value: &str) -> DataId {
        intern_indexed(&mut self.class_names, &mut self.index.class_names, value)
    }
}

/// Returns the position of `value` in `values` when the index knows it.
///
/// An index entry is only believed when the vector still holds that value there: speculative
/// lowering rolls the pool back by truncating the vectors, which leaves entries pointing past the
/// end or at a value interned since. Checking on read is what lets a rollback stay a truncate.
fn indexed_id(
    values: &[String],
    index: &crate::fast_hash::FastMap<String, u32>,
    value: &str,
) -> Option<DataId> {
    let id = *index.get(value)?;
    (values.get(id as usize).map(String::as_str) == Some(value)).then(|| DataId::from_raw(id))
}

/// Interns `value` into a string vector and returns its zero-based index.
///
/// Finding an existing value by scanning the vector made interning quadratic in the number of
/// distinct literals, which the Symfony build felt in lowering; the index answers in one lookup.
fn intern_indexed(
    values: &mut Vec<String>,
    index: &mut crate::fast_hash::FastMap<String, u32>,
    value: &str,
) -> DataId {
    if let Some(id) = indexed_id(values, index, value) {
        return id;
    }
    let id = values.len() as u32;
    values.push(value.to_string());
    index.insert(value.to_string(), id);
    DataId::from_raw(id)
}

/// C-facing extern function declaration referenced by EIR.
#[derive(Debug, Clone)]
pub struct ExternDecl {
    pub name: String,
    pub params: Vec<ExternParamDecl>,
    pub return_type: IrType,
    pub return_php_type: PhpType,
    /// Declared C shape of the return slot; drives register normalization after the call.
    pub return_abi: ExternAbiType,
    pub link_libs: Vec<String>,
}

/// One extern function parameter.
#[derive(Debug, Clone)]
pub struct ExternParamDecl {
    pub name: String,
    pub ir_type: IrType,
    pub php_type: PhpType,
    /// Declared C shape of the parameter; narrow integers are extended by the caller.
    pub abi: ExternAbiType,
}

/// Minimal class metadata table placeholder for Phase 02.
#[derive(Debug, Clone, Default)]
pub struct ClassTable {
    pub names: Vec<String>,
}

/// Minimal enum metadata table placeholder for Phase 02.
#[derive(Debug, Clone, Default)]
pub struct EnumTable {
    pub names: Vec<String>,
}

/// Minimal interface metadata table placeholder for Phase 02.
#[derive(Debug, Clone, Default)]
pub struct InterfaceTable {
    pub names: Vec<String>,
}

/// Minimal trait metadata table placeholder for Phase 04 introspection.
#[derive(Debug, Clone, Default)]
pub struct TraitTable {
    pub names: Vec<String>,
}

/// Minimal packed-layout metadata table placeholder for Phase 02.
#[derive(Debug, Clone, Default)]
pub struct PackedLayoutTable {
    pub names: Vec<String>,
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    /// Every reflected member with attributes asks the module for the factory ids: the table is
    /// numbered once and lent out after that, and a class added later gets a fresh numbering
    /// rather than the stale one.
    #[test]
    fn attribute_factory_ids_are_numbered_once_per_module_shape() {
        let mut module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let first = match module.attribute_factory_ids() {
            Cow::Borrowed(ids) => ids as *const _,
            Cow::Owned(_) => panic!("first numbering must be the module's own table"),
        };
        let second = match module.attribute_factory_ids() {
            Cow::Borrowed(ids) => ids as *const _,
            Cow::Owned(_) => panic!("second request must reuse the module's table"),
        };
        assert_eq!(first, second);

        let tokens = crate::lexer::tokenize("<?php class Late {}").expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let checked =
            crate::types::checker::check_types(&program, module.target).expect("check");
        let late = checked.classes.get("Late").expect("Late").clone();
        module.class_infos.insert("Late".to_string(), late);
        assert!(matches!(module.attribute_factory_ids(), Cow::Owned(_)));
    }

    /// Codegen asks whole-module facts per call site; each is computed once for a module shape:
    /// a second question builds nothing, and a module that grew gets a fresh answer.
    #[test]
    fn whole_module_facts_are_computed_once_per_module_shape() {
        let mut module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let builds = std::cell::Cell::new(0);
        let build = || {
            builds.set(builds.get() + 1);
            true
        };
        assert!(module.interface_activation_events(build));
        assert!(module.interface_activation_events(build));
        assert_eq!(builds.get(), 1, "the second question must reuse the module's answer");
        // The backtrace predicates scan every instruction and codegen asks them per body.
        assert!(module.backtrace_frames(build) && module.backtrace_frames(build));
        assert!(module.materialized_backtrace(build) && module.materialized_backtrace(build));
        assert_eq!(builds.get(), 3, "each backtrace fact is computed once per module shape");
        module.required_runtime_features.eval_bridge = true;
        assert!(module.backtrace_frames(build));
        assert_eq!(builds.get(), 4, "turning the eval bridge on recomputes the backtrace fact");
        let groups = || {
            let mut names = crate::fast_hash::FastMap::default();
            names.insert("f".to_string(), "F".to_string());
            names
        };
        let first = module.variant_group_names(groups);
        assert!(std::sync::Arc::ptr_eq(&first, &module.variant_group_names(groups)));
        module.data.strings.push("grown".to_string());
        let grown = module.variant_group_names(groups);
        assert!(!std::sync::Arc::ptr_eq(&first, &grown), "a grown module is recomputed");
        assert!(
            std::sync::Arc::ptr_eq(&grown, &module.variant_group_names(groups)),
            "and the grown module's answer is cached in turn"
        );
        let groups_first = module.dispatch_groups(Vec::new);
        assert!(std::sync::Arc::ptr_eq(&groups_first, &module.dispatch_groups(Vec::new)));
        let keys_first = module.function_indexes_by_key(Default::default);
        assert!(std::sync::Arc::ptr_eq(&keys_first, &module.function_indexes_by_key(Default::default)));
    }

    /// The span index answers what the linear scan over `functions` then `closures` found, in
    /// the same order, so Reflection keeps picking the first helper in scope.
    #[test]
    fn callable_default_helpers_match_the_linear_scan_in_order() {
        let mut module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let helper = |name: &str, line: Option<u32>| {
            let mut function = crate::ir::Function::new(name.to_string(), IrType::Void, PhpType::Void);
            function.callable_default = line
                .map(|line| Expr::new(ExprKind::IntLiteral(1), crate::span::Span::new(line, 1)));
            function
        };
        module.functions.push(helper("f_none", None));
        module.functions.push(helper("f_a", Some(3)));
        module.functions.push(helper("f_b", Some(7)));
        module.closures.push(helper("c_a", Some(3)));
        module.closures.push(helper("c_none", None));
        for line in [3, 7, 9] {
            let span = crate::span::Span::new(line, 1);
            let indexed: Vec<&str> =
                module.callable_default_helpers_at(span).map(|f| f.name.as_str()).collect();
            let scanned: Vec<&str> = module
                .functions
                .iter()
                .chain(module.closures.iter())
                .filter(|f| f.callable_default.as_ref().is_some_and(|d| d.span == span))
                .map(|f| f.name.as_str())
                .collect();
            assert_eq!(indexed, scanned, "line {line}");
        }
        module.closures.push(helper("c_late", Some(7)));
        let late: Vec<&str> = module
            .callable_default_helpers_at(crate::span::Span::new(7, 1))
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(late, ["f_b", "c_late"], "a grown module is re-indexed");
    }

    /// The descendant index answers exactly what `class_inherits_from` answers, for every pair
    /// of classes of a hierarchy with siblings, depth and an unrelated root.
    #[test]
    fn strict_descendants_match_class_inherits_from() {
        let source = "<?php class A {} class B extends A {} class C extends B {} class D extends A {} class E {} class F extends E {}";
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let mut module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let checked = crate::types::checker::check_types(&program, module.target).expect("check");
        for (name, info) in checked.classes {
            module.class_infos.insert(name, info);
        }
        let index = module.strict_descendants();
        for ancestor in ["A", "B", "C", "D", "E", "F"] {
            let mut expected: Vec<&str> = module
                .class_infos
                .keys()
                .filter(|candidate| crate::types::class_inherits_from(&module.class_infos, candidate, ancestor))
                .map(String::as_str)
                .collect();
            expected.sort_unstable();
            let mut indexed: Vec<&str> = index.get(ancestor).into_iter().flatten().map(String::as_str).collect();
            indexed.sort_unstable();
            assert_eq!(indexed, expected, "{ancestor}");
        }
    }

    /// The folded-name class and interface lookups find what codegen's linear case-insensitive
    /// scans found, for every spelling, and miss what is not declared.
    #[test]
    fn folded_class_and_interface_lookups_match_the_scan() {
        let source = "<?php interface Shape {} class Square implements Shape {} class Circle {}";
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let mut module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let checked = crate::types::checker::check_types(&program, module.target).expect("check");
        module.class_infos.extend(checked.classes);
        module.interface_infos.extend(checked.interfaces);
        for spelling in ["square", "SQUARE", "\\Circle", "Circle"] {
            let key = crate::names::php_symbol_key(spelling.trim_start_matches('\\'));
            let expected = module
                .class_infos
                .keys()
                .find(|name| crate::names::php_symbol_key(name.trim_start_matches('\\')) == key);
            assert_eq!(module.class_info_by_key(&key).map(|(name, _)| name), expected.map(String::as_str), "{spelling}");
        }
        assert_eq!(module.interface_info_by_key("shape").map(|(name, _)| name), Some("Shape"));
        assert!(module.class_info_by_key("shape").is_none());
        assert!(module.interface_info_by_key("square").is_none());
    }

    /// The class-method index follows a table that grows between questions (lowering appends
    /// while it asks), keeps the first body per key, and tells static from instance bodies.
    #[test]
    fn class_method_index_follows_a_growing_table() {
        let mut module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let body = |name: &str| crate::ir::Function::new(name.to_string(), crate::ir::IrType::Void, PhpType::Void);
        module.class_methods.push(body("A::Run"));
        assert!(module.class_method("A", "run", false).is_some());
        assert!(module.class_method("B", "run", false).is_none());
        module.class_methods.push(body("B::run"));
        module.class_methods.push(body("A::run"));
        assert_eq!(module.class_method("B", "run", false).map(|f| f.name.as_str()), Some("B::run"));
        assert_eq!(module.class_method("A", "run", false).map(|f| f.name.as_str()), Some("A::Run"), "first body wins");
        assert!(module.class_method("A", "run", true).is_none());
        let mut statics = body("C::make");
        statics.flags.is_static = true;
        module.class_methods.push(statics);
        assert!(module.class_method("C", "make", true).is_some());
        assert!(module.class_method("C", "make", false).is_none());
    }
}
