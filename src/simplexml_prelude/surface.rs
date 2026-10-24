//! Purpose:
//! PHP-visible SimpleXML classes and functions over the existing XML bridge.
//!
//! Called from:
//! - `simplexml_prelude::declarations`, before name resolution and EIR lowering.
//!
//! Key details:
//! - Ordinary PHP methods share one native engine between compiled code and eval.

pub(super) const SOURCE: &str = r###"<?php
extern "elephc_xml" {
    function elephc_xml_sx_load(string $data, int $length, int $options, string $ns, int $prefix): int;
    function elephc_xml_sx_free(int $handle): void;
    function elephc_xml_sx_clone(int $handle): int;
    function elephc_xml_sx_alias(int $handle): int;
    function elephc_xml_sx_select(int $handle, int $kind, string $name, string $ns, int $prefix, int $index): int;
    function elephc_xml_sx_int(int $handle, int $operation): int;
    function elephc_xml_sx_projection_base(int $handle): int;
    function elephc_xml_sx_string(int $handle, int $operation): string;
    function elephc_xml_sx_xml_hex(int $handle, int $whole): string;
    function elephc_xml_sx_item(int $handle, int $index): int;
    function elephc_xml_sx_mutate(int $handle, int $operation, string $key, string $value, string $ns, int $flags, int $index): int;
    function elephc_xml_sx_register(int $handle, string $prefix, string $uri): int;
    function elephc_xml_sx_xpath(int $handle, string $expression): int;
    function elephc_xml_sx_namespace(int $handle, int $declared, int $recursive, int $root, int $index, int $field): string;
}

function __elephc_simplexml_wrap(int $handle, string $class): SimpleXMLElement {
    $object = __elephc_simplexml_allocate($class);
    if ($object instanceof SimpleXMLElement) {
        $object->__elephc_attach($handle);
        return $object;
    }
    elephc_xml_sx_free($handle);
    throw new TypeError("SimpleXML class must extend SimpleXMLElement");
}

function __elephc_simplexml_class(?string $class): string {
    if ($class === null || $class === "") { return "SimpleXMLElement"; }
    return $class;
}

class SimpleXMLElement implements Stringable, Countable, RecursiveIterator {
    private int $__elephc_handle = 0;
    private int $__elephc_position = -1;
    private mixed $__elephc_current = null;

