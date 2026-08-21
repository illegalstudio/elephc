//! Purpose:
//! Oracle-pinned reflection coverage for the complete public PHP 8.5 DOM,
//! libxml, and SimpleXML surface.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::dom_reflection_surface`.
//!
//! Key details:
//! - Expectations were captured with PHP 8.5.8 and libxml2 2.15.3.
//! - Reflection checks make hierarchy, signatures, virtual-property metadata, and
//!   extension registration independently observable from native DOM behaviour.

use crate::support::compile_and_run;

/// Verifies legacy and modern DOM hierarchy, interfaces, finality, construction, and cloning metadata.
#[test]
fn dom_reflection_class_hierarchy_matches_php_8_5_8() {
    let output = compile_and_run(
        r#"<?php
function flag(bool $value): string {
    return $value ? "yes" : "no";
}

function class_row(string $name): void {
    $reflection = new ReflectionClass($name);
    $parent = $reflection->getParentClass();
    echo $name, "|", flag($reflection->isInterface()), "|";
    echo flag($reflection->isAbstract()), "|", flag($reflection->isFinal()), "|";
    echo flag($reflection->isInstantiable()), "|", flag($reflection->isCloneable()), "|";
    echo ($parent ? $parent->getName() : "-"), "|";
    echo implode(",", $reflection->getInterfaceNames()), "\n";
}

class_row("DOMDocument");
class_row("DOMNode");
class_row("Dom\\Document");
class_row("Dom\\XMLDocument");
class_row("Dom\\Element");
class_row("DOMNodeList");
class_row("DOMNamedNodeMap");
class_row("SimpleXMLElement");
class_row("SimpleXMLIterator");
class_row("DOMException");
"#,
    );

    assert_eq!(
        output,
        concat!(
            "DOMDocument|no|no|no|yes|yes|DOMNode|DOMParentNode\n",
            "DOMNode|no|no|no|yes|yes|-|\n",
            "Dom\\Document|no|yes|no|no|no|Dom\\Node|Dom\\ParentNode\n",
            "Dom\\XMLDocument|no|no|yes|no|yes|Dom\\Document|Dom\\ParentNode\n",
            "Dom\\Element|no|no|no|no|yes|Dom\\Node|Dom\\ParentNode,Dom\\ChildNode\n",
            "DOMNodeList|no|no|no|yes|no|-|IteratorAggregate,Traversable,Countable\n",
            "DOMNamedNodeMap|no|no|no|yes|no|-|IteratorAggregate,Traversable,Countable\n",
            "SimpleXMLElement|no|no|no|yes|yes|-|Stringable,Countable,RecursiveIterator,Traversable,Iterator\n",
            "SimpleXMLIterator|no|no|no|yes|yes|SimpleXMLElement|Iterator,Traversable,RecursiveIterator,Countable,Stringable\n",
            "DOMException|no|no|yes|yes|no|Exception|Throwable,Stringable\n",
        ),
    );
}

