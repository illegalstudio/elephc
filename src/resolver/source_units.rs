//! Immutable physical-source snapshots collected independently of execution state.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::errors::CompileError;
use crate::source::SourceMode;
use crate::span::Span;

/// Input retained for a reusable source entry, before caller-specific resolution.
#[derive(Clone, Debug)]
pub struct SourceUnit {
    /// Physical identity; display strings and assembly labels are not identity keys.
    pub canonical_path: PathBuf,
    /// The language mode used when this source was parsed.
    pub mode: SourceMode,
    /// Losslessly decoded lexer input, shared without retaining a second full AST.
    pub source: Arc<str>,
}

impl SourceUnit {
    /// Merges one immutable input without silently replacing a different version.
    pub(crate) fn insert_into(
        self,
        units: &mut BTreeMap<PathBuf, Self>,
        span: Span,
    ) -> Result<(), CompileError> {
        if let Some(previous) = units.get(&self.canonical_path) {
            if previous.mode != self.mode || previous.source != self.source {
                return Err(CompileError::new(span, &format!(
                    "Conflicting source content or mode during compilation: '{}'", self.canonical_path.display(),
                )));
            }
            return Ok(());
        }
        units.insert(self.canonical_path.clone(), self);
        Ok(())
    }
}

/// Compilation metadata shared across isolated namespace/constant resolver scopes.
/// Collecting a unit neither includes the file nor activates its declarations.
#[derive(Clone, Default)]
pub(super) struct SourceUnitCollector(Arc<Mutex<BTreeMap<PathBuf, SourceUnit>>>);

impl SourceUnitCollector {
    pub(super) fn record(&self, unit: SourceUnit, span: Span) -> Result<(), CompileError> {
        let mut units = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        unit.insert_into(&mut units, span)
    }

    pub(super) fn snapshot(&self) -> BTreeMap<PathBuf, SourceUnit> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("elephc_source_units_{}_{nonce}", std::process::id()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn source_units_keep_canonical_paths_in_rewritten_include_markers() {
        use crate::parser::ast::StmtKind;
        let fixture = Fixture::new();
        let path = fixture.0.join("module.php");
        std::fs::write(&path, "<?php echo 'included';").unwrap();
        let canonical = path.canonicalize().unwrap();
        for keyword in ["include", "include_once"] {
            let text = format!("<?php {keyword} 'module.php';");
            let tokens = crate::lexer::tokenize(&text).unwrap();
            let parsed = crate::parser::parse(&tokens).unwrap();
            let (program, _, sources) = super::super::resolve_collecting_includes_with_defines_and_sources(
                parsed, &fixture.0, &crate::fast_hash::FastSet::default(),
            ).unwrap();
            let program = crate::conditional::apply(program, &crate::fast_hash::FastSet::default());
            let program = crate::magic_constants::substitute_file_and_scope_constants(program, &fixture.0.join("main.php"));
            let program = crate::name_resolver::resolve(program).unwrap();
            let program = crate::autoload::collect_aliases(program).unwrap();
            let program = crate::optimize::fold_constants(program);
            let paths: Vec<_> = program.iter().filter_map(|stmt| match &stmt.kind {
                StmtKind::IncludeOnceMark { source_path }
                | StmtKind::IncludeOnceGuard { source_path, .. } => Some(source_path),
                _ => None,
            }).collect();
            assert_eq!(paths, vec![&canonical], "{keyword}");
            assert!(sources.source_units.contains_key(paths[0]));
            let catalog = crate::ir::SourceCatalog::from_units(sources.source_units.into_values()).unwrap();
            let id = catalog.id_for_path(&canonical).unwrap();
            let target = crate::codegen::platform::Target::detect_host();
            let checked = crate::types::check_with_target(&program, target).unwrap();
            let module = crate::ir_lower::lower_program_with_source_catalog(
                &program, &checked, target, &fixture.0.join("main.php"), false, catalog,
            ).unwrap();
            assert!(module.functions.iter().flat_map(|function| &function.instructions).any(|inst| {
                matches!(inst.immediate, Some(crate::ir::Immediate::Source(found)) if found == id)
            }), "resolved include must carry its typed source identity");
        }
    }

    #[test]
    fn resolver_preserves_source_before_return_discard_and_declaration_hoisting() {
        let fixture = Fixture::new();
        let source = "<?php interface SnapshotProtocol {} return $caller;";
        let path = fixture.0.join("module.php");
        std::fs::write(&path, source).unwrap();
        let tokens = crate::lexer::tokenize("<?php function choose($caller) { if ($caller) { include 'module.php'; } }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let (_, _, sources) = super::super::resolve_collecting_includes_with_defines_and_sources(
            program, &fixture.0, &crate::fast_hash::FastSet::default(),
        ).unwrap();
        assert_eq!(sources.source_units.len(), 1);
        let captured = &sources.source_units[&path.canonicalize().unwrap()];
        assert_eq!(&*captured.source, source);
        assert_eq!(captured.mode, SourceMode::Php);
    }

    fn unit(path: &str, source: &str) -> SourceUnit {
        SourceUnit {
            canonical_path: PathBuf::from(path),
            mode: SourceMode::Php,
            source: Arc::from(source),
        }
    }

    #[test]
    fn isolated_scopes_share_snapshots_without_copying_source_text() {
        let collector = SourceUnitCollector::default();
        let isolated = collector.clone();
        let original = unit("/source/unit.php", "<?php return $caller;");
        isolated.record(original.clone(), Span::dummy()).unwrap();
        let snapshots = collector.snapshot();
        let captured = snapshots.get(&original.canonical_path).unwrap();
        assert!(Arc::ptr_eq(&original.source, &captured.source));
    }

    #[test]
    fn source_identity_is_path_based_and_iteration_is_deterministic() {
        let collector = SourceUnitCollector::default();
        collector.record(unit("/source/b.php", "<?php echo 1;"), Span::dummy()).unwrap();
        collector.record(unit("/source/a.php", "<?php echo 1;"), Span::dummy()).unwrap();
        collector.record(unit("/source/a.php", "<?php echo 1;"), Span::dummy()).unwrap();
        assert_eq!(collector.snapshot().keys().cloned().collect::<Vec<_>>(),
            vec![PathBuf::from("/source/a.php"), PathBuf::from("/source/b.php")]);
    }

    #[test]
    fn conflicting_input_does_not_replace_a_captured_unit() {
        let collector = SourceUnitCollector::default();
        collector.record(unit("/source/unit.php", "<?php echo 1;"), Span::dummy()).unwrap();
        assert!(collector.record(unit("/source/unit.php", "<?php echo 2;"), Span::dummy()).is_err());
        assert_eq!(&*collector.snapshot()[&PathBuf::from("/source/unit.php")].source, "<?php echo 1;");
    }
}
