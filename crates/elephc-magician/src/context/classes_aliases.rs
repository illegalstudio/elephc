//! Purpose:
//! Registers eval classes, external declarations, aliases, and callable class metadata.
//!
//! Called from:
//! - Class declaration execution and callable construction.
//!
//! Key details:
//! - Alias kinds and global class snapshots preserve case-insensitive PHP lookup.

use super::*;

impl ElephcEvalContext {
    /// Defines an eval-declared class, failing if this context already has it.
    pub fn define_class(&mut self, class: Arc<EvalClass>) -> bool {
        let key = normalize_class_name(class.name());
        if self.classes.contains_key(&key)
            || self.class_aliases.contains_key(&key)
            || self.interfaces.contains_key(&key)
            || self.traits.contains_key(&key)
            || self.enums.contains_key(&key)
        {
            return false;
        }
        self.own_declared_class_names.push(class.name().to_string());
        #[cfg(not(test))]
        register_global_eval_class(&class);
        self.classes.insert(key, class);
        true
    }

    /// Drops every class-like this context declared while running a request.
    ///
    /// The process-lifetime null-handle context — the one AOT callbacks with no handle of their
    /// own use — outlives the request that declared into it, and
    /// `reset_global_eval_classes` deliberately empties the process-global registry at each
    /// request boundary so the next request re-declares and re-publishes. Leaving this context's
    /// own tables populated defeats exactly that: `define_class` is never reached a second time
    /// (the name is already here), so nothing re-publishes, and a later request resolves the
    /// class through the owner fallback while its OWN method tables stay empty — every call on
    /// such an object then raised "Call to undefined method".
    ///
    /// The materialized CONSTANT CELLS go with them, and they must: `class_constants` caches a
    /// `RuntimeCellHandle` per `(class, constant)`, and the heap those handles point into is wiped
    /// by the same request boundary. Keeping them made the next request read a dangling cell --
    /// `twig/twig`'s `FilesystemLoader::addPath(string $path, string $namespace =
    /// self::MAIN_NAMESPACE)` could not materialize its default on the SECOND request, the bind
    /// threw, the generated Symfony container swallowed it, `get('twig')` answered null and the
    /// controller "returned null" with nothing in the log.
    ///
    /// Only declarations and their constant cells are dropped. The autoload registry and functions
    /// are reset by their own request-boundary hooks.
    pub(crate) fn forget_declared_class_likes(&mut self) {
        // The imports are gone with them, so the resume point must go too: otherwise the next
        // sync would skip exactly the classes this context just dropped.
        self.global_eval_sync = GlobalEvalSyncMark::default();
        self.classes.clear();
        self.class_constants.clear();
        self.evaluating_class_constants.clear();
        self.class_source_files.clear();
        self.class_aliases.clear();
        self.declared_class_names = SharedNative::default();
        self.own_declared_class_names.clear();
        self.interfaces.clear();
        self.declared_interface_names = SharedNative::default();
        self.own_declared_interface_names.clear();
        self.traits.clear();
        self.declared_trait_names = SharedNative::default();
        self.own_declared_trait_names.clear();
        self.enums.clear();
        self.declared_enum_names.clear();
        self.enum_cases.clear();
        self.enum_case_values.clear();
    }