/// Verifies reflection exposes PHP's parameter names, nullable/union returns, and readonly slots.
#[test]
fn dom_reflection_method_and_property_signatures_match_php_8_5_8() {
    let output = compile_and_run(
        r#"<?php
function method_row(string $class, string $method): void {
    $reflection = new ReflectionMethod($class, $method);
    echo $class, "::", $method, "|";
    echo $reflection->isStatic() ? "static" : "instance";
    echo "|", ($reflection->hasReturnType() ? $reflection->getReturnType() : "-");
    echo "|", $reflection->getNumberOfRequiredParameters(), "/";
    echo $reflection->getNumberOfParameters(), "|";
    foreach ($reflection->getParameters() as $parameter) {
        echo $parameter->getName(), ":";
        echo $parameter->hasType() ? $parameter->getType() : "-";
        echo ":", ($parameter->isOptional() ? "optional" : "required"), ":";
        echo $parameter->isPassedByReference() ? "ref" : "value", ";";
    }
    echo "\n";
}

function property_row(string $class, string $property): void {
    $reflection = new ReflectionProperty($class, $property);
    echo $class, "::$", $property, "|", $reflection->getType(), "|";
    echo $reflection->isReadOnly() ? "readonly" : "mutable";
    echo "|", ($reflection->isPublic() ? "public" : "nonpublic"), "\n";
}

method_row("DOMDocument", "loadXML");
method_row("Dom\\XMLDocument", "createFromString");
method_row("DOMXPath", "query");
method_row("SimpleXMLElement", "__toString");
method_row("SimpleXMLElement", "current");
property_row("DOMDocument", "documentElement");
property_row("Dom\\NamespaceInfo", "prefix");
property_row("Dom\\NamespaceInfo", "element");
property_row("DOMNodeList", "length");
"#,
    );

    assert_eq!(
        output,
        concat!(
            "DOMDocument::loadXML|instance|-|1/2|source:string:required:value;options:int:optional:value;\n",
            "Dom\\XMLDocument::createFromString|static|Dom\\XMLDocument|1/3|source:string:required:value;options:int:optional:value;overrideEncoding:?string:optional:value;\n",
            "DOMXPath::query|instance|-|1/3|expression:string:required:value;contextNode:?DOMNode:optional:value;registerNodeNS:bool:optional:value;\n",
            "SimpleXMLElement::__toString|instance|string|0/0|\n",
            "SimpleXMLElement::current|instance|-|0/0|\n",
            "DOMDocument::$documentElement|?DOMElement|mutable|public\n",
            "Dom\\NamespaceInfo::$prefix|?string|readonly|public\n",
            "Dom\\NamespaceInfo::$element|Dom\\Element|readonly|public\n",
            "DOMNodeList::$length|int|mutable|public\n",
        ),
    );
}

/// Verifies DOM's backed enum and the extension's classes, interfaces, traits, and ancestry probes.
#[test]
fn dom_reflection_enum_and_extension_existence_probes_match_php_8_5_8() {
    let output = compile_and_run(
        r#"<?php
$enum = new ReflectionEnum("Dom\\AdjacentPosition");
echo $enum->getName(), "|", ($enum->isBacked() ? "backed" : "unit"), "|";
echo $enum->getBackingType(), "|";
echo implode(",", array_map(
    fn($case) => $case->getName() . "=" . $case->getBackingValue(),
    $enum->getCases(),
)), "\n";

echo "exists|", extension_loaded("dom") ? "yes" : "no";
echo "|", extension_loaded("libxml") ? "yes" : "no";
echo "|", extension_loaded("SimpleXML") ? "yes" : "no";
echo "|", class_exists("DOMDocument") ? "yes" : "no";
echo "|", class_exists("Dom\\XMLDocument") ? "yes" : "no";
echo "|", class_exists("SimpleXMLElement") ? "yes" : "no";
echo "|", interface_exists("DOMParentNode") ? "yes" : "no";
echo "|", interface_exists("Dom\\ParentNode") ? "yes" : "no";
echo "|", trait_exists("DOMNode") ? "yes" : "no";
echo "|", is_subclass_of("DOMDocument", "DOMNode") ? "yes" : "no";
echo "|", is_subclass_of("Dom\\XMLDocument", "Dom\\Document") ? "yes" : "no", "\n";
"#,
    );

    assert_eq!(
        output,
        concat!(
            "Dom\\AdjacentPosition|backed|string|BeforeBegin=beforebegin,AfterBegin=afterbegin,BeforeEnd=beforeend,AfterEnd=afterend\n",
            "exists|yes|yes|yes|yes|yes|yes|yes|yes|no|yes|yes\n",
        ),
    );
}

