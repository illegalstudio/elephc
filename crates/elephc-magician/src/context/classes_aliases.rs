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
        #[cfg(not(test))]
        register_global_eval_class(&class);
        self.classes.insert(key, class);
        true
    }

    /// Imports eval-declared process-global class-like metadata not yet known by this context.
    #[cfg(not(test))]
    pub fn sync_global_eval_classes(&mut self) {
        let Ok(registry) = global_eval_classes().lock() else {
            return;
        };
        for name in &registry.declared_class_names {
            let key = normalize_class_name(name);
            if self.classes.contains_key(&key)
                || self.interfaces.contains_key(&key)
                || self.traits.contains_key(&key)
                || self.enums.contains_key(&key)
                || self.class_aliases.contains_key(&key)
            {
                continue;
            }
            let Some(class) = registry.classes.get(&key).cloned() else {
                continue;
            };
            self.declared_class_names.push(class.name().to_string());
            self.classes.insert(key, class);
        }
        for name in &registry.declared_interface_names {
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
            self.declared_interface_names
                .push(interface.name().to_string());
            self.interfaces.insert(key, interface);
        }
        for name in &registry.declared_trait_names {
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
            self.declared_trait_names
                .push(trait_decl.name().to_string());
            self.traits.insert(key, trait_decl);
        }
        for name in &registry.declared_enum_names {
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
            self.declared_class_names
                .push(enum_decl.name().trim_start_matches('\\').to_string());
            self.classes
                .insert(key.clone(), enum_decl.as_class_metadata());
            self.enums.insert(key, enum_decl);
        }
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
    pub fn declared_class_names(&self) -> &[String] {
        &self.declared_class_names
    }

    /// Registers a runtime-visible class or enum declaration name for `get_declared_classes()`.
    pub fn define_external_declared_class_name(&mut self, name: &str) -> bool {
        push_external_declared_name(&mut self.declared_class_names, name)
    }
}