    /// Drops everything a RETAINED context accumulated for one request.
    ///
    /// A context that outlives the request that filled it is the hazard `__rt_web_reset` cannot
    /// see. The generated reset wipes the PHP arena back to pure-bump allocation, so every
    /// `RuntimeCellHandle` this context still holds points at storage the NEXT request is about to
    /// hand to something else. Reading one answers with whatever now lives there -- a Twig
    /// property came back with an object tag on a string, a controller "returned null" -- and
    /// releasing one frees a block that is live, which puts a live block on the small-bin free
    /// chain and faults `__rt_heap_free` the next time the `--web` guard walks it.
    ///
    /// Handles are raw pointers, so clearing a map here neither decrefs nor frees: forgetting is
    /// exactly the right operation at a boundary where the whole arena has already gone.
    ///
    /// NOT cleared, each for its own reason: the `native_*` tables are immutable AOT metadata with
    /// no arena pointers in them; `functions` and `autoload_callbacks` are released by their own
    /// request-boundary hooks, which also finalize the contexts that own them; `include_state` is
    /// reset through `reset_global_eval_included_files`; `streams` and `pcntl_foreign_callables`
    /// own OS resources that have to be CLOSED rather than dropped; and the `live_*` counters plus
    /// `retained_context_free_requested` drive retained-context teardown, so zeroing them would
    /// either leak the context or free it twice.
    pub(crate) fn forget_request_scoped_state(&mut self) {
        self.forget_declared_class_likes();
        self.constants.clear();
        self.static_slot_keys.clear();
        self.closures.clear();
        self.closure_objects.clear();
        self.lazy_ghost_initializers.clear();
        self.next_closure_id = 0;
        self.dynamic_objects.clear();
        self.dynamic_destructing_objects.clear();
        self.dynamic_destructed_objects.clear();
        self.dynamic_property_values.clear();
        self.dynamic_property_order.clear();
        self.dynamic_property_aliases.clear();
        self.dynamic_initialized_properties.clear();
        self.array_element_aliases.clear();
        self.array_cursors.clear();
        self.array_iterators.clear();
        self.eval_generators.clear();
        self.eval_reflection_attributes.clear();
        self.eval_reflection_classes.clear();
        self.eval_reflection_functions.clear();
        self.eval_reflection_function_closure_targets.clear();
        self.eval_reflection_methods.clear();
        self.eval_reflection_properties.clear();
        self.eval_dynamic_reflection_properties.clear();
        self.eval_reflection_class_constants.clear();
        self.eval_static_callables.clear();
        self.eval_object_callables.clear();
        // Reflection-minted closure registrations name arena descriptors by address.
        self.local_native_functions.clear();
        self.claimed_aot_include_classlikes.clear();
        self.include_execution_stack.clear();
        self.autoloading_classes.clear();
        super::runtime_state::clear_global_autoloading_classes();
        self.function_stack.clear();
        self.class_stack.clear();
        self.called_class_stack.clear();
        self.magic_stack.clear();
        self.pending_return_reference = None;
        self.pending_throw = None;
        self.tick_functions.clear();
        self.tick_interval = 0;
        self.tick_counter = 0;
        self.tick_running = false;
        self.returns_by_ref = false;
        self.error_suppression_depth = 0;
        self.compile_time_constant_depth = 0;
        self.file_magic_override = None;
        // The fallback handler slots (used when the runtime has no handler hooks) hold callable
        // cells in the wiped arena; replacing one on the next request would release a dead cell.
        self.error_handler = None;
        self.error_handler_stack.clear();
        self.exception_handler = None;
        self.exception_handler_stack.clear();
        // Keyed by object identity or holding object cells / arena addresses, like the tables
        // above; an uncaught throw leaves the frame stacks non-empty at the boundary.
        self.clone_reinitialized_properties.clear();
        self.function_args_stack.clear();
        self.eval_backtrace_boundaries.clear();
        drop(self.array_reference_retirements.take());
    }

