//! Purpose:
//! Manages interfaces, traits, enums, dynamic objects, and runtime property aliases.
//!
//! Called from:
//! - Class-like declaration execution and object/property runtime paths.
//!
//! Key details:
//! - Enum cases, destructor state, aliases, and initialized-property markers stay context-owned.

use super::*;

impl ElephcEvalContext {
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
        self.own_declared_interface_names
            .push(interface.name().to_string());
        let interface = Arc::new(interface);
        #[cfg(not(test))]
        register_global_eval_interface(&interface);
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
            return Some(interface.as_ref());
        }
        let alias = self.class_aliases.get(&key)?;
        (alias.kind == EvalClassAliasKind::Interface)
            .then(|| self.interfaces.get(&normalize_class_name(&alias.target)))
            .flatten()
            .map(Arc::as_ref)
    }

    /// Returns interface names declared through eval or registered from generated metadata.
    pub fn declared_interface_names(&self) -> Vec<String> {
        self.declared_interface_names
            .iter()
            .chain(self.own_declared_interface_names.iter())
            .cloned()
            .collect()
    }

    /// Registers a runtime-visible interface declaration name for `get_declared_interfaces()`.
    pub fn define_external_declared_interface_name(&mut self, name: &str) -> bool {
        push_external_declared_name(
            &self.declared_interface_names,
            &mut self.own_declared_interface_names,
            name,
        )
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
        self.own_declared_trait_names
            .push(trait_decl.name().to_string());
        let trait_decl = Arc::new(trait_decl);
        #[cfg(not(test))]
        register_global_eval_trait(&trait_decl);
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
            return Some(trait_decl.as_ref());
        }
        let alias = self.class_aliases.get(&key)?;
        (alias.kind == EvalClassAliasKind::Trait)
            .then(|| self.traits.get(&normalize_class_name(&alias.target)))
            .flatten()
            .map(Arc::as_ref)
    }

    /// Returns trait names declared through eval or registered from generated metadata.
    pub fn declared_trait_names(&self) -> Vec<String> {
        self.declared_trait_names
            .iter()
            .chain(self.own_declared_trait_names.iter())
            .cloned()
            .collect()
    }

    /// Registers a runtime-visible trait declaration name for `get_declared_traits()`.
    pub fn define_external_declared_trait_name(&mut self, name: &str) -> bool {
        push_external_declared_name(
            &self.declared_trait_names,
            &mut self.own_declared_trait_names,
            name,
        )
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
        self.own_declared_class_names
            .push(enum_decl.name().trim_start_matches('\\').to_string());
        let enum_decl = Arc::new(enum_decl);
        #[cfg(not(test))]
        register_global_eval_enum(&enum_decl);
        self.classes
            .insert(key.clone(), Arc::new(enum_decl.as_class_metadata()));
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
            return Some(enum_decl.as_ref());
        }
        let alias = self.class_aliases.get(&key)?;
        (alias.kind == EvalClassAliasKind::Enum)
            .then(|| self.enums.get(&normalize_class_name(&alias.target)))
            .flatten()
            .map(Arc::as_ref)
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
        let key = (
            normalize_class_name(&enum_name),
            normalize_enum_case_name(case_name),
        );
        if let Some(case) = self.enum_cases.get(&key).copied() {
            return Some(case.borrowed());
        }
        // An enum declared by ANOTHER context (a class file the autoloader included from a
        // compiled frame gets a context of its own) is imported by `sync_global_eval_classes`,
        // but its case singletons were made over there. Symfony's `ReflectionExtractor::MAP_TYPES`
        // reads `TypeIdentifier::INT->value` right after the autoloader declared the enum.
        #[cfg(not(test))]
        if self.enums.contains_key(&key.0) {
            return global_eval_enum_case(&key.0, &key.1).map(|(case, _)| case.borrowed());
        }
        None
    }

    /// Stores a materialized singleton case object and returns any replaced distinct cell.
    pub fn set_enum_case(
        &mut self,
        enum_name: &str,
        case_name: &str,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let key = (
            normalize_class_name(enum_name),
            normalize_enum_case_name(case_name),
        );
        #[cfg(not(test))]
        register_global_eval_enum_case(key.0.clone(), key.1.clone(), cell);
        let previous = self.enum_cases.insert(key, cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Returns a materialized backing value for one eval backed-enum case.
    pub fn enum_case_value(&self, enum_name: &str, case_name: &str) -> Option<RuntimeCellHandle> {
        let enum_name = self
            .resolve_enum_name(enum_name)
            .unwrap_or_else(|| enum_name.trim_start_matches('\\').to_string());
        let key = (
            normalize_class_name(&enum_name),
            normalize_enum_case_name(case_name),
        );
        if let Some(value) = self.enum_case_values.get(&key).copied() {
            return Some(value.borrowed());
        }
        #[cfg(not(test))]
        if self.enums.contains_key(&key.0) {
            return global_eval_enum_case(&key.0, &key.1).and_then(|(_, value)| value).map(RuntimeCellHandle::borrowed);
        }
        None
    }

    /// Stores a materialized backing value and returns any replaced distinct cell.
    pub fn set_enum_case_value(
        &mut self,
        enum_name: &str,
        case_name: &str,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let key = (
            normalize_class_name(enum_name),
            normalize_enum_case_name(case_name),
        );
        #[cfg(not(test))]
        register_global_eval_enum_case_value(key.0.clone(), key.1.clone(), cell);
        let previous = self.enum_case_values.insert(key, cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Records that one runtime object handle was created for an eval-declared class.
    pub fn register_dynamic_object(&mut self, identity: u64, class_name: &str) {
        let class_name = self
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.to_string());
        if self
            .dynamic_objects
            .insert(identity, normalize_class_name(&class_name))
            .is_none()
        {
            self.live_dynamic_objects
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        crate::ffi::dynamic_destructors::register_dynamic_object_context(identity, self as *mut Self);
        self.dynamic_destructing_objects.remove(&identity);
        self.dynamic_destructed_objects.remove(&identity);
        self.dynamic_initialized_properties
            .retain(|(object, _)| *object != identity);
    }

    /// Registers one `ArrayIterator` instance's backing array, which the context OWNS.
    ///
    /// `ArrayIterator` is a NAME in the known-class list with no implementation behind it, so the
    /// object it builds has no members at all. Its state lives here for the same reason a
    /// closure's target does: the runtime object carries no eval-visible storage of its own.
    /// Returns any array it replaces, so the caller can give that reference back.
    pub fn set_array_iterator(
        &mut self,
        identity: u64,
        storage: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let replaced = self
            .array_iterators
            .insert(identity, EvalArrayIteratorState { storage, position: 0 })
            .map(|state| state.storage);
        replaced.filter(|previous| *previous != storage)
    }

    /// Replaces one `ArrayIterator`'s backing array, keeping its cursor.
    pub fn replace_array_iterator_storage(
        &mut self,
        identity: u64,
        storage: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let state = self.array_iterators.get_mut(&identity)?;
        let replaced = std::mem::replace(&mut state.storage, storage);
        (replaced != storage).then_some(replaced)
    }

    /// Returns one `ArrayIterator`'s backing array, if the identity is one.
    pub fn array_iterator_storage(&self, identity: u64) -> Option<RuntimeCellHandle> {
        self.array_iterators.get(&identity).map(|state| state.storage)
    }

    /// Returns one `ArrayIterator`'s zero-based cursor.
    pub fn array_iterator_position(&self, identity: u64) -> Option<usize> {
        self.array_iterators.get(&identity).map(|state| state.position)
    }

    /// Moves one `ArrayIterator`'s cursor.
    pub fn set_array_iterator_position(&mut self, identity: u64, position: usize) {
        if let Some(state) = self.array_iterators.get_mut(&identity) {
            state.position = position;
        }
    }

    /// Records the (retained) initializer of a lazy ghost, returning one it replaces.
    pub fn register_lazy_ghost(
        &mut self,
        identity: u64,
        initializer: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        self.lazy_ghost_initializers.insert(identity, initializer)
    }

    /// Removes and returns a pending lazy-ghost initializer, marking the object initialized.
    pub fn take_lazy_ghost_initializer(&mut self, identity: u64) -> Option<RuntimeCellHandle> {
        if self.lazy_ghost_initializers.is_empty() {
            return None;
        }
        self.lazy_ghost_initializers.remove(&identity)
    }

    /// Returns whether an object is a lazy ghost whose initializer has not run yet.
    pub fn is_uninitialized_lazy_ghost(&self, identity: u64) -> bool {
        self.lazy_ghost_initializers.contains_key(&identity)
    }

    /// Removes one dynamic object and returns its owned properties plus deferred-free state.
    pub fn forget_dynamic_object(&mut self, identity: u64) -> (Vec<RuntimeCellHandle>, bool) {
        let removed_dynamic_object = self.dynamic_objects.remove(&identity).is_some();
        // A closure object's metadata outlives its destructor hook (see
        // `eval_dynamic_destructor`) and is dropped here, once its receiver edges are released,
        // so its lifetime count is settled here too.
        let closure_finalizes = self.forget_closure_object(identity);
        self.dynamic_destructing_objects.remove(&identity);
        self.dynamic_destructed_objects.remove(&identity);
        self.clone_reinitialized_properties.remove(&identity);
        let property_values = self
            .dynamic_property_order
            .remove(&identity)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|property| self.dynamic_property_values.remove(&(identity, property)))
            .collect();
        self.dynamic_property_aliases
            .retain(|(object, _), _| *object != identity);
        self.dynamic_initialized_properties
            .retain(|(object, _)| *object != identity);
        crate::ffi::dynamic_destructors::unregister_dynamic_object(identity);
        let mut property_values: Vec<RuntimeCellHandle> = property_values;
        if let Some(state) = self.array_iterators.remove(&identity) {
            property_values.push(state.storage);
        }
        // A ghost released before anything touched it never runs its initializer; the entry must
        // still go, or the next object allocated at this address would inherit it.
        if let Some(initializer) = self.lazy_ghost_initializers.remove(&identity) {
            property_values.push(initializer);
        }
        let should_finalize =
            (removed_dynamic_object && self.forget_dynamic_object_owner()) || closure_finalizes;
        (property_values, should_finalize)
    }

    /// Returns one retained-by-context overlay value for an eval object property slot.
    pub fn dynamic_property_value(
        &self,
        identity: u64,
        storage_property_name: &str,
    ) -> Option<RuntimeCellHandle> {
        if let Some(value) = self
            .dynamic_property_values
            .get(&(identity, storage_property_name.to_string()))
            .copied()
        {
            return Some(value.borrowed());
        }
        #[cfg(not(test))]
        {
            dynamic_object_foreign_owner(self, identity)?
                .dynamic_property_values
                .get(&(identity, storage_property_name.to_string()))
                .copied()
        }
        #[cfg(test)]
        {
            None
        }
    }

    /// Stores one owned overlay value and returns any replaced distinct cell.
    pub fn set_dynamic_property_value(
        &mut self,
        identity: u64,
        storage_property_name: &str,
        value: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        // An object another context built keeps its state in THAT context. Writing it here made
        // a shadow only this context could see: Laravel's routes file, included from a closure
        // with its own context, set the router's compiled routes through `setCompiledRoutes()`,
        // and the router itself kept reading the empty collection it was built with -- every
        // request 404ed. A slot this context already shadows keeps being written here, so its
        // own later reads stay consistent.
        #[cfg(not(test))]
        if !self
            .dynamic_property_values
            .contains_key(&(identity, storage_property_name.to_string()))
        {
            if let Some(owner) = dynamic_object_foreign_owner_mut(self, identity) {
                return owner.set_dynamic_property_value(identity, storage_property_name, value);
            }
        }
        let previous = self
            .dynamic_property_values
            .insert((identity, storage_property_name.to_string()), value);
        if previous.is_none() {
            // First write of this name on this object fixes its position. PHP reports dynamic
            // properties in creation order, and overwriting a property does not move it.
            self.dynamic_property_order
                .entry(identity)
                .or_default()
                .push(storage_property_name.to_string());
        }
        previous.filter(|previous| *previous != value)
    }

    /// Removes and returns one owned overlay value for an eval object property slot.
    pub fn remove_dynamic_property_value(
        &mut self,
        identity: u64,
        storage_property_name: &str,
    ) -> Option<RuntimeCellHandle> {
        #[cfg(not(test))]
        if !self
            .dynamic_property_values
            .contains_key(&(identity, storage_property_name.to_string()))
        {
            if let Some(owner) = dynamic_object_foreign_owner_mut(self, identity) {
                return owner.remove_dynamic_property_value(identity, storage_property_name);
            }
        }
        let removed = self
            .dynamic_property_values
            .remove(&(identity, storage_property_name.to_string()));
        if removed.is_some() {
            if let Some(order) = self.dynamic_property_order.get_mut(&identity) {
                order.retain(|name| name != storage_property_name);
            }
        }
        removed
    }

    /// Returns property names and cells that need retaining for a shallow object clone.
    pub fn dynamic_property_values_for_clone(
        &self,
        identity: u64,
    ) -> Vec<(String, RuntimeCellHandle)> {
        self.dynamic_property_storage_names(identity)
            .into_iter()
            .filter_map(|property| {
                self.dynamic_property_value(identity, &property)
                    .map(|value| (property, value))
            })
            .collect()
    }

    /// Returns the storage names of every overlay slot currently held for one object.
    ///
    /// A property created by `$object->name = ...` on an eval-declared object exists ONLY here:
    /// the runtime-slot write in `interpreter::statements::instance_property_access` is guarded
    /// on the property being declared, so an undeclared one never reaches the object's slots.
    /// Anything that enumerates or probes an object's properties has to consult this too, or it
    /// reports a dynamic property as absent.
    ///
    /// The order is the order the properties were first written, which is the order PHP
    /// reports them in, so a caller can present these to a program as they come.
    pub fn dynamic_property_storage_names(&self, identity: u64) -> Vec<String> {
        if let Some(order) = self.dynamic_property_order.get(&identity) {
            return order.clone();
        }
        #[cfg(not(test))]
        {
            dynamic_object_foreign_owner(self, identity)
                .and_then(|owner| owner.dynamic_property_order.get(&identity).cloned())
                .unwrap_or_default()
        }
        #[cfg(test)]
        {
            Vec::new()
        }
    }

    /// Returns the `index`th cell THIS context owns on behalf of one object, for the native
    /// cycle collector.
    ///
    /// The overlay values, an `ArrayIterator`'s storage and a pending lazy-ghost initializer are
    /// all references the object holds, released with it by `forget_dynamic_object`, but they
    /// live here rather than in the object's slots. A collector that cannot see them takes each
    /// for an external root: `$o->self = $o` on an eval-declared object was never collected.
    /// Only this context's own entries are answered; a shadow another context keeps stays an
    /// uncounted (conservative) owner.
    pub fn dynamic_object_gc_child(&self, identity: u64, index: usize) -> Option<RuntimeCellHandle> {
        let order = self.dynamic_property_order.get(&identity);
        let property_count = order.map_or(0, Vec::len);
        if index < property_count {
            let name = &order?[index];
            return self
                .dynamic_property_values
                .get(&(identity, name.clone()))
                .copied();
        }
        let mut extra = index - property_count;
        if let Some(state) = self.array_iterators.get(&identity) {
            if extra == 0 {
                return Some(state.storage);
            }
            extra -= 1;
        }
        if extra == 0 {
            return self.lazy_ghost_initializers.get(&identity).copied();
        }
        None
    }

    /// Removes this context from the process-local dynamic object destructor registry.
    pub fn unregister_dynamic_object_context(&self) {
        crate::ffi::dynamic_destructors::unregister_dynamic_objects_for_context(
            self as *const Self as *mut Self,
        );
    }

    /// Returns the dynamic eval class metadata associated with one object identity.
    pub fn dynamic_object_class(&self, identity: u64) -> Option<&EvalClass> {
        if let Some(class_key) = self.dynamic_objects.get(&identity) {
            return self.classes.get(class_key).map(Arc::as_ref);
        }
        #[cfg(not(test))]
        {
            let owner = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)?;
            let owner = unsafe { owner.as_ref()? };
            if owner.abi_version() != ABI_VERSION {
                return None;
            }
            let class_key = owner.dynamic_objects.get(&identity)?;
            // This context may never have imported the class: a hydrator closure that the
            // autoloaded `DeepClone` declared writes objects of a class Doctrine's autoloader
            // declared elsewhere. Missing it here sent `$object->$name = $value` to the native
            // shell's hash, and the private slot the class reads stayed null ("This ParserResult
            // lacks both the SqlFinalizer..."). The owner outlives its objects, so its entry does.
            self.classes
                .get(class_key)
                .or_else(|| owner.classes.get(class_key))
                .map(Arc::as_ref)
        }
        #[cfg(test)]
        {
            None
        }
    }

    /// Imports the declarations another context made when this one is about to touch an object of
    /// a class it has never seen.
    ///
    /// `dynamic_object_class` answers such an object from its owner, but every later lookup by
    /// NAME -- declared properties, visibility, the class chain -- reads this context's own table.
    /// Without the import, Symfony's polyfill-deepclone hydrator (declared by one autoloaded file)
    /// wrote a Doctrine `ParserResult` (declared by another) as if `$sqlFinalizer` were undeclared:
    /// a public dynamic property next to the private slot the class reads, which stayed null.
    pub fn import_foreign_object_class(&mut self, identity: u64) {
        if self.dynamic_objects.contains_key(&identity) {
            return;
        }
        #[cfg(not(test))]
        {
            let Some(owner) = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)
            else {
                return;
            };
            let Some(owner) = (unsafe { owner.as_ref() }) else {
                return;
            };
            if owner.abi_version() != ABI_VERSION {
                return;
            }
            let Some(class_key) = owner.dynamic_objects.get(&identity) else {
                return;
            };
            if !self.classes.contains_key(class_key) {
                self.sync_global_eval_classes();
            }
        }
    }

    /// Returns the eval class one dynamic object was built from, with the context that declares it.
    ///
    /// An object created inside one eval context carries the class *that* context declared, so a
    /// second context asking about the same object has to read the owner's class table. Resolving
    /// the owner's class *name* against the asker's own table — what `dynamic_object_class` does —
    /// loses the class outright whenever the asker has not itself declared a class of that name,
    /// and every relation asked about the object then answers false, its own class included.
    /// Answers are ordered owner-last so a context that knows the object keeps deciding for it.
    pub fn dynamic_object_declaring_class(&self, identity: u64) -> Option<(&Self, &EvalClass)> {
        if let Some(class) = self
            .dynamic_objects
            .get(&identity)
            .and_then(|class_key| self.classes.get(class_key))
        {
            return Some((self, class));
        }
        #[cfg(not(test))]
        {
            let owner = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)?;
            let owner = unsafe { owner.as_ref()? };
            if owner.abi_version() != ABI_VERSION {
                return None;
            }
            let class_key = owner.dynamic_objects.get(&identity)?;
            owner.classes.get(class_key).map(|class| (owner, Arc::as_ref(class)))
        }
        #[cfg(test)]
        {
            None
        }
    }

    /// Returns the PHP-visible eval class name associated with one dynamic object identity.
    pub fn dynamic_object_class_name(&self, identity: u64) -> Option<String> {
        if self.closure_objects.contains_key(&identity) {
            return Some(String::from("Closure"));
        }
        if let Some(class) = self.dynamic_object_class(identity) {
            return Some(class.name().trim_start_matches('\\').to_string());
        }
        #[cfg(not(test))]
        {
            let owner = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)?;
            let owner = unsafe { owner.as_ref()? };
            if owner.abi_version() != ABI_VERSION {
                return None;
            }
            owner
                .dynamic_object_class(identity)
                .map(|class| class.name().trim_start_matches('\\').to_string())
        }
        #[cfg(test)]
        {
            None
        }
    }

    /// Marks one dynamic object's destructor as active if it has not already run.
    pub fn begin_dynamic_object_destructor(&mut self, identity: u64) -> bool {
        if self.dynamic_destructed_objects.contains(&identity) {
            return false;
        }
        if !self.dynamic_destructing_objects.insert(identity) {
            return false;
        }
        self.dynamic_destructed_objects.insert(identity);
        true
    }

    /// Clears the active destructor guard for one dynamic object identity.
    pub fn finish_dynamic_object_destructor(&mut self, identity: u64) {
        self.dynamic_destructing_objects.remove(&identity);
    }

    /// Suppresses destructor execution for a receiver whose construction did not complete.
    pub fn suppress_dynamic_object_destructor(&mut self, identity: u64) {
        self.dynamic_destructed_objects.insert(identity);
    }

    /// Reports an executed or suppressed destructor, including failed construction receivers.
    pub fn dynamic_object_destructor_is_finished(&self, identity: u64) -> bool {
        self.dynamic_destructed_objects.contains(&identity)
    }

    /// Returns whether one dynamic object identity was registered with a class-like name.
    pub fn dynamic_object_is_class(&self, identity: u64, class_name: &str) -> bool {
        let class_name = normalize_class_name(class_name);
        if class_name == "closure" && self.closure_objects.contains_key(&identity) {
            return true;
        }
        if self
            .dynamic_objects
            .get(&identity)
            .is_some_and(|class_key| class_key == &class_name)
        {
            return true;
        }
        #[cfg(not(test))]
        {
            let Some(owner) =
                crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)
            else {
                return false;
            };
            let Some(owner) = (unsafe { owner.as_ref() }) else {
                return false;
            };
            if owner.abi_version() != ABI_VERSION {
                return false;
            }
            owner
                .dynamic_objects
                .get(&identity)
                .is_some_and(|class_key| class_key == &class_name)
        }
        #[cfg(test)]
        {
            false
        }
    }

    /// Returns whether one dynamic object's declaring class satisfies a class-like target.
    pub fn dynamic_object_is_a(&self, identity: u64, target: &str) -> bool {
        if let Some(class_key) = self.dynamic_objects.get(&identity) {
            return self.class_is_a(class_key, target, false);
        }
        if self.array_iterators.contains_key(&identity) {
            // An `ArrayIterator` has no eval class to walk, so its interfaces are answered from
            // the side table that holds its state. Without this a declared
            // `getIterator(): Traversable` returning one is a TypeError -- which is the shape
            // every Symfony bag uses.
            return eval_array_iterator_class_is_a(target);
        }
        #[cfg(not(test))]
        {
            let Some(owner) =
                crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)
            else {
                return false;
            };
            let Some(owner) = (unsafe { owner.as_ref() }) else {
                return false;
            };
            if owner.abi_version() != ABI_VERSION {
                return false;
            }
            return owner
                .dynamic_objects
                .get(&identity)
                .is_some_and(|class_key| owner.class_is_a(class_key, target, false));
        }
        #[cfg(test)]
        {
            false
        }
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

    /// Binds one runtime array payload slot to a PHP reference target.
    pub fn bind_array_element_alias(
        &mut self,
        array: RuntimeCellHandle,
        key: EvalArrayReferenceKey,
        target: EvalReferenceTarget,
    ) -> Option<EvalReferenceTarget> {
        self.prune_retired_array_aliases();
        let address = array.as_ptr() as usize;
        let aliases = self.array_element_aliases.entry(address).or_insert_with(|| {
            let lifetime = crate::ffi::array_references::register_array_reference_cell(
                address, &self.array_reference_retirements,
            );
            EvalArrayReferenceAliases { targets: HashMap::new(), lifetime }
        });
        aliases.targets.insert(key, target)
    }

    /// Returns whether any slot of this runtime array is bound to a PHP reference target.
    pub fn array_has_element_aliases(&self, array: RuntimeCellHandle) -> bool {
        self.array_element_aliases
            .get(&(array.as_ptr() as usize))
            .is_some_and(|aliases| !aliases.targets.is_empty())
    }

    /// Returns the persistent reference target bound to one runtime array payload slot.
    pub fn array_element_alias(
        &self,
        array: RuntimeCellHandle,
        key: &EvalArrayReferenceKey,
    ) -> Option<&EvalReferenceTarget> {
        self.array_element_aliases
            .get(&(array.as_ptr() as usize))
            .filter(|alias| alias.lifetime.is_live())
            .and_then(|alias| alias.targets.get(key))
    }

    /// Copies element references into a fresh COW array, omitting an explicitly removed key.
    pub(crate) fn clone_array_element_aliases(
        &mut self,
        source: RuntimeCellHandle,
        destination: RuntimeCellHandle,
        removed: Option<&EvalArrayReferenceKey>,
    ) {
        self.prune_retired_array_aliases();
        let targets = self.array_element_aliases.get(&(source.as_ptr() as usize))
            .filter(|aliases| aliases.lifetime.is_live())
            .map(|aliases| aliases.targets.iter()
                .filter(|(key, _)| Some(*key) != removed)
                .map(|(key, target)| (key.clone(), target.clone()))
                .collect::<HashMap<_, _>>())
            .unwrap_or_default();
        let destination = destination.as_ptr() as usize;
        self.array_element_aliases.remove(&destination);
        if !targets.is_empty() {
            let lifetime = crate::ffi::array_references::register_array_reference_cell(
                destination, &self.array_reference_retirements,
            );
            self.array_element_aliases.insert(destination, EvalArrayReferenceAliases { targets, lifetime });
        }
    }

    /// Removes alias and cursor metadata when an array box is retired or reused.
    pub(crate) fn clear_array_metadata(&mut self, array: RuntimeCellHandle) {
        let identity = array.as_ptr() as usize;
        self.array_element_aliases.remove(&identity);
        self.array_cursors.remove(&identity);
    }

    /// Copies array references and cursor state to an independently owned array box.
    pub(crate) fn copy_array_metadata(&mut self, source: RuntimeCellHandle, target: RuntimeCellHandle) {
        let source_id = source.as_ptr() as usize;
        let target_id = target.as_ptr() as usize;
        if source_id == target_id { return; }
        let cursor = self.array_cursors.get(&source_id).copied();
        self.clone_array_element_aliases(source, target, None);
        self.array_cursors.remove(&target_id);
        if let Some(cursor) = cursor {
            self.array_cursors.insert(target_id, cursor);
        }
    }

    /// Drops only retired allocation records, preserving any newer allocation at the same address.
    fn prune_retired_array_aliases(&mut self) {
        for address in self.array_reference_retirements.take() {
            if self.array_element_aliases.get(&address).is_some_and(|aliases| !aliases.lifetime.is_live()) {
                self.array_element_aliases.remove(&address);
            }
        }
    }

    /// Marks one eval object storage slot as initialized.
    pub fn mark_dynamic_property_initialized(
        &mut self,
        identity: u64,
        storage_property_name: &str,
    ) {
        #[cfg(not(test))]
        if let Some(owner) = dynamic_object_foreign_owner_mut(self, identity) {
            owner.mark_dynamic_property_initialized(identity, storage_property_name);
            return;
        }
        self.dynamic_initialized_properties
            .insert((identity, storage_property_name.to_string()));
    }

    /// Marks one eval object storage slot as uninitialized.
    pub fn mark_dynamic_property_uninitialized(
        &mut self,
        identity: u64,
        storage_property_name: &str,
    ) {
        #[cfg(not(test))]
        if let Some(owner) = dynamic_object_foreign_owner_mut(self, identity) {
            owner.mark_dynamic_property_uninitialized(identity, storage_property_name);
        }
        self.dynamic_initialized_properties
            .remove(&(identity, storage_property_name.to_string()));
    }

    /// Returns whether one eval object storage slot is known to be initialized.
    pub fn dynamic_property_is_initialized(
        &self,
        identity: u64,
        storage_property_name: &str,
    ) -> bool {
        if self
            .dynamic_initialized_properties
            .contains(&(identity, storage_property_name.to_string()))
        {
            return true;
        }
        #[cfg(not(test))]
        {
            dynamic_object_foreign_owner(self, identity).is_some_and(|owner| {
                owner
                    .dynamic_initialized_properties
                    .contains(&(identity, storage_property_name.to_string()))
            })
        }
        #[cfg(test)]
        {
            false
        }
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
        let mut initialized = self
            .dynamic_initialized_properties
            .iter()
            .filter_map(|(identity, property)| {
                (*identity == source_identity).then(|| property.clone())
            })
            .collect::<Vec<_>>();
        // A source another context built keeps its initialization marks THERE, as its property
        // values are: cloning it from here copied the values and no marks, so every typed property
        // of the clone read as uninitialized. Carbon's `instance()` clones like that, and the clone
        // failed on its own `?WrapperClock $clock = null`.
        #[cfg(not(test))]
        if let Some(owner) = dynamic_object_foreign_owner(self, source_identity) {
            initialized.extend(owner.dynamic_initialized_properties.iter().filter_map(
                |(identity, property)| (*identity == source_identity).then(|| property.clone()),
            ));
        }
        for property in initialized {
            self.mark_dynamic_property_initialized(clone_identity, &property);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an eval object keeps its metadata context alive until its final release.
    #[test]
    fn dynamic_object_retains_context_until_forget() {
        let mut context = ElephcEvalContext::new();
        context.register_dynamic_object(0xCAFE, "DeferredObject");
        context.register_dynamic_object(0xCAFE, "DeferredObject");

        assert!(!context.request_retained_context_free());
        let (properties, should_finalize) = context.forget_dynamic_object(0xCAFE);

        assert!(properties.is_empty());
        assert!(should_finalize);
    }
}

/// One live `ArrayIterator` instance's backing array and cursor.
///
/// The array is OWNED: it is retained on the way in and given back when the object is forgotten.
#[derive(Debug, Clone, Copy)]
pub struct EvalArrayIteratorState {
    pub(super) storage: RuntimeCellHandle,
    pub(super) position: usize,
}

/// Returns PHP's own parent and interfaces for one builtin iterator or container class.
///
/// The interfaces are listed TRANSITIVELY (`Iterator` brings `Traversable`) but the parent's are
/// not: `eval_builtin_class_is_a` walks the parent chain. These are fixed facts of PHP 8, not
/// of any program, and they matter to a declaration: `getIterator(): \EmptyIterator` is php's
/// covariant narrowing of `IteratorAggregate::getIterator(): Traversable`, and a class the
/// interpreter could not relate to `Traversable` was refused at declaration.
fn eval_builtin_class_hierarchy(class_name: &str) -> Option<(Option<&'static str>, &'static [&'static str])> {
    const ITERATOR: &[&str] = &["Iterator", "Traversable"];
    const OUTER: &[&str] = &["OuterIterator", "Iterator", "Traversable"];
    const RECURSIVE_OUTER: &[&str] = &["RecursiveIterator", "OuterIterator", "Iterator", "Traversable"];
    const AGGREGATE: &[&str] = &["IteratorAggregate", "Traversable"];
    let entry: (Option<&'static str>, &'static [&'static str]) =
        match class_name.trim_start_matches('\\').to_ascii_lowercase().as_str() {
            "arrayiterator" => (
                None,
                &["SeekableIterator", "Iterator", "Traversable", "ArrayAccess", "Serializable", "Countable"],
            ),
            "recursivearrayiterator" => (Some("ArrayIterator"), &["RecursiveIterator"]),
            "arrayobject" => (
                None,
                &["IteratorAggregate", "Traversable", "ArrayAccess", "Serializable", "Countable"],
            ),
            "emptyiterator" | "generator" | "multipleiterator" => (None, ITERATOR),
            "iteratoriterator" | "recursiveiteratoriterator" => (None, OUTER),
            "filteriterator" | "limititerator" | "norewinditerator" | "infiniteiterator"
            | "appenditerator" => (Some("IteratorIterator"), &[]),
            "cachingiterator" => (
                Some("IteratorIterator"),
                &["ArrayAccess", "Countable", "Stringable"],
            ),
            "recursivecachingiterator" => (Some("CachingIterator"), RECURSIVE_OUTER),
            "callbackfilteriterator" | "regexiterator" => (Some("FilterIterator"), &[]),
            "recursivecallbackfilteriterator" => (Some("CallbackFilterIterator"), RECURSIVE_OUTER),
            "recursivefilteriterator" => (Some("FilterIterator"), RECURSIVE_OUTER),
            "parentiterator" => (Some("RecursiveFilterIterator"), &[]),
            "recursiveregexiterator" => (Some("RegexIterator"), RECURSIVE_OUTER),
            "recursivetreeiterator" => (Some("RecursiveIteratorIterator"), &[]),
            "spldoublylinkedlist" => (
                None,
                &["Iterator", "Traversable", "Countable", "ArrayAccess", "Serializable"],
            ),
            "splqueue" | "splstack" => (Some("SplDoublyLinkedList"), &[]),
            "splobjectstorage" => (
                None,
                &["Countable", "Iterator", "Traversable", "Serializable", "ArrayAccess"],
            ),
            "splfixedarray" => (
                None,
                &["IteratorAggregate", "Traversable", "ArrayAccess", "Countable", "JsonSerializable"],
            ),
            "splheap" | "splpriorityqueue" => (None, &["Iterator", "Traversable", "Countable"]),
            "splminheap" | "splmaxheap" => (Some("SplHeap"), &[]),
            "splfileinfo" => (None, &["Stringable"]),
            "directoryiterator" => (
                Some("SplFileInfo"),
                &["SeekableIterator", "Iterator", "Traversable"],
            ),
            "filesystemiterator" => (Some("DirectoryIterator"), &[]),
            "recursivedirectoryiterator" => (Some("FilesystemIterator"), &["RecursiveIterator"]),
            "globiterator" => (Some("FilesystemIterator"), &["Countable"]),
            "splfileobject" => (
                Some("SplFileInfo"),
                &["RecursiveIterator", "SeekableIterator", "Iterator", "Traversable"],
            ),
            "spltempfileobject" => (Some("SplFileObject"), &[]),
            "dateperiod" => (None, AGGREGATE),
            "weakmap" => (None, &["ArrayAccess", "Countable", "IteratorAggregate", "Traversable"]),
            _ => return None,
        };
    Some(entry)
}

/// Answers whether one PHP builtin class is, or descends from, a class-like name.
///
/// `None` means the class is not in the builtin table, so the caller keeps its own answer.
pub fn eval_builtin_class_is_a(class_name: &str, target_class: &str) -> Option<bool> {
    let target = target_class.trim_start_matches('\\');
    let mut current = class_name.trim_start_matches('\\').to_string();
    eval_builtin_class_hierarchy(&current)?;
    loop {
        if current.eq_ignore_ascii_case(target) {
            return Some(true);
        }
        let Some((parent, interfaces)) = eval_builtin_class_hierarchy(&current) else {
            return Some(false);
        };
        if interfaces.iter().any(|name| name.eq_ignore_ascii_case(target)) {
            return Some(true);
        }
        let Some(parent) = parent else {
            return Some(false);
        };
        current = parent.to_string();
    }
}

/// Returns whether `ArrayIterator` is, or descends from, one class-like name.
///
/// Kept beside the state table rather than in the interpreter, because both the `instanceof`
/// answer and the declared-return-type check need it and neither owns the other.
pub fn eval_array_iterator_class_is_a(target_class: &str) -> bool {
    let target = target_class.trim_start_matches('\\');
    [
        "ArrayIterator",
        "Iterator",
        "Traversable",
        "Countable",
        "ArrayAccess",
        "SeekableIterator",
    ]
    .iter()
    .any(|name| name.eq_ignore_ascii_case(target))
}

/// Borrows the eval context that built one dynamic object, when it is not the asking context.
///
/// An object outlives the context that created it, and the creator is not always the context
/// that later reads it. `unserialize` is the sharpest case: the generated decoder has no context
/// at all, so its eval-object hook hydrates into the shared null-handle context, and the request
/// context that then calls a method on the result finds none of its per-object state.
/// `dynamic_object_class` has answered across contexts since eval generators landed; the
/// property overlay has to answer the same way or the object arrives with its class intact and
/// every slot empty.
///
/// Returns `None` for the asking context itself, so a caller can consult its own tables first and
/// reach here only for an object it did not build.
#[cfg(not(test))]
pub(super) fn dynamic_object_foreign_owner<'asking>(
    asking: &'asking ElephcEvalContext,
    identity: u64,
) -> Option<&'asking ElephcEvalContext> {
    let owner = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)?;
    if std::ptr::eq(owner.cast_const(), std::ptr::from_ref(asking)) {
        return None;
    }
    let owner = unsafe { owner.as_ref()? };
    (owner.abi_version() == ABI_VERSION).then_some(owner)
}

/// Returns the owning context of an object another context built, for writing its state.
///
/// The owner is a DIFFERENT context than `asking` (the same one answers `None`), so the returned
/// reference never aliases the caller's own `&mut self`.
#[cfg(not(test))]
pub(super) fn dynamic_object_foreign_owner_mut<'owner>(
    asking: &ElephcEvalContext,
    identity: u64,
) -> Option<&'owner mut ElephcEvalContext> {
    let owner = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)?;
    if std::ptr::eq(owner.cast_const(), std::ptr::from_ref(asking)) {
        return None;
    }
    let owner = unsafe { owner.as_mut()? };
    (owner.abi_version() == ABI_VERSION).then_some(owner)
}