    public function __elephc_attach(int $handle): void { $this->__elephc_handle = $handle; }
    public function __elephc_handle_value(): int { return $this->__elephc_handle; }
    public function __construct(string $data, int $options = 0, bool $dataIsURL = false, string $namespaceOrPrefix = "", bool $isPrefix = false) {
        if ($dataIsURL) {
            $contents = file_get_contents($data);
            if ($contents === false) { throw new Exception("String could not be parsed as XML"); }
            $data = $contents;
        }
        $handle = elephc_xml_sx_load($data, strlen($data), $options, $namespaceOrPrefix, (int)$isPrefix);
        if ($handle === 0) { throw new Exception("String could not be parsed as XML"); }
        $old = $this->__elephc_handle;
        $this->__elephc_handle = $handle;
        $this->__elephc_current = null;
        $this->__elephc_position = -1;
        if ($old !== 0) { elephc_xml_sx_free($old); }
    }
    public function __destruct() {
        $handle = $this->__elephc_handle;
        $this->__elephc_handle = 0;
        elephc_xml_sx_free($handle);
    }
    public function __clone(): void {
        $handle = $this->__elephc_handle;
        $this->__elephc_handle = elephc_xml_sx_clone($handle);
        $this->__elephc_current = null;
    }
    public function __toString(): string {
        $handle = $this->__elephc_handle;
        return elephc_xml_sx_string($handle, 0);
    }
    public function __elephc_int(): int { return (int)(string)$this; }
    public function __elephc_float(): float { return (float)(string)$this; }
    public function __elephc_bool(): bool {
        $handle = $this->__elephc_handle;
        return elephc_xml_sx_int($handle, 3) !== 0;
    }
    public function __get(string $name): SimpleXMLElement {
        $handle = $this->__elephc_handle;
        $selected = elephc_xml_sx_select($handle, 2, $name, "", 0, 0);
        return __elephc_simplexml_wrap($selected, get_class($this));
    }
    public function __set(string $name, mixed $value): void {
        $handle = $this->__elephc_handle;
        $status = elephc_xml_sx_mutate($handle, 2, $name, (string)$value, "", 0, 0);
        if ($status === -2) { trigger_error("Cannot assign to an array of nodes (duplicate subnodes or attr detected)", E_USER_WARNING); }
    }
    public function __isset(string $name): bool {
        $node = $this->__get($name);
        $handle = $node->__elephc_handle;
        return elephc_xml_sx_int($handle, 1) !== 0;
    }
    public function __unset(string $name): void {
        $handle = $this->__elephc_handle;
        elephc_xml_sx_mutate($handle, 12, $name, "", "", 0, 0);
    }
    public function offsetGet(mixed $offset): mixed {
        $handle = $this->__elephc_handle;
        if (is_int($offset)) {
            $selected = elephc_xml_sx_select($handle, 4, "", "", 0, $offset);
        } else {
            $selected = elephc_xml_sx_select($handle, 3, (string)$offset, "", 0, 0);
        }
        if (elephc_xml_sx_int($selected, 1) === 0) { elephc_xml_sx_free($selected); return null; }
        return __elephc_simplexml_wrap($selected, get_class($this));
    }
    public function offsetExists(mixed $offset): bool {
        return $this->offsetGet($offset) !== null;
    }
    public function offsetSet(mixed $offset, mixed $value): void {
        $handle = $this->__elephc_handle;
        $kind = 3;
        $index = 0;
        $key = "";
        if ($offset === null) { $kind = 5; }
        elseif (is_int($offset)) { $kind = 4; $index = $offset; }
        else { $key = (string)$offset; }
        elephc_xml_sx_mutate($handle, $kind, $key, (string)$value, "", 0, $index);
    }
    public function __elephc_append(mixed $value): void { $this->offsetSet(null, $value); }
    public function offsetUnset(mixed $offset): void {
        $handle = $this->__elephc_handle;
        if (is_int($offset)) { elephc_xml_sx_mutate($handle, 14, "", "", "", 0, $offset); }
        else { elephc_xml_sx_mutate($handle, 13, (string)$offset, "", "", 0, 0); }
    }
    public function addChild(string $qualifiedName, ?string $value = null, ?string $namespace = null): ?SimpleXMLElement {
        $handle = $this->__elephc_handle;
        $flags = 0;
        if ($value !== null) { $flags = $flags + 1; }
        if ($namespace !== null) { $flags = $flags + 2; }
        $child = elephc_xml_sx_mutate($handle, 20, $qualifiedName, $value ?? "", $namespace ?? "", $flags, 0);
        if ($child === 0) { return null; }
        return __elephc_simplexml_wrap($child, get_class($this));
    }
    public function addAttribute(string $qualifiedName, string $value, ?string $namespace = null): void {
        $handle = $this->__elephc_handle;
        $flags = 1;
        if ($namespace !== null) { $flags = 3; }
        $attribute = elephc_xml_sx_mutate($handle, 21, $qualifiedName, $value, $namespace ?? "", $flags, 0);
        elephc_xml_sx_free($attribute);
    }
    public function asXML(?string $filename = null): string|bool {
        $handle = $this->__elephc_handle;
        if (elephc_xml_sx_int($handle, 1) === 0) { return false; }
        $xml = (string)hex2bin(elephc_xml_sx_xml_hex($handle, 1));
        if ($filename === null) { return $xml; }
        return file_put_contents($filename, $xml) !== false;
    }
    public function saveXML(?string $filename = null): string|bool { return $this->asXML($filename); }
    public function attributes(?string $namespaceOrPrefix = null, bool $isPrefix = false): ?SimpleXMLElement {
        $handle = $this->__elephc_handle;
        if (elephc_xml_sx_int($handle, 1) === 0 || elephc_xml_sx_int($handle, 4) === 2) { return null; }
        $selected = elephc_xml_sx_select($handle, 1, "", $namespaceOrPrefix ?? "", (int)$isPrefix, 0);
        return __elephc_simplexml_wrap($selected, get_class($this));
    }
    public function children(?string $namespaceOrPrefix = null, bool $isPrefix = false): ?SimpleXMLElement {
        $handle = $this->__elephc_handle;
        if (elephc_xml_sx_int($handle, 1) === 0 || elephc_xml_sx_int($handle, 4) === 2) { return null; }
        $selected = elephc_xml_sx_select($handle, 0, "", $namespaceOrPrefix ?? "", (int)$isPrefix, 0);
        return __elephc_simplexml_wrap($selected, get_class($this));
    }
    public function count(): int {
        $handle = $this->__elephc_handle;
        return elephc_xml_sx_int($handle, 0);
    }
    public function getName(): string {
        $handle = $this->__elephc_handle;
        return elephc_xml_sx_string($handle, 1);
    }
    public function registerXPathNamespace(string $prefix, string $namespace): bool {
        $handle = $this->__elephc_handle;
        return elephc_xml_sx_register($handle, $prefix, $namespace) !== 0;
    }
    public function xpath(string $expression): array|null|false {
        $handle = $this->__elephc_handle;
        $result = elephc_xml_sx_xpath($handle, $expression);
        if ($result === 0) { return null; }
        if ($result < 0) { return false; }
        $nodes = [];
        $length = elephc_xml_sx_int($result, 2);
        for ($i = 0; $i < $length; $i++) {
            $child = elephc_xml_sx_item($result, $i);
            $nodes[] = __elephc_simplexml_wrap($child, get_class($this));
        }
        elephc_xml_sx_free($result);
        return $nodes;
    }
    public function getNamespaces(bool $recursive = false): array {
        return $this->__elephc_namespaces(false, $recursive, false);
    }
    public function getDocNamespaces(bool $recursive = false, bool $fromRoot = true): array|false {
        return $this->__elephc_namespaces(true, $recursive, $fromRoot);
    }
    private function __elephc_namespaces(bool $declared, bool $recursive, bool $root): array {
        $handle = $this->__elephc_handle;
        $length = (int)elephc_xml_sx_namespace($handle, (int)$declared, (int)$recursive, (int)$root, -1, 0);
        $namespaces = [];
        for ($i = 0; $i < $length; $i++) {
            $prefix = elephc_xml_sx_namespace($handle, (int)$declared, (int)$recursive, (int)$root, $i, 0);
            $uri = elephc_xml_sx_namespace($handle, (int)$declared, (int)$recursive, (int)$root, $i, 1);
            $namespaces[$prefix] = $uri;
        }
        return $namespaces;
    }
    public function __elephc_array(): array<string, mixed> {
        $handle = $this->__elephc_handle;
        $mode = elephc_xml_sx_int($handle, 5);
        $base = __elephc_simplexml_wrap(elephc_xml_sx_projection_base($handle), get_class($this));
        $result = ["__elephc_projection_seed" => null];
        unset($result["__elephc_projection_seed"]);
        if ($mode === 4 || elephc_xml_sx_int($handle, 4) === 2) {
            $result["0"] = (string)$this;
            return $result;
        }
        $attributes = $base->attributes();
        if ($attributes !== null && count($attributes) > 0) {
            $values = ["__elephc_projection_seed" => ""];
            unset($values["__elephc_projection_seed"]);
            foreach ($attributes as $name => $attribute) { $values[$name] = (string)$attribute; }
            $result["@attributes"] = $values;
        }
        if ($mode === 3) { return $result; }
        if ($mode === 2 && elephc_xml_sx_int($handle, 6) !== 0) {
            $i = 0;
            foreach ($this as $element) { $result[(string)$i] = (string)$element; $i++; }
            return $result;
        }
        $children = $base->children();
        if ($children !== null && count($children) > 0) {
            foreach ($children as $name => $child) {
                $value = $child;
                $raw = $child->__elephc_handle_value();
                if (elephc_xml_sx_int($raw, 6) !== 0) { $value = (string)$child; }
                if (array_key_exists($name, $result)) {
                    $old = $result[$name];
                    if (!is_array($old)) { $old = [$old]; }
                    $old[] = $value;
                    $result[$name] = $old;
                } else { $result[$name] = $value; }
            }
        } else {
            $text = (string)$base;
            if ($text !== "") { $result["0"] = $text; }
        }
        return $result;
    }
    public function __elephc_json(): mixed { return (object)$this->__elephc_array(); }
    public function __elephc_dom_text(): string {
        $handle = $this->__elephc_handle;
        return elephc_xml_sx_string($handle, 3);
    }
    public function __elephc_dom_xml(): string {
        $handle = $this->__elephc_handle;
        return (string)hex2bin(elephc_xml_sx_xml_hex($handle, 0));
    }
    public function __elephc_dom_set_text(string $text): void {
        $handle = $this->__elephc_handle;
        elephc_xml_sx_mutate($handle, 22, "", $text, "", 0, 0);
    }
    public function __elephc_dom_selections(): array {
        $handle = $this->__elephc_handle;
        $all = elephc_xml_sx_select($handle, 6, "", "", 0, 0);
        $length = elephc_xml_sx_int($all, 2);
        $nodes = [];
        for ($i = 0; $i < $length; $i++) {
            $child = elephc_xml_sx_item($all, $i);
            $nodes[] = __elephc_simplexml_wrap($child, get_class($this));
        }
        elephc_xml_sx_free($all);
        return $nodes;
    }
    public function __elephc_dom_children(mixed $document, mixed $parent): DOMNodeList {
        $children = [];
        foreach ($this->__elephc_dom_selections() as $view) {
            if ($view instanceof SimpleXMLElement) { $children[] = $view->__elephc_dom_node($document, $parent); }
        }
        return new DOMNodeList($children);
    }
    public function __elephc_dom_node(mixed $document, mixed $parent): DOMNode {
        $handle = $this->__elephc_handle;
        $kind = elephc_xml_sx_int($handle, 4);
        $node = null;
        if ($kind === 3 || $kind === 4) { $node = new DOMText(); }
        elseif ($kind === 8) { $node = new DOMComment(); }
        elseif ($kind === 7) { $node = new DOMProcessingInstruction($this->getName(), ""); }
        else { $node = new DOMElement(); }
        $node->nodeName = $this->getName();
        if ($kind === 3 || $kind === 4) { $node->nodeName = "#text"; }
        if ($kind === 8) { $node->nodeName = "#comment"; }
        $node->ownerDocument = $document;
        $node->parentNode = $parent;
        $node->__elephc_simplexml = $this;
        return $node;
    }
    public function rewind(): void { $this->__elephc_current = null; $this->__elephc_position = 0; }
    public function next(): void { $this->__elephc_current = null; $this->__elephc_position = $this->__elephc_position + 1; }
    public function valid(): bool {
        $handle = $this->__elephc_handle;
        return $this->__elephc_position >= 0 && $this->__elephc_position < elephc_xml_sx_int($handle, 2);
    }
    public function current(): SimpleXMLElement {
        if (!$this->valid()) { throw new Error("Iterator not initialized or already finished"); }
        $cached = $this->__elephc_current;
        if ($cached instanceof SimpleXMLElement) { return $cached; }
        $handle = $this->__elephc_handle;
        $position = $this->__elephc_position;
        $child = elephc_xml_sx_item($handle, $position);
        $current = __elephc_simplexml_wrap($child, get_class($this));
        $this->__elephc_current = $current;
        return $current;
    }
    public function key(): string { return $this->current()->getName(); }
    public function hasChildren(): bool { return $this->valid() && $this->current()->count() > 0; }
    public function getChildren(): ?SimpleXMLElement { if (!$this->valid()) { return null; } return $this->current(); }
}