    /// Turns a context that has just been freed into the fresh one the next
    /// `__elephc_eval_context_new` hands out, keeping ONLY what a fresh context would import.
    ///
    /// WHY. Every compiled frame that reaches the bridge builds a context of its own and frees it
    /// when it returns, so a Laravel request made ~2 200 of them. Each one started empty, adopted
    /// the AOT snapshot and then imported every class-like the request had declared so far --
    /// hundreds of hash inserts and name copies -- and dropped all of it on the way out. That
    /// import plus the drop was the largest named cost of the request. A recycled context has
    /// already done the import; its sync mark tells `sync_global_eval_classes` to pick up only
    /// what was declared since.
    ///
    /// Everything else -- live-object counters, handler slots, per-frame stacks, dynamic property
    /// overlays, stream tables, constant cells -- is replaced by the defaults `new` would give:
    /// those are the fields the old frame owned, and several hold arena handles. Keeping the
    /// class-like tables is sound because a class-like is request-global in PHP: the registry
    /// every context imports from is the same one this context imported from, and the request
    /// boundary (`reset_global_eval_classes`) drains the pool so no declaration crosses it.
    pub(crate) fn into_recycled(mut self: Box<Self>) -> Box<Self> {
        let mut fresh = Box::new(Self::new());
        fresh.native_global_sync = self.native_global_sync;
        fresh.global_eval_sync = self.global_eval_sync;
        fresh.classes = std::mem::take(&mut self.classes);
        fresh.class_source_files = std::mem::take(&mut self.class_source_files);
        fresh.class_aliases = std::mem::take(&mut self.class_aliases);
        fresh.declared_class_names = std::mem::take(&mut self.declared_class_names);
        fresh.own_declared_class_names = std::mem::take(&mut self.own_declared_class_names);
        fresh.interfaces = std::mem::take(&mut self.interfaces);
        fresh.declared_interface_names = std::mem::take(&mut self.declared_interface_names);
        fresh.own_declared_interface_names =
            std::mem::take(&mut self.own_declared_interface_names);
        fresh.traits = std::mem::take(&mut self.traits);
        fresh.declared_trait_names = std::mem::take(&mut self.declared_trait_names);
        fresh.own_declared_trait_names = std::mem::take(&mut self.own_declared_trait_names);
        fresh.enums = std::mem::take(&mut self.enums);
        fresh.declared_enum_names = std::mem::take(&mut self.declared_enum_names);
        fresh.native_global_constants = std::mem::take(&mut self.native_global_constants);
        fresh.native_user_constants = std::mem::take(&mut self.native_user_constants);
        fresh.native_functions = std::mem::take(&mut self.native_functions);
        fresh.native_methods = std::mem::take(&mut self.native_methods);
        fresh.native_static_methods = std::mem::take(&mut self.native_static_methods);
        fresh.native_constructors = std::mem::take(&mut self.native_constructors);
        fresh.native_class_parents = std::mem::take(&mut self.native_class_parents);
        fresh.native_class_attributes = std::mem::take(&mut self.native_class_attributes);
        fresh.native_method_attributes = std::mem::take(&mut self.native_method_attributes);
        fresh.native_constant_attributes = std::mem::take(&mut self.native_constant_attributes);
        fresh.native_interface_properties = std::mem::take(&mut self.native_interface_properties);
        fresh.native_abstract_properties = std::mem::take(&mut self.native_abstract_properties);
        fresh.native_property_types = std::mem::take(&mut self.native_property_types);
        fresh.native_property_defaults = std::mem::take(&mut self.native_property_defaults);
        fresh.native_property_attributes = std::mem::take(&mut self.native_property_attributes);
        fresh.native_class_constant_defaults = std::mem::take(&mut self.native_class_constant_defaults);
        // `self` drops here with everything the old frame owned, exactly as the free did before.
        fresh
    }

    /// Records the physical file that defined one eval class for method magic constants.
    pub(crate) fn set_class_source_file(&mut self, class_name: &str, file: String) {
        if file.is_empty() {
            return;
        }
        // Through the shared realpath cache, not `fs::canonicalize`: every class an autoloader
        // declares lands here, so an uncached call is one `realpath` syscall per class per
        // request. A profile of a Symfony `--web` worker put 7% of the request in that one
        // call. The cache already answers `include`'s canonicalization with php's own TTL.
        let file = crate::realpath_cache::canonicalize_cached(std::path::Path::new(&file))
            .to_string_lossy()
            .into_owned();
        self.class_source_files
            .insert(normalize_class_name(class_name), file.clone());
        #[cfg(not(test))]
        crate::context::register_global_eval_class_source(class_name, file);
    }

    /// Returns the source file that supplied one eval class declaration, when known.
    pub(crate) fn class_source_file(&self, class_name: &str) -> Option<&str> {
        self.class_source_files
            .get(&normalize_class_name(class_name))
            .map(String::as_str)
    }

