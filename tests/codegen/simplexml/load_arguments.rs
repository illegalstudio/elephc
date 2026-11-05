//! Purpose:
//! Covers SimpleXML argument overflow before XML parsing or file I/O.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on Linux and macOS.
//!
//! Key details:
//! - AOT and opaque eval preserve exception types, messages, and signed C-int boundaries.

use crate::support::*;

/// Invalid options throw before parsing in both modes, while the C-int endpoints remain valid.
#[test]
fn test_simplexml_load_argument_overflow_bundle() {
    if skip_without_xml_native("test_simplexml_load_argument_overflow_bundle") { return; }
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool { echo 'unexpected-warning|'; return true; });
function checkLoadOptions(int $options): void {
    try { simplexml_load_string('<broken>', options: $options); echo 'accepted-string|'; }
    catch (ValueError $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    try { simplexml_load_file('load-options-missing.xml', options: $options); echo 'accepted-file|'; }
    catch (ValueError $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    try { new SimpleXMLElement('<broken>', $options); echo 'accepted-constructor|'; }
    catch (Exception $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    try { new SimpleXMLElement('load-options-missing.xml', $options, true); echo 'accepted-url|'; }
    catch (Exception $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
}
foreach ([2147483648, -2147483649, PHP_INT_MAX, PHP_INT_MIN] as $options) { checkLoadOptions($options); }
file_put_contents('load-options.xml', '<r/>');
foreach ([2147483647, -2147483648] as $options) {
    $string = simplexml_load_string('<r/>', null, $options);
    $file = simplexml_load_file('load-options.xml', null, $options);
    $constructed = new SimpleXMLElement('<r/>', $options);
    $url = new SimpleXMLElement('load-options.xml', $options, true);
    echo $string->getName(), ':', $file->getName(), ':', $constructed->getName(), ':', $url->getName(), '|';
    unset($string, $file, $constructed, $url);
}
unlink('load-options.xml');
restore_error_handler();
unset($options, $error);
"#;
    let invalid = "ValueError:simplexml_load_string(): Argument #3 ($options) is too large|ValueError:simplexml_load_file(): Argument #3 ($options) is too large|Exception:SimpleXMLElement::__construct(): Argument #2 ($options) is invalid|Exception:SimpleXMLElement::__construct(): Argument #2 ($options) is invalid|";
    let expected = format!("{}r:r:r:r|r:r:r:r|", invalid.repeat(4));
    let evaluated = source.strip_prefix("<?php").unwrap()
        .replace("checkLoadOptions", "checkEvalLoadOptions")
        .replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $activate = new SimpleXMLElement('<activate/>'); unset($activate); $source = $argc > 0 ? '{evaluated}' : ''; eval($source); unset($source);");
    let cases = [
        BundleCase { name: "load_options_aot", source, expected: &expected },
        BundleCase { name: "load_options_eval", source: &eval, expected: &expected },
    ];
    let run = || assert_bundle_with(&cases, |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
    run();
    without_ir_opt(run);
}
