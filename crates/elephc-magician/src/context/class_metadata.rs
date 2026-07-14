//! Purpose:
//! Resolves class chains, inherited members, traits, interfaces, and compatibility requirements.
//!
//! Called from:
//! - Declaration validation, Reflection metadata, and runtime class checks.
//!
//! Key details:
//! - Traversals are cycle-safe and preserve PHP case-insensitive class-like semantics.

use super::*;

impl ElephcEvalContext {
    /// Returns eval-declared class metadata from parent to child for construction.
    pub fn class_chain(&self, name: &str) -> Vec<EvalClass> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        self.collect_class_chain(name, &mut chain, &mut seen);
        chain
    }

    /// Collects one eval-declared class ancestry chain without following cycles.
    pub(super) fn collect_class_chain(
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
    pub(super) fn class_or_interface_constant(
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
        let mut current = self
            .class(class_name)
            .and_then(EvalClass::parent)
            .map(str::to_string)
            .or_else(|| self.native_class_parent(class_name).map(str::to_string));
        let mut seen = HashSet::new();
        while let Some(parent) = current {
            let parent = self
                .resolve_class_name(&parent)
                .unwrap_or_else(|| parent.trim_start_matches('\\').to_string());
            let key = normalize_class_name(&parent);
            if !seen.insert(key) {
                break;
            }
            if let Some(parent_class) = self.class(&parent) {
                parents.push(parent_class.name().trim_start_matches('\\').to_string());
                current = parent_class
                    .parent()
                    .map(str::to_string)
                    .or_else(|| self.native_class_parent(parent_class.name()).map(str::to_string));
            } else {
                parents.push(parent);
                current = parents
                    .last()
                    .and_then(|parent| self.native_class_parent(parent))
                    .map(str::to_string);
            }
        }
        parents
    }

    /// Returns the nearest runtime/AOT parent backing an eval class hierarchy.
    pub fn class_native_parent_name(&self, class_name: &str) -> Option<String> {
        let mut current = self
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(normalize_class_name(&current)) {
                return None;
            }
            let parent = self
                .class(&current)
                .and_then(EvalClass::parent)
                .map(str::to_string)
                .or_else(|| self.native_class_parent(&current).map(str::to_string))?;
            let parent = self
                .resolve_class_name(&parent)
                .unwrap_or_else(|| parent.trim_start_matches('\\').to_string());
            if self.class(&parent).is_none() {
                return Some(parent);
            }
            current = parent;
        }
    }

    /// Returns direct and inherited interface names for an eval-declared class.
    pub fn class_interface_names(&self, class_name: &str) -> Vec<String> {
        let mut interfaces = Vec::new();
        let mut seen = HashSet::new();
        let is_enum = self.enum_decl(class_name).is_some();
        for class in self.class_chain(class_name) {
            for interface in class.interfaces() {
                push_unique_class_name(interface, &mut interfaces, &mut seen);
                self.collect_class_interface_parent_names(
                    interface,
                    is_enum,
                    &mut interfaces,
                    &mut seen,
                );
            }
        }
        if let Some(enum_decl) = self.enum_decl(class_name) {
            push_unique_class_name("UnitEnum", &mut interfaces, &mut seen);
            if enum_decl.backing_type().is_some() {
                push_unique_class_name("BackedEnum", &mut interfaces, &mut seen);
            }
        }
        interfaces
    }

    /// Collects interface parents while preserving PHP enum marker interface ordering.
    pub(super) fn collect_class_interface_parent_names(
        &self,
        interface_name: &str,
        skip_enum_markers: bool,
        names: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        let Some(interface) = self.interface(interface_name) else {
            return;
        };
        for parent in interface.parents() {
            if skip_enum_markers && is_php_enum_marker_interface(parent) {
                continue;
            }
            push_unique_class_name(parent, names, seen);
            self.collect_class_interface_parent_names(parent, skip_enum_markers, names, seen);
        }
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

    /// Returns trait method aliases declared directly by an eval-declared class.
    pub fn class_trait_aliases(&self, class_name: &str) -> Vec<(String, String)> {
        let Some(class) = self.class(class_name) else {
            return Vec::new();
        };
        let mut aliases = Vec::new();
        for adaptation in class.trait_adaptations() {
            let EvalTraitAdaptation::Alias {
                trait_name,
                method,
                alias: Some(alias),
                ..
            } = adaptation
            else {
                continue;
            };
            let Some(source_trait) =
                self.class_trait_alias_source(class, trait_name.as_deref(), method)
            else {
                continue;
            };
            aliases.push((alias.clone(), format!("{source_trait}::{method}")));
        }
        aliases
    }

    /// Returns trait names used directly by an eval-declared trait.
    pub fn trait_trait_names(&self, trait_name: &str) -> Vec<String> {
        self.trait_decl(trait_name).map_or_else(Vec::new, |trait_decl| {
            let mut traits = Vec::new();
            let mut seen = HashSet::new();
            for used_trait in trait_decl.traits() {
                push_unique_class_name(used_trait, &mut traits, &mut seen);
            }
            traits
        })
    }

    /// Returns trait method aliases declared directly by an eval-declared trait.
    pub fn trait_trait_aliases(&self, trait_name: &str) -> Vec<(String, String)> {
        let Some(trait_decl) = self.trait_decl(trait_name) else {
            return Vec::new();
        };
        let mut aliases = Vec::new();
        for adaptation in trait_decl.trait_adaptations() {
            let EvalTraitAdaptation::Alias {
                trait_name,
                method,
                alias: Some(alias),
                ..
            } = adaptation
            else {
                continue;
            };
            let Some(source_trait) =
                self.trait_trait_alias_source(trait_decl, trait_name.as_deref(), method)
            else {
                continue;
            };
            aliases.push((alias.clone(), format!("{source_trait}::{method}")));
        }
        aliases
    }

    /// Resolves the trait name shown in `ReflectionClass::getTraitAliases()`.
    pub(super) fn class_trait_alias_source(
        &self,
        class: &EvalClass,
        explicit_trait: Option<&str>,
        method: &str,
    ) -> Option<String> {
        if let Some(trait_name) = explicit_trait {
            return Some(
                self.trait_decl(trait_name)
                    .map_or(trait_name, EvalTrait::name)
                    .trim_start_matches('\\')
                    .to_string(),
            );
        }
        class.traits().iter().find_map(|trait_name| {
            let trait_decl = self.trait_decl(trait_name)?;
            trait_decl
                .methods()
                .iter()
                .any(|candidate| candidate.name().eq_ignore_ascii_case(method))
                .then(|| trait_decl.name().trim_start_matches('\\').to_string())
            })
    }

    /// Resolves the trait name shown for a trait's internal `getTraitAliases()`.
    pub(super) fn trait_trait_alias_source(
        &self,
        trait_decl: &EvalTrait,
        explicit_trait: Option<&str>,
        method: &str,
    ) -> Option<String> {
        if let Some(trait_name) = explicit_trait {
            return Some(
                self.trait_decl(trait_name)
                    .map_or(trait_name, EvalTrait::name)
                    .trim_start_matches('\\')
                    .to_string(),
            );
        }
        trait_decl.traits().iter().find_map(|trait_name| {
            let used_trait_decl = self.trait_decl(trait_name)?;
            used_trait_decl
                .methods()
                .iter()
                .any(|candidate| candidate.name().eq_ignore_ascii_case(method))
                .then(|| used_trait_decl.name().trim_start_matches('\\').to_string())
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
    pub(super) fn collect_interface_constant_names(
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
    pub(super) fn collect_interface_parent_names(
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
    pub(super) fn collect_interface_method_requirements(
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
        self.interface_property_requirements_with_owners(interface_name)
            .into_iter()
            .map(|(_, property)| property)
            .collect()
    }

    /// Returns direct and inherited property contracts with their declaring interface.
    pub fn interface_property_requirements_with_owners(
        &self,
        interface_name: &str,
    ) -> Vec<(String, EvalInterfaceProperty)> {
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
    pub(super) fn collect_interface_property_requirements(
        &self,
        interface_name: &str,
        properties: &mut Vec<(String, EvalInterfaceProperty)>,
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
            if let Some((_, existing)) = properties
                .iter_mut()
                .find(|(_, existing)| existing.name() == property.name())
            {
                *existing = existing.merged_with(property);
            } else {
                properties.push((interface.name().to_string(), property.clone()));
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
    pub(super) fn class_has_valid_tostring(&self, class_name: &str) -> bool {
        self.class_method(class_name, "__toString")
            .is_some_and(|(_, method)| {
                method.visibility() == EvalVisibility::Public
                    && !method.is_static()
                    && !method.is_abstract()
                    && method.params().is_empty()
            })
    }
}