    /// Imports eval-declared process-global class-like metadata not yet known by this context.
    ///
    /// Every autoload step calls this, and it used to re-walk the whole process-global registry
    /// each time: one `normalize_class_name` allocation and five hash probes per known class, for
    /// every class already known. That is quadratic in the number of classes a request declares,
    /// and on a Symfony `--web` request it was the single most expensive named path in the
    /// interpreter -- more than reflection, includes and dispatch.
    ///
    /// The registry only ever grows between resets, so a context that already consumed the first
    /// `n` names needs to look at the rest. The generation makes that safe across a request
    /// boundary: `reset_global_eval_classes` empties the registry and bumps it, which tells a
    /// surviving context that index `n` no longer means the class it once did, and the next sync
    /// starts over from zero.
    #[cfg(not(test))]
    pub fn sync_global_eval_classes(&mut self) {
        let Ok(registry) = global_eval_classes().lock() else {
            return;
        };
        if self.global_eval_sync.generation != registry.generation {
            self.global_eval_sync = GlobalEvalSyncMark {
                generation: registry.generation,
                ..GlobalEvalSyncMark::default()
            };
        }
        let mark = self.global_eval_sync;
        for name in registry.declared_class_names.iter().skip(mark.classes) {
            let key = normalize_class_name(name);
            if self.classes.contains_key(&key)
                || self.interfaces.contains_key(&key)
                || self.traits.contains_key(&key)
                || self.enums.contains_key(&key)
                || self.class_aliases.contains_key(&key)
            {
                continue;
            }
            // `Arc::clone`, not a deep copy: importing a class another context declared must not
            // duplicate its method bodies into this one.
            let Some(class) = registry.classes.get(&key).map(Arc::clone) else {
                continue;
            };
            self.own_declared_class_names.push(class.name().to_string());
            self.classes.insert(key.clone(), class);
            if let Some(file) = registry.class_source_files.get(&key) {
                self.class_source_files.insert(key.clone(), file.clone());
            }
        }
        self.global_eval_sync.classes = registry.declared_class_names.len();
        for name in registry.declared_interface_names.iter().skip(mark.interfaces) {
            let key = normalize_class_name(name);
            if self.interfaces.contains_key(&key)
                || self.classes.contains_key(&key)
                || self.traits.contains_key(&key)
                || self.enums.contains_key(&key)
                || self.class_aliases.contains_key(&key)
            {
                continue;
            }
            let Some(interface) = registry.interfaces.get(&key).cloned() else {
                continue;
            };
            self.own_declared_interface_names
                .push(interface.name().to_string());
            if let Some(file) = registry.class_source_files.get(&key) {
                self.class_source_files.insert(key.clone(), file.clone());
            }
            self.interfaces.insert(key, interface);
        }
        self.global_eval_sync.interfaces = registry.declared_interface_names.len();
        for name in registry.declared_trait_names.iter().skip(mark.traits) {
            let key = normalize_class_name(name);
            if self.traits.contains_key(&key)
                || self.classes.contains_key(&key)
                || self.interfaces.contains_key(&key)
                || self.enums.contains_key(&key)
                || self.class_aliases.contains_key(&key)
            {
                continue;
            }
            let Some(trait_decl) = registry.traits.get(&key).cloned() else {
                continue;
            };
            self.own_declared_trait_names
                .push(trait_decl.name().to_string());
            if let Some(file) = registry.class_source_files.get(&key) {
                self.class_source_files.insert(key.clone(), file.clone());
            }
            self.traits.insert(key, trait_decl);
        }
        self.global_eval_sync.traits = registry.declared_trait_names.len();
        for name in registry.declared_enum_names.iter().skip(mark.enums) {
            let key = normalize_class_name(name);
            if self.enums.contains_key(&key)
                || self.classes.contains_key(&key)
                || self.interfaces.contains_key(&key)
                || self.traits.contains_key(&key)
                || self.class_aliases.contains_key(&key)
            {
                continue;
            }
            let Some(enum_decl) = registry.enums.get(&key).cloned() else {
                continue;
            };
            self.declared_enum_names
                .push(enum_decl.name().trim_start_matches('\\').to_string());
            self.own_declared_class_names
                .push(enum_decl.name().trim_start_matches('\\').to_string());
            if let Some(file) = registry.class_source_files.get(&key) {
                self.class_source_files.insert(key.clone(), file.clone());
            }
            self.classes
                .insert(key.clone(), Arc::new(enum_decl.as_class_metadata()));
            self.enums.insert(key, enum_decl);
        }
        self.global_eval_sync.enums = registry.declared_enum_names.len();
        // Aliases are a map, so there is no index to resume from -- but `class_alias` is rare and
        // the map is small, so re-walking it only when its size moved costs nothing.
        if mark.aliases != registry.aliases.len() {
            for (key, alias) in &registry.aliases {
                if self.classes.contains_key(key)
                    || self.interfaces.contains_key(key)
                    || self.traits.contains_key(key)
                    || self.enums.contains_key(key)
                    || self.class_aliases.contains_key(key)
                {
                    continue;
                }
                self.class_aliases.insert(key.clone(), alias.clone());
            }
            self.global_eval_sync.aliases = registry.aliases.len();
        }
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
        self.classes
            .get(&normalize_class_name(&alias.target))
            .map(Arc::as_ref)
    }