class SimpleXMLIterator extends SimpleXMLElement {}

function simplexml_load_string(string $data, ?string $class_name = "SimpleXMLElement", int $options = 0, string $namespace_or_prefix = "", bool $is_prefix = false): SimpleXMLElement|false {
    $class = __elephc_simplexml_class($class_name);
    $handle = elephc_xml_sx_load($data, strlen($data), $options, $namespace_or_prefix, (int)$is_prefix);
    if ($handle === 0) { return false; }
    return __elephc_simplexml_wrap($handle, $class);
}
function simplexml_load_file(string $filename, ?string $class_name = "SimpleXMLElement", int $options = 0, string $namespace_or_prefix = "", bool $is_prefix = false): SimpleXMLElement|false {
    $class = __elephc_simplexml_class($class_name);
    $data = file_get_contents($filename);
    if ($data === false) { return false; }
    return simplexml_load_string($data, $class, $options, $namespace_or_prefix, $is_prefix);
}
function __elephc_simplexml_bind_dom(DOMNode $node, SimpleXMLElement $view): void {
    $children = $node->childNodes;
    if ($node instanceof DOMDocument) {
        $node->__elephc_simplexml = $view;
        if ($children instanceof DOMNodeList) {
            foreach ($children as $child) {
                if ($child instanceof DOMElement) { __elephc_simplexml_bind_dom($child, $view); break; }
            }
        }
        return;
    }
    $selections = $view->__elephc_dom_selections();
    $node->__elephc_simplexml = $view;
    $i = 0;
    if ($children instanceof DOMNodeList) {
        foreach ($children as $child) {
            if ($child instanceof DOMNode && isset($selections[$i])) {
                $selected = $selections[$i];
                if ($selected instanceof SimpleXMLElement) { __elephc_simplexml_bind_dom($child, $selected); }
            }
            $i++;
        }
    }
}

function simplexml_import_dom(object $node, ?string $class_name = "SimpleXMLElement"): ?SimpleXMLElement {
    $class = __elephc_simplexml_class($class_name);
    if (!($node instanceof DOMNode)) { throw new TypeError("simplexml_import_dom(): Argument #1 ($node) must be of type DOMNode"); }
    if (!($node instanceof DOMDocument) && !($node instanceof DOMElement)) { return null; }
    $bound = $node->__elephc_simplexml;
    if ($bound instanceof SimpleXMLElement) {
        $handle = $bound->__elephc_handle_value();
        return __elephc_simplexml_wrap(elephc_xml_sx_alias($handle), $class);
    }
    $document = $node->ownerDocument;
    if ($node instanceof DOMDocument) { $document = $node; }
    if (!($document instanceof DOMDocument)) { return null; }
    $result = simplexml_load_string($document->saveXML(), $class);
    if ($result === false) { return null; }
    __elephc_simplexml_bind_dom($document, $result);
    $bound = $node->__elephc_simplexml;
    if ($bound instanceof SimpleXMLElement) {
        $handle = $bound->__elephc_handle_value();
        return __elephc_simplexml_wrap(elephc_xml_sx_alias($handle), $class);
    }
    return null;
}
"###;
