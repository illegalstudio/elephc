//! Purpose:
//! Preserves temporary SimpleXML selections passed to reference parameters.
//!
//! Called from:
//! - Managed-libxml2 executable CI through the SimpleXML test module.
//!
//! Key details:
//! - Replacing a reference selection leaves XML intact while mutations through its view stay live.
//! - Native and opaque eval programs check properties, dimensions, casts, escaping and destruction.

use crate::support::*;

/// Native calls create missing paths before subsequent argument side effects.
#[test]
fn test_simplexml_reference_arguments_materialize_missing_paths_bundle() {
    if skip_without_xml_native("test_simplexml_reference_arguments_materialize_missing_paths_bundle") { return; }
    verify_reference_arguments_materialize_missing_paths(false);
}

/// Eval calls preserve the same six lifetime checkpoints within one catalog compile.
#[test]
fn test_simplexml_reference_arguments_materialize_missing_paths_eval_bundle() {
    if skip_without_xml_native("test_simplexml_reference_arguments_materialize_missing_paths_eval_bundle") { return; }
    verify_reference_arguments_materialize_missing_paths(true);
}

/// Keeps every source case, argv checkpoint, expected value and strict heap assertion in both modes.
fn verify_reference_arguments_materialize_missing_paths(eval_mode: bool) {
    let body = r#"
function replaceMissingXmlSelection(&$value, string $observed): void {
    echo get_class($value), ':', $observed, '|'; $value = 'replacement';
}
function observeMissingXmlSelection(SimpleXMLElement $xml): string {
    return isset($xml->nested->child) ? 'created' : 'absent';
}
$xml = new SimpleXMLElement('<r><p>old</p><q>4</q></r>');
replaceMissingXmlSelection($xml->missing, 'first');
replaceMissingXmlSelection($xml->nested->child, observeMissingXmlSelection($xml));
$name = 'dynamic'; replaceMissingXmlSelection($xml->{$name}, 'name');
echo (int)isset($xml->missing), ':', (int)isset($xml->nested->child), ':', (int)isset($xml->dynamic), '|';
echo (string)$xml->missing, ':', (string)$xml->nested->child, ':', (string)$xml->dynamic, '|';
echo (int)settype($xml->q, 'integer'), ':', (string)$xml->q, '|';
class MissingXmlSelectionConsumer {
    public function __construct(&$value) { $value = 'ctor'; }
    public function change(&$value): void { $value = 'method'; }
    public static function changeStatic(&$value): void { $value = 'static'; }
    public function __invoke(&$value): void { $value = 'invoke'; }
}
$consumer = new MissingXmlSelectionConsumer($xml->ctor);
$consumer->change($xml->method);
MissingXmlSelectionConsumer::changeStatic($xml->stat);
$consumer($xml->invoke);
$callback = $consumer->change(...); $callback($xml->bound);
$callback = MissingXmlSelectionConsumer::changeStatic(...); $callback($xml->boundStatic);
$callback = function (&$value): void { $value = 'closure'; }; $callback($xml->closure);
echo (int)isset($xml->ctor), (int)isset($xml->method), (int)isset($xml->stat), (int)isset($xml->invoke),
    (int)isset($xml->bound), (int)isset($xml->boundStatic), (int)isset($xml->closure), '|';
function readMissingXmlSelection($value): void { echo get_class($value), '|'; }
readMissingXmlSelection($xml->unread); echo (int)isset($xml->unread), '|';
function replaceMissingVariadicXmlSelection(&...$values): void {
    if (array_key_exists('slot', $values)) { $values['slot'] = 'replacement'; }
    else { $values[0] = 'replacement'; }
}
replaceMissingVariadicXmlSelection($xml->tail);
replaceMissingVariadicXmlSelection($xml->tail[0]);
$callback = replaceMissingVariadicXmlSelection(...); $callback(slot: $xml->namedTail);
echo (int)isset($xml->tail), (int)isset($xml->namedTail), '|';
function replaceOneMissingXmlSelection(&$value): void { $value = 'replacement'; }
$dimensionCallback = replaceOneMissingXmlSelection(...);
$dimensionCallback($xml->positionalDimension[0]);
$dimensionCallback(value: $xml->namedDimension[0]);
echo (int)isset($xml->positionalDimension), (int)isset($xml->namedDimension), '|';
function runtimeMissingXmlReference(&$value, string $after): void { echo $after, '|'; $value = 'replacement'; }
function runtimeMissingXmlValue($value, string $after): void { echo $after, '|'; }
function runtimeMissingXmlCall(mixed $callback, SimpleXMLElement $root): void {
    $callback($root->runtime, isset($root->runtime) ? 'created' : 'absent');
    $callback(value: $root->runtimeNamed[0], after: isset($root->runtimeNamed) ? 'created' : 'absent');
    $name = 'runtimeDynamic';
    $callback($root->{$name}, isset($root->{$name}) ? 'created' : 'absent');
    echo (int)isset($root->runtime), (int)isset($root->runtimeNamed), (int)isset($root->{$name}), '|';
}
runtimeMissingXmlCall(runtimeMissingXmlReference(...), new SimpleXMLElement('<r/>'));
runtimeMissingXmlCall(runtimeMissingXmlValue(...), new SimpleXMLElement('<r/>'));
function runtimeMissingXmlAfterEval(mixed $callback, SimpleXMLElement $root, string $source): void {
    eval($source);
    $callback($root->afterEval, isset($root->afterEval) ? 'created' : 'absent');
    $callback(value: $root->afterEvalNamed[0], after: isset($root->afterEvalNamed) ? 'created' : 'absent');
    echo (int)isset($root->afterEval), (int)isset($root->afterEvalNamed), '|';
}
$barrier = $argc > 0 ? '' : 'echo "unexpected";';
runtimeMissingXmlAfterEval(runtimeMissingXmlReference(...), new SimpleXMLElement('<r/>'), $barrier);
runtimeMissingXmlAfterEval(runtimeMissingXmlValue(...), new SimpleXMLElement('<r/>'), $barrier);
"#;
    let expected = "SimpleXMLElement:first|SimpleXMLElement:created|SimpleXMLElement:name|1:1:1|::|1:4|1111111|SimpleXMLElement|0|11|11|created|created|created|111|absent|absent|absent|000|created|created|11|absent|absent|00|";
    let mut body = body.to_string();
    for (argc, marker) in [
        (2, "replaceMissingVariadicXmlSelection($xml->tail);"),
        (3, "replaceMissingVariadicXmlSelection($xml->tail[0]);"),
        (4, "$callback = replaceMissingVariadicXmlSelection(...);"),
        (5, "echo (int)isset($xml->tail), (int)isset($xml->namedTail), '|';"),
        (6, "runtimeMissingXmlCall(runtimeMissingXmlReference(...), new SimpleXMLElement('<r/>'));"),
    ] {
        assert_eq!(body.matches(marker).count(), 1, "{marker}");
        body = body.replace(marker, &format!("if ($argc === {argc}) {{ return; }}\n{marker}"));
    }
    let native = format!("<?php {body}");
    let encoded = body.replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $xmlCapability = new SimpleXMLElement('<r/>'); unset($xmlCapability); $code = $argc > 0 ? '{encoded}' : ''; eval($code); unset($code);");
    let before_variadic = expected.split_inclusive('|').take(9).collect::<String>();
    let before_runtime = expected.split_inclusive('|').take(11).collect::<String>();
    let expected_windows = [expected, &before_variadic, &before_variadic, &before_variadic, &before_variadic, &before_runtime];
    let arguments = ["checkpoint"; 5];
    let argv = (0..=5).map(|length| &arguments[..length]).collect::<Vec<_>>();
    let mode = usize::from(eval_mode);
    let source = if eval_mode { &eval } else { &native };
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(source, &argv);
    preserve_stdout_mismatches(source, &assembly, &runs, &expected_windows);
    let mut failures = Vec::new();
    for (window, (out, expected)) in runs.iter().zip(expected_windows).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} window={window}: stdout={} stderr={}", out.stdout, out.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Native indirect calls inspect eval closure modes in the closure's owning context.
