//! Frozen, typed identities for the physical source inputs of one EIR module.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::resolver::SourceUnit;
use crate::span::Span;

/// Module-local source identity, distinct from literal-pool and declaration IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(u32);

impl SourceId {
    pub const fn from_raw(raw: u32) -> Self { Self(raw) }
    pub const fn as_raw(self) -> u32 { self.0 }
}

/// Immutable catalog: source IDs cannot change after instructions start using them.
#[derive(Clone, Default)]
pub struct SourceCatalog {
    units: Vec<SourceUnit>,
    ids: BTreeMap<PathBuf, SourceId>,
}

impl std::fmt::Debug for SourceCatalog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Module diagnostics must not dump every retained source body.
        formatter.debug_struct("SourceCatalog").field("ids", &self.ids).finish()
    }
}

impl SourceCatalog {
    /// Checks duplicate snapshots, then assigns IDs in canonical path order.
    pub fn from_units(units: impl IntoIterator<Item = SourceUnit>) -> Result<Self, CompileError> {
        let mut ordered = BTreeMap::new();
        for unit in units { unit.insert_into(&mut ordered, Span::dummy())?; }
        let mut catalog = Self::default();
        for (path, unit) in ordered {
            let raw = u32::try_from(catalog.units.len())
                .map_err(|_| CompileError::new(Span::dummy(), "Too many source units"))?;
            catalog.ids.insert(path, SourceId(raw));
            catalog.units.push(unit);
        }
        Ok(catalog)
    }

    pub fn id_for_path(&self, canonical_path: &Path) -> Option<SourceId> {
        self.ids.get(canonical_path).copied()
    }

    pub fn get(&self, id: SourceId) -> Option<&SourceUnit> {
        self.units.get(id.0 as usize)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (SourceId, &SourceUnit)> {
        self.units.iter().enumerate().map(|(index, unit)| (SourceId(index as u32), unit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::source::SourceMode;

    fn unit(path: impl Into<PathBuf>, text: &str) -> SourceUnit {
        SourceUnit { canonical_path: path.into(), mode: SourceMode::Php, source: text.into() }
    }

    #[test]
    fn source_catalog_handles_share_storage_across_module_clones() {
        let catalog = SourceCatalog::from_units([unit("/entry.php", "<?php echo 1;")]).unwrap();
        let module = crate::ir::Module::with_source_catalog(
            Target::new(Platform::MacOS, Arch::AArch64), catalog,
        );
        let first = module.shared_source_catalog().unwrap();
        let second = module.shared_source_catalog().unwrap();
        let cloned = module.clone().shared_source_catalog().unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert!(std::sync::Arc::ptr_eq(&first, &cloned));
        drop(module);
        assert_eq!(first.iter().len(), 1);
        assert_eq!(&*first.get(SourceId::from_raw(0)).unwrap().source, "<?php echo 1;");
    }

    #[test]
    fn source_catalog_is_frozen_on_empty_module_construction() {
        let catalog = SourceCatalog::from_units([unit("/entry.php", "<?php echo 1;")]).unwrap();
        let id = catalog.id_for_path(Path::new("/entry.php")).unwrap();
        let mut module = crate::ir::Module::with_source_catalog(
            Target::new(Platform::MacOS, Arch::AArch64), catalog,
        );
        assert!(module.functions.is_empty());
        assert_eq!(module.source_catalog().unwrap().id_for_path(Path::new("/entry.php")), Some(id));
        assert!(module.bind_source_units([unit("/replacement.php", "<?php")]).is_err());
        assert_eq!(&*module.source_catalog().unwrap().get(id).unwrap().source, "<?php echo 1;");
    }

    #[test]
    fn source_ids_are_independent_of_discovery_order() {
        let a = unit("/sources/a.php", "<?php echo 1;");
        let b = unit("/sources/b.php", "<?php echo 2;");
        let first = SourceCatalog::from_units([b.clone(), a.clone(), a.clone()]).unwrap();
        let second = SourceCatalog::from_units([a, b]).unwrap();
        for path in ["/sources/a.php", "/sources/b.php"] {
            let id = first.id_for_path(Path::new(path)).unwrap();
            assert_eq!(Some(id), second.id_for_path(Path::new(path)));
            assert_eq!(&*first.get(id).unwrap().source, &*second.get(id).unwrap().source);
        }
        assert_eq!(first.iter().len(), 2);
        assert!(first.get(SourceId::from_raw(u32::MAX)).is_none());
    }

    #[test]
    fn source_catalog_binding_is_atomic_and_cannot_renumber_ids() {
        let mut module = crate::ir::Module::new(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(module.bind_source_units([unit("/a.php", "first"), unit("/a.php", "changed")]).is_err());
        assert!(module.source_catalog().is_none());
        module.bind_source_units([unit("/b.php", "body")]).unwrap();
        assert!(module.bind_source_units([unit("/a.php", "new")]).is_err());
        assert_eq!(module.source_catalog().unwrap().id_for_path(Path::new("/b.php")).unwrap().as_raw(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn source_identity_preserves_non_utf8_path_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let a = PathBuf::from(std::ffi::OsString::from_vec(b"/source/\x80.php".to_vec()));
        let b = PathBuf::from(std::ffi::OsString::from_vec(b"/source/\x81.php".to_vec()));
        let catalog = SourceCatalog::from_units([unit(a.clone(), "same"), unit(b.clone(), "same")]).unwrap();
        assert_ne!(catalog.id_for_path(&a), catalog.id_for_path(&b));
    }
}
