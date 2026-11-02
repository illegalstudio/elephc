//! Source snapshots from autoload roots and their nested includes.

use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("elephc_autoload_units_{}_{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

#[test]
fn source_units_preserve_autoload_root_and_nested_input() {
    let fixture = Fixture::new();
    let root = fixture.0.join("root.php");
    let nested = fixture.0.join("nested.php");
    let root_text = "<?php namespace UnitFixture; require __DIR__.'/nested.php'; class Root {} return $caller;";
    let nested_text = "<?php namespace UnitFixture; interface Nested {} echo 'side effect';";
    std::fs::write(&root, root_text).unwrap();
    std::fs::write(&nested, nested_text).unwrap();

    let (_, _, sources) = load_autoloaded_file(&root, &fixture.0, &HashSet::new()).unwrap();
    assert_eq!(sources.source_units.len(), 2);
    assert_eq!(&*sources.source_units[&root.canonicalize().unwrap()].source, root_text);
    assert_eq!(&*sources.source_units[&nested.canonicalize().unwrap()].source, nested_text);
    assert!(sources.class_likes.contains_key(&crate::names::php_symbol_key("UnitFixture\\Root")));
    assert!(sources.class_likes.contains_key(&crate::names::php_symbol_key("UnitFixture\\Nested")));
}

#[test]
fn source_units_reject_conflicting_autoload_merges() {
    let path = PathBuf::from("/source/module.php");
    let make = |text: &str| {
        let mut sources = DeclarationSourceFiles::default();
        sources.source_units.insert(path.clone(), crate::resolver::SourceUnit {
            canonical_path: path.clone(),
            mode: crate::source::SourceMode::Php,
            source: std::sync::Arc::from(text),
        });
        sources
    };
    let mut sources = make("<?php echo 1;");
    sources.extend(make("<?php echo 1;")).unwrap();
    assert!(sources.extend(make("<?php echo 2;")).is_err());
    assert_eq!(&*sources.source_units[&path].source, "<?php echo 1;");
}