/// Verifies every public DOM/libxml/SimpleXML function family retains names, types, and named arguments.
#[test]
fn dom_reflection_function_signatures_and_registration_match_php_8_5_8() {
    let output = compile_and_run(
        r#"<?php
function function_row(string $name): void {
    $reflection = new ReflectionFunction($name);
    echo $name, "|";
    echo $reflection->hasReturnType() ? $reflection->getReturnType() : "-";
    echo "|", $reflection->getNumberOfRequiredParameters(), "/";
    echo $reflection->getNumberOfParameters(), "|";
    foreach ($reflection->getParameters() as $parameter) {
        echo $parameter->getName(), ":";
        echo $parameter->hasType() ? $parameter->getType() : "-";
        echo ":", ($parameter->isOptional() ? "optional" : "required"), ";";
    }
    echo "\n";
}

function_row("dom_import_simplexml");
function_row("Dom\\import_simplexml");
function_row("libxml_use_internal_errors");
function_row("libxml_set_external_entity_loader");
function_row("simplexml_load_string");
function_row("simplexml_import_dom");
echo "functions|", function_exists("dom_import_simplexml") ? "yes" : "no";
echo "|", function_exists("Dom\\import_simplexml") ? "yes" : "no";
echo "|", function_exists("libxml_get_errors") ? "yes" : "no";
echo "|", function_exists("simplexml_load_file") ? "yes" : "no", "\n";
"#,
    );

    assert_eq!(
        output,
        concat!(
            "dom_import_simplexml|DOMAttr|DOMElement|1/1|node:object:required;\n",
            "Dom\\import_simplexml|Dom\\Attr|Dom\\Element|1/1|node:object:required;\n",
            "libxml_use_internal_errors|bool|0/1|use_errors:?bool:optional;\n",
            "libxml_set_external_entity_loader|true|1/1|resolver_function:?callable:required;\n",
            "simplexml_load_string|SimpleXMLElement|false|1/5|data:string:required;class_name:?string:optional;options:int:optional;namespace_or_prefix:string:optional;is_prefix:bool:optional;\n",
            "simplexml_import_dom|?SimpleXMLElement|1/2|node:object:required;class_name:?string:optional;\n",
            "functions|yes|yes|yes|yes\n",
        ),
    );
}

/// Verifies arity, unknown named parameters, and type errors remain catchable PHP runtime exceptions.
#[test]
fn dom_public_surface_argument_errors_match_php_8_5_8() {
    let output = compile_and_run(
        r#"<?php
function probe(string $id, Closure $call): void {
    try {
        $call();
        echo $id, "|none\n";
    } catch (Throwable $error) {
        echo $id, "|", get_class($error), "|", $error->getMessage(), "\n";
    }
}

$legacy = new DOMDocument();
probe("legacy-arity", fn() => $legacy->loadXML());
probe("legacy-named", fn() => $legacy->loadXML(source: "<r/>", unexpected: 1));
probe("legacy-type", fn() => $legacy->loadXML([]));
probe("xpath-arity", fn() => new DOMXPath());
probe("xpath-named", fn() => new DOMXPath(document: $legacy, unexpected: true));
probe("xpath-type", fn() => new DOMXPath(new stdClass()));
probe("modern-factory-arity", fn() => Dom\\XMLDocument::createFromString());
probe("modern-factory-type", fn() => Dom\\XMLDocument::createFromString([]));
probe("simplexml-arity", fn() => simplexml_load_string());
probe("simplexml-type", fn() => simplexml_load_string([]));
probe("libxml-arity", fn() => libxml_set_streams_context());
probe("libxml-type", fn() => libxml_use_internal_errors([]));
"#,
    );

    assert_eq!(
        output,
        concat!(
            "legacy-arity|ArgumentCountError|DOMDocument::loadXML() expects at least 1 argument, 0 given\n",
            "legacy-named|Error|Unknown named parameter $unexpected\n",
            "legacy-type|TypeError|DOMDocument::loadXML(): Argument #1 ($source) must be of type string, array given\n",
            "xpath-arity|ArgumentCountError|DOMXPath::__construct() expects at least 1 argument, 0 given\n",
            "xpath-named|Error|Unknown named parameter $unexpected\n",
            "xpath-type|TypeError|DOMXPath::__construct(): Argument #1 ($document) must be of type DOMDocument, stdClass given\n",
            "modern-factory-arity|ArgumentCountError|Dom\\XMLDocument::createFromString() expects at least 1 argument, 0 given\n",
            "modern-factory-type|TypeError|Dom\\XMLDocument::createFromString(): Argument #1 ($source) must be of type string, array given\n",
            "simplexml-arity|ArgumentCountError|simplexml_load_string() expects at least 1 argument, 0 given\n",
            "simplexml-type|TypeError|simplexml_load_string(): Argument #1 ($data) must be of type string, array given\n",
            "libxml-arity|ArgumentCountError|libxml_set_streams_context() expects exactly 1 argument, 0 given\n",
            "libxml-type|TypeError|libxml_use_internal_errors(): Argument #1 ($use_errors) must be of type ?bool, array given\n",
        ),
    );
}