#[test]
fn test_simplexml_foreign_context_reference_arguments_bundle() {
    if skip_without_xml_native("test_simplexml_foreign_context_reference_arguments_bundle") { return; }
    let source = r#"<?php
function invokeForeignXmlReference(mixed $callback, SimpleXMLElement $root): void {
    $callback($root->missing);
    echo (int)isset($root->missing), '|';
}
$code = $argc > 0 ? '
$foreignReference = function (&$value): void { $value = "ref"; };
$foreignValue = function ($value): void {};
' : '';
eval($code);
invokeForeignXmlReference($foreignReference, new SimpleXMLElement('<r/>'));
invokeForeignXmlReference($foreignValue, new SimpleXMLElement('<r/>'));
unset($foreignReference, $foreignValue, $code);
"#;
    let (output, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&output), &["1|0|"]);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "1|0|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Reference arguments keep selected views without replaying a parent property store.
#[test]
fn test_simplexml_reference_argument_selections_bundle() {
    if skip_without_xml_native("test_simplexml_reference_argument_selections_bundle") { return; }
    let body = r#"
class ReferenceSelectionXml extends SimpleXMLElement {}
function replaceReferenceSelection(&$value): void { $value = "new"; }
function escapeReferenceSelection(&$value): callable {
    return function (bool $replace) use (&$value): mixed {
        if ($replace) { $value = "detached"; }
        return $value;
    };
}
function touchReferenceSelection(&$value): void { $value["id"] = "kept"; }
function throwReferenceSelection(&$value): void { $value = "throw"; throw new RuntimeException("stop"); }
function referenceSelectionWarning(int $severity, string $message): bool {
    if (!str_contains($message, 'must be passed by reference')) { throw new RuntimeException($message); }
    return true;
}
function exerciseReferenceSelection(SimpleXMLElement $root): void {
    replaceReferenceSelection($root->p); echo (string)$root->p, "|";
    $references = escapeReferenceSelection($root->p);
    $root->p = "changed"; echo (string)$references(false), "|";
    $references(true); echo (string)$root->p, "|";
    replaceReferenceSelection($root->p[0]); echo (string)$root->p, "|";
    replaceReferenceSelection($root->a->b); echo (string)$root->a->b, "|";
    $name = "p"; replaceReferenceSelection($root->{$name}); echo (string)$root->p, "|";
    echo (int)settype($root->q, "integer"), ":", (string)$root->q, "|";
    echo (int)settype(type: "integer", var: $root->a->b), ":", (string)$root->a->b, "|";
    touchReferenceSelection($root->p); echo (string)$root->p["id"], "|";
    try { throwReferenceSelection($root->p); }
    catch (RuntimeException $error) { echo $error->getMessage(), ":", (string)$root->p, "|"; unset($error); }
    set_error_handler(referenceSelectionWarning(...));
    call_user_func('replaceReferenceSelection', $root->readOnlyCall);
    call_user_func_array('replaceReferenceSelection', [$root->readOnlyArray]);
    restore_error_handler();
    echo (int)isset($root->readOnlyCall), ':', (int)isset($root->readOnlyArray), '|';
    unset($references);
}
$base = new SimpleXMLElement("<r><p>old</p><q>4</q><a><b>8</b></a></r>");
exerciseReferenceSelection($base); unset($base);
$subclass = new ReferenceSelectionXml("<r><p>old</p><q>4</q><a><b>8</b></a></r>");
exerciseReferenceSelection($subclass); unset($subclass);
class ReferenceSelectionDestructorXml extends SimpleXMLElement {
    public function __destruct() { echo "drop:", $this->getName(), "|"; }
}
function makeReferenceSelection(): ReferenceSelectionDestructorXml {
    return new ReferenceSelectionDestructorXml("<r><p>old</p></r>");
}
function consumeReferenceSelection(&$value): void { echo "call|"; $value = "new"; echo "tail|"; }
consumeReferenceSelection(makeReferenceSelection()->p);
$keptReferenceRoot = new ReferenceSelectionDestructorXml('<r/>');
consumeReferenceSelection($keptReferenceRoot->p);
echo 'after|'; unset($keptReferenceRoot); echo 'end|';
$keptReferenceRoot = new ReferenceSelectionDestructorXml('<r/>');
consumeReferenceSelection($keptReferenceRoot->a->b);
echo 'after|'; unset($keptReferenceRoot); echo 'end|';
$keptReferenceRoot = new ReferenceSelectionDestructorXml('<r/>');
$referenceName = 'p'; consumeReferenceSelection($keptReferenceRoot->{$referenceName});
echo 'after|'; unset($keptReferenceRoot); echo 'end|';
"#;
    let native = format!("<?php {body}");
    let encoded = body.replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $capability = new SimpleXMLElement('<r/>'); unset($capability); $source = $argc > 0 ? '{encoded}' : ''; eval($source); unset($source);");
    let row = "old|changed|changed|changed|8|changed|1:4|1:8|kept|stop:changed|0:0|";
    let expected = row.repeat(2) + "drop:r|call|drop:p|tail|call|drop:p|tail|after|drop:r|end|call|drop:b|tail|after|drop:r|drop:a|end|call|drop:p|tail|after|drop:r|end|";
    for (mode, source) in [native, eval].iter().enumerate() {
        let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
        preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&out), &[&expected]);
        assert!(out.success, "mode={mode}: stdout={} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "mode={mode}");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={mode}: {}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "mode={mode}: {}", out.stderr);
    }
}