    /// Returns the SHARED handle for a dynamic eval class, for callers that need to keep it.
    ///
    /// [`Self::class`] hands out a borrow, and a caller that must outlive the borrow used to
    /// `.cloned()` it -- a deep copy of every method body. This hands over the allocation itself.
    pub fn class_shared(&self, name: &str) -> Option<&Arc<EvalClass>> {
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

    /// Registers one eval-created static callable array with late-static dispatch metadata.
    pub fn register_eval_static_callable(
        &mut self,
        callable: RuntimeCellHandle,
        class_name: &str,
        method: &str,
        called_class: &str,
        native_dispatch: Option<(&str, &str)>,
    ) {
        let (native_class, bridge_scope) = native_dispatch
            .map(|(native_class, bridge_scope)| {
                (
                    Some(native_class.trim_start_matches('\\').to_string()),
                    Some(bridge_scope.trim_start_matches('\\').to_string()),
                )
            })
            .unwrap_or((None, None));
        self.eval_static_callables.insert(
            callable.as_ptr() as usize,
            EvalStaticCallableMetadata {
                class_name: class_name.trim_start_matches('\\').to_string(),
                method: method.to_string(),
                called_class: called_class.trim_start_matches('\\').to_string(),
                native_class,
                bridge_scope,
            },
        );
    }

    /// Returns the captured late-static called class for one matching static callable array.
    pub fn eval_static_callable_called_class(
        &self,
        callable: RuntimeCellHandle,
        class_name: &str,
        method: &str,
    ) -> Option<&str> {
        let metadata = self.eval_static_callables.get(&(callable.as_ptr() as usize))?;
        let class_name = class_name.trim_start_matches('\\');
        (metadata.class_name.eq_ignore_ascii_case(class_name)
            && metadata.method.eq_ignore_ascii_case(method))
        .then_some(metadata.called_class.as_str())
    }

    /// Returns native method bridge metadata captured for one static callable array.
    pub fn eval_static_callable_native_dispatch(
        &self,
        callable: RuntimeCellHandle,
        class_name: &str,
        method: &str,
    ) -> Option<(&str, &str)> {
        let metadata = self.eval_static_callables.get(&(callable.as_ptr() as usize))?;
        let class_name = class_name.trim_start_matches('\\');
        if !metadata.class_name.eq_ignore_ascii_case(class_name)
            || !metadata.method.eq_ignore_ascii_case(method)
        {
            return None;
        }
        Some((
            metadata.native_class.as_deref()?,
            metadata.bridge_scope.as_deref()?,
        ))
    }

    /// Registers one eval-created object method callable with native bridge metadata.
    pub fn register_eval_object_callable(
        &mut self,
        callable: RuntimeCellHandle,
        object: RuntimeCellHandle,
        method: &str,
        called_class: &str,
        native_class: &str,
        bridge_scope: &str,
    ) {
        self.eval_object_callables.insert(
            callable.as_ptr() as usize,
            EvalObjectCallableMetadata {
                object: object.as_ptr() as usize,
                method: method.to_string(),
                called_class: called_class.trim_start_matches('\\').to_string(),
                native_class: native_class.trim_start_matches('\\').to_string(),
                bridge_scope: bridge_scope.trim_start_matches('\\').to_string(),
            },
        );
    }

    /// Returns native method bridge metadata captured for one object callable array.
    pub fn eval_object_callable_native_dispatch(
        &self,
        callable: RuntimeCellHandle,
        object: RuntimeCellHandle,
        method: &str,
    ) -> Option<(&str, &str, &str)> {
        let metadata = self
            .eval_object_callables
            .get(&(callable.as_ptr() as usize))?;
        (metadata.object == object.as_ptr() as usize
            && metadata.method.eq_ignore_ascii_case(method))
        .then_some((
            metadata.native_class.as_str(),
            metadata.bridge_scope.as_str(),
            metadata.called_class.as_str(),
        ))
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
    pub(super) fn resolve_class_like_alias_target(
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
    pub(super) fn define_class_alias_with_kind(
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
        let alias_record = EvalClassAlias {
            target: original.trim_start_matches('\\').to_string(),
            kind,
        };
        #[cfg(not(test))]
        register_global_eval_alias(alias, &alias_record);
        self.class_aliases.insert(alias_key, alias_record);
        true
    }

    /// Returns class names declared through eval or registered from generated metadata.
    pub fn declared_class_names(&self) -> Vec<String> {
        self.declared_class_names
            .iter()
            .chain(self.own_declared_class_names.iter())
            .cloned()
            .collect()
    }

    /// Registers a runtime-visible class or enum declaration name for `get_declared_classes()`.
    pub fn define_external_declared_class_name(&mut self, name: &str) -> bool {
        push_external_declared_name(
            &self.declared_class_names,
            &mut self.own_declared_class_names,
            name,
        )
    }
}
