//! Purpose:
//! PHP-visible SimpleXML classes and functions over the existing XML bridge.
//!
//! Called from:
//! - `simplexml_prelude::declarations`, before name resolution and EIR lowering.
//!
//! Key details:
//! - Ordinary PHP methods share one native engine between compiled code and eval.
//! - Owned leases release native handles even when subclasses override PHP destructors.

pub(super) const SOURCE: &str = r###"<?php
extern "elephc_xml" {
    function elephc_xml_sx_load(string $data, int $length, int $options, string $ns, int $prefix, string $url): int;
    function elephc_xml_sx_load_dom(string $data, int $length, int $options): int;
    function elephc_xml_sx_load_file(string $filename, int $options, string $ns, int $prefix): int;
    function elephc_xml_sx_load_stream(string $filename, string $data, int $length, int $options, string $ns, int $prefix): int;
    function elephc_xml_sx_diagnostics(): string;
    function elephc_xml_sx_free(int $handle): void;
    function elephc_xml_sx_clone(int $handle): int;
    function elephc_xml_sx_alias(int $handle): int;
    function elephc_xml_sx_compare(int $left, int $right): int;
    function elephc_xml_sx_select(int $handle, int $kind, string $name, string $ns, int $prefix, int $index): int;
    function elephc_xml_sx_int(int $handle, int $operation): int;
    function elephc_xml_sx_projection_base(int $handle): int;
    function elephc_xml_sx_string(int $handle, int $operation): string;
    function elephc_xml_sx_xml_hex(int $handle, int $whole): string;
    function elephc_xml_sx_item(int $handle, int $index): int;
    function elephc_xml_sx_iterator(int $handle, int $operation): int;
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
    if ($class === null) { return "SimpleXMLElement"; }
    $name = (string)$class;
    if (!is_a($name, "SimpleXMLElement", true)) {
        throw new TypeError("SimpleXML class must be derived from SimpleXMLElement");
    }
    return $name;
}

function __elephc_simplexml_assigned_value(mixed $value, bool $attribute): string {
    if ($value === null || is_scalar($value)) { return (string)$value; }
    if (is_object($value) && get_class($value) === 'SimpleXMLElement') { return (string)$value; }
    $target = $attribute ? 'attributes' : 'properties';
    $type = gettype($value);
    if (is_object($value)) { $type = (string)explode("\0", get_class($value))[0]; }
    elseif (is_resource($value)) { $type = 'resource (' . get_resource_type($value) . ')'; }
    throw new TypeError("It's not possible to assign a complex type to " . $target . ', ' . $type . ' given');
}

function __elephc_simplexml_numeric_value(mixed $value): mixed {
    if ($value instanceof SimpleXMLElement) { return $value->__elephc_number(); }
    return $value;
}

function __elephc_simplexml_parse_number(string $text): int|float {
    $text = ltrim($text, " \t\n\r\v\f");
    $length = strlen($text);
    $i = 0;
    $negative = $length > 0 && $text[0] === '-';
    if ($length > 0 && ($text[0] === '-' || $text[0] === '+')) { $i++; }
    $start = $i;
    while ($i < $length && ord($text[$i]) >= 48 && ord($text[$i]) <= 57) { $i++; }
    $whole = substr($text, $start, $i - $start);
    $digits = $i - $start;
    $floating = false;
    if ($i < $length && $text[$i] === '.') {
        $floating = true;
        $i++; $start = $i;
        while ($i < $length && ord($text[$i]) >= 48 && ord($text[$i]) <= 57) { $i++; }
        $digits += $i - $start;
    }
    if ($digits === 0) { return 0; }
    if ($i < $length && ($text[$i] === 'e' || $text[$i] === 'E')) {
        $i++;
        if ($i < $length && ($text[$i] === '-' || $text[$i] === '+')) { $i++; }
        if ($i < $length && ord($text[$i]) >= 48 && ord($text[$i]) <= 57) { $floating = true; }
    }
    if (!$floating) {
        $whole = ltrim($whole, '0');
        $limit = $negative ? '9223372036854775808' : '9223372036854775807';
        $size = strlen($whole);
        $floating = $size > 19 || ($size === 19 && strcmp($whole, $limit) > 0);
    }
    return $floating ? (float)$text : (int)$text;
}

function __elephc_simplexml_write_warning(string $message, string $sourceCaller = ''): void {
    $caller = $sourceCaller === '' ? 'main()' : $sourceCaller;
    foreach ($sourceCaller === '' ? debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS) : [] as $frame) {
        $function = (string)$frame['function'];
        if (str_starts_with($function, '__elephc_')) { continue; }
        if ($function === 'eval') { $caller = 'Unknown'; }
        elseif (isset($frame['class'])) { $caller = (string)$frame['class'] . '::' . $function . '()'; }
        else { $caller = $function . '()'; }
        break;
    }
    __elephc_simplexml_warning('Warning: ' . $caller . ': ' . $message . "\n");
}

function __elephc_simplexml_report_diagnostics(string $caller, bool $dom = false): void {
    $text = elephc_xml_sx_diagnostics();
    __elephc_simplexml_report_text($text, $caller, $dom);
}

function __elephc_simplexml_report_text(string $text, string $caller, bool $dom = false): void {
    if ($text === '') { return; }
    $lines = explode("\n", $text);
    $length = count($lines);
    if ($length > 0 && $lines[$length - 1] === '') { $length = $length - 1; }
    for ($i = 0; $i < $length; $i++) {
        $line = $lines[$i];
        $category = 'Warning: ';
        if ($dom && str_starts_with($line, 'Notice: ')) { $category = 'Notice: '; $line = substr($line, 8); }
        elseif ($dom && str_starts_with($line, 'Warning: ')) { $line = substr($line, 9); }
        __elephc_simplexml_warning($category . $caller . ': ' . $line . "\n");
    }
}

final class __ElephcSimpleXMLLease {
    private int $handle;
    public function __construct(int $handle) { $this->handle = $handle; }
    public function __destruct() { $handle = $this->handle; elephc_xml_sx_free($handle); }
}

class SimpleXMLElement implements Stringable, Countable, RecursiveIterator {
    private int $__elephc_handle = 0;
    private int $__elephc_position = -1;
    private mixed $__elephc_current = null;
    private mixed $__elephc_owner = null;
    public function __elephc_attach(int $handle): void {
        $this->__elephc_handle = $handle;
        $this->__elephc_owner = new __ElephcSimpleXMLLease($handle);
    }
    public function __elephc_handle_value(): int { return $this->__elephc_handle; }
    public function __construct(string $data, int $options = 0, bool $dataIsURL = false, string $namespaceOrPrefix = "", bool $isPrefix = false) {
        try {
            if ($dataIsURL) {
                $handle = __elephc_simplexml_file_handle($data, $options, $namespaceOrPrefix, $isPrefix, 'SimpleXMLElement::__construct()');
            } else {
                $handle = elephc_xml_sx_load($data, strlen($data), $options, $namespaceOrPrefix, (int)$isPrefix, "");
            }
        } catch (Throwable $error) {
            throw new Exception("String could not be parsed as XML", 0, $error);
        }
        if ($handle !== 0) { $this->__elephc_attach($handle); }
        try { __elephc_simplexml_report_diagnostics('SimpleXMLElement::__construct()'); }
        catch (Throwable $error) {
            if ($handle === 0) { throw new Exception("String could not be parsed as XML", 0, $error); }
            throw $error;
        }
        if ($handle === 0) { throw new Exception("String could not be parsed as XML"); }
        $this->__elephc_current = null;
        $this->__elephc_position = -1;
    }
    final public function __elephc_clone_initialize(): void {
        $handle = $this->__elephc_handle;
        $this->__elephc_attach(elephc_xml_sx_clone($handle));
        $this->__elephc_current = null;
        $this->__elephc_position = -1;
    }
    public function __toString(): string {
        $handle = $this->__elephc_handle;
        return elephc_xml_sx_string($handle, 0);
    }
    final public function __elephc_int(): int { $handle = $this->__elephc_handle; return (int)elephc_xml_sx_string($handle, 0); }
    final public function __elephc_float(): float { $handle = $this->__elephc_handle; return (float)elephc_xml_sx_string($handle, 0); }
    final public function __elephc_number(): int|float {
        $handle = $this->__elephc_handle; $text = elephc_xml_sx_string($handle, 0);
        return __elephc_simplexml_parse_number($text);
    }
    public static function __elephc_add(mixed $left, mixed $right): int|float { return __elephc_simplexml_numeric_value($left) + __elephc_simplexml_numeric_value($right); }
    public static function __elephc_sub(mixed $left, mixed $right): int|float { return __elephc_simplexml_numeric_value($left) - __elephc_simplexml_numeric_value($right); }
    public static function __elephc_mul(mixed $left, mixed $right): int|float { return __elephc_simplexml_numeric_value($left) * __elephc_simplexml_numeric_value($right); }
    public static function __elephc_div(mixed $left, mixed $right): int|float { return __elephc_simplexml_numeric_value($left) / __elephc_simplexml_numeric_value($right); }
    public static function __elephc_mod(mixed $left, mixed $right): int|float { return __elephc_simplexml_numeric_value($left) % __elephc_simplexml_numeric_value($right); }
    public static function __elephc_pow(mixed $left, mixed $right): int|float { return __elephc_simplexml_numeric_value($left) ** __elephc_simplexml_numeric_value($right); }
    public static function __elephc_compare(mixed $left, mixed $right): int {
        if ($left instanceof SimpleXMLElement && $right instanceof SimpleXMLElement) {
            return elephc_xml_sx_compare($left->__elephc_handle, $right->__elephc_handle);
        }
        if (is_bool($left) || is_bool($right)) {
            $a = $left instanceof SimpleXMLElement ? $left->__elephc_bool() : (bool)$left;
            $b = $right instanceof SimpleXMLElement ? $right->__elephc_bool() : (bool)$right;
            return (int)$a - (int)$b;
        }
        if (is_object($left) && !($left instanceof SimpleXMLElement)) { return 2; }
        if (is_object($right) && !($right instanceof SimpleXMLElement)) { return 2; }
        if ($left === null || is_array($left) || is_resource($left) || gettype($left) === 'resource (closed)') { return -1; }
        if ($right === null || is_array($right) || is_resource($right) || gettype($right) === 'resource (closed)') { return 1; }
        if ((is_float($left) && is_nan($left)) || (is_float($right) && is_nan($right))) { return 2; }
        if ($left instanceof SimpleXMLElement) {
            if (is_int($right)) { return $left->__elephc_int() <=> $right; }
            if (is_float($right)) { return $left->__elephc_float() <=> $right; }
            return (string)$left <=> (string)$right;
        }
        if (is_int($left)) { return $left <=> $right->__elephc_int(); }
        if (is_float($left)) { return $left <=> $right->__elephc_float(); }
        return (string)$left <=> (string)$right;
    }
    public function __elephc_bool(): bool {
        $handle = $this->__elephc_handle;
        return elephc_xml_sx_int($handle, 3) !== 0;
    }
    final public function __elephc_property_get(string $name): static {
        $handle = $this->__elephc_handle;
        $selected = elephc_xml_sx_select($handle, 2, $name, "", 0, 0);
        return __elephc_simplexml_wrap($selected, get_class($this));
    }
    final public function __elephc_property_write_fetch(string $name): static {
        if ($name === '') { throw new ValueError('Cannot create element with an empty name'); }
        $selected = $this->__elephc_property_get($name);
        $handle = $selected->__elephc_handle;
        if (elephc_xml_sx_int($handle, 1) === 0) {
            $created = elephc_xml_sx_mutate($handle, 23, '', '', '', 0, 0);
            if ($created > 0) { return __elephc_simplexml_wrap($created, get_class($this)); }
        }
        return $selected;
    }
    final public function __elephc_property_set(string $name, mixed $value): void {
        $this->__elephc_property_write($name, $value, '');
    }
    final public function __elephc_eval_property_set(string $name, mixed $value, string $caller): void {
        $this->__elephc_property_write($name, $value, $caller);
    }
    private function __elephc_property_write(string $name, mixed $value, string $caller): void {
        if ($name === '') { throw new ValueError('Cannot create element with an empty name'); }
        $handle = $this->__elephc_handle;
        $mode = elephc_xml_sx_int($handle, 5);
        $text = __elephc_simplexml_assigned_value($value, $mode === 3 || $mode === 4);
        $status = elephc_xml_sx_mutate($handle, 2, $name, $text, "", 0, 0);
        if ($status === -2) { __elephc_simplexml_write_warning('Cannot assign to an array of nodes (duplicate subnodes or attr detected)', $caller); }
    }
    final public function __elephc_property_isset(string $name): bool {
        $node = $this->__elephc_property_get($name);
        $handle = $node->__elephc_handle;
        return elephc_xml_sx_int($handle, 1) !== 0;
    }
    final public function __elephc_property_probe(string $name): mixed {
        if (!$this->__elephc_property_isset($name)) { return null; }
        return $this->__elephc_property_get($name);
    }
    final public function __elephc_property_empty(string $name): bool {
        $handle = $this->__elephc_handle;
        $selected = elephc_xml_sx_select($handle, 2, $name, "", 0, 0);
        $empty = elephc_xml_sx_int($selected, 7) !== 0;
        elephc_xml_sx_free($selected);
        return $empty;
    }
    final public function __elephc_dimension_empty(mixed $offset): bool {
        $handle = $this->__elephc_handle;
        if (is_int($offset)) { $selected = elephc_xml_sx_select($handle, 4, "", "", 0, $offset); }
        else { $selected = elephc_xml_sx_select($handle, 3, (string)$offset, "", 0, 0); }
        $empty = elephc_xml_sx_int($selected, 7) !== 0;
        elephc_xml_sx_free($selected);
        return $empty;
    }
    final public function __elephc_property_unset(string $name): void {
        $handle = $this->__elephc_handle;
        elephc_xml_sx_mutate($handle, 12, $name, "", "", 0, 0);
    }
    final public function __elephc_dimension_get(mixed $offset): mixed {
        $handle = $this->__elephc_handle;
        if (is_int($offset)) {
            $selected = elephc_xml_sx_select($handle, 4, "", "", 0, $offset);
        } else {
            $selected = elephc_xml_sx_select($handle, 3, (string)$offset, "", 0, 0);
        }
        if (elephc_xml_sx_int($selected, 1) === 0) { elephc_xml_sx_free($selected); return null; }
        return __elephc_simplexml_wrap($selected, get_class($this));
    }
    final public function __elephc_dimension_isset(mixed $offset): bool {
        return $this->__elephc_dimension_get($offset) !== null;
    }
    final public function __elephc_dimension_set(mixed $offset, mixed $value): void {
        $this->__elephc_dimension_write($offset, $value, false);
    }
    final public function __elephc_eval_dimension_set(mixed $offset, mixed $value, string $caller): void {
        $this->__elephc_dimension_write($offset, $value, false, $caller);
    }
    final public function __elephc_dimension_write(mixed $offset, mixed $value, bool $append, string $caller = ''): void {
        $handle = $this->__elephc_handle;
        $mode = elephc_xml_sx_int($handle, 5);
        $attributes = $mode === 3 || $mode === 4;
        $kind = 3;
        $index = 0;
        $key = "";
        if ($append) { $kind = 5; }
        elseif (is_int($offset)) { $kind = 4; $index = $offset; }
        else { $key = is_string($offset) ? $offset : trim((string)$offset); }
        if ($kind === 5 && $attributes) { throw new Error('Cannot append to an attribute list'); }
        if ($kind === 5 && $mode === 0 && elephc_xml_sx_int($handle, 9) !== 0) {
            throw new ValueError('Cannot append to an attribute list');
        }
        if ($kind === 3 && $key === '') { throw new ValueError('Cannot create attribute with an empty name'); }
        $text = __elephc_simplexml_assigned_value($value, $attributes || $kind === 3);
        if (!$attributes && ($kind === 4 || $kind === 5) && elephc_xml_sx_int($handle, 4) === 2) {
            throw new Error('Cannot create duplicate attribute');
        }
        if ($kind === 4 && $attributes) {
            $length = elephc_xml_sx_int($handle, 0);
            if ($index < 0 || $index >= $length) {
                $length = $index < 0 ? 0 : $length;
                __elephc_simplexml_write_warning('Cannot change attribute number ' . $index . ' when only ' . $length . ' attributes exist', $caller);
                return;
            }
        }
        if ($kind === 4 && $mode === 0 && $index !== 0 && elephc_xml_sx_int($handle, 1) !== 0) {
            $name = elephc_xml_sx_string($handle, 1);
            __elephc_simplexml_write_warning('Cannot add element ' . $name . ' number ' . $index . ' when only 0 such elements exist', $caller);
            return;
        }
        if ($kind === 4 && $mode === 2) {
            $length = elephc_xml_sx_int($handle, 0);
            if ($index < 0 || ($length > 0 && $index > $length)) {
                $name = elephc_xml_sx_string($handle, 1);
                __elephc_simplexml_write_warning('Cannot add element ' . $name . ' number ' . $index . ' when only ' . $length . ' such elements exist', $caller);
            }
        }
        $status = elephc_xml_sx_mutate($handle, $kind, $key, $text, "", 0, $index);
        if ($status === -2) { __elephc_simplexml_write_warning('Cannot assign to an array of nodes (duplicate subnodes or attr detected)', $caller); }
    }
    final public function __elephc_append(mixed $value): void { $this->__elephc_dimension_write(null, $value, true); }
    final public function __elephc_dimension_unset(mixed $offset): void {
        $handle = $this->__elephc_handle;
        if (is_int($offset)) { elephc_xml_sx_mutate($handle, 14, "", "", "", 0, $offset); }
        else { elephc_xml_sx_mutate($handle, 13, (string)$offset, "", "", 0, 0); }
    }
    public function addChild(string $qualifiedName, ?string $value = null, ?string $namespace = null): ?SimpleXMLElement {
        if ($qualifiedName === '') { throw new ValueError('SimpleXMLElement::addChild(): Argument #1 ($qualifiedName) must not be empty'); }
        $handle = $this->__elephc_handle;
        $flags = 0;
        if ($value !== null) { $flags = $flags + 1; }
        if ($namespace !== null) { $flags = $flags + 2; }
        $child = elephc_xml_sx_mutate($handle, 20, $qualifiedName, $value ?? "", $namespace ?? "", $flags, 0);
        if ($child <= 0) {
            if ($child === -10) { __elephc_simplexml_warning("Warning: SimpleXMLElement::addChild(): Cannot add element to attributes\n"); }
            elseif ($child === -11) { __elephc_simplexml_warning("Warning: SimpleXMLElement::addChild(): Cannot add child. Parent is not a permanent member of the XML tree\n"); }
            return null;
        }
        return __elephc_simplexml_wrap($child, get_class($this));
    }
    public function addAttribute(string $qualifiedName, string $value, ?string $namespace = null): void {
        if ($qualifiedName === '') { throw new ValueError('SimpleXMLElement::addAttribute(): Argument #1 ($qualifiedName) must not be empty'); }
        $handle = $this->__elephc_handle;
        $flags = 1;
        if ($namespace !== null) { $flags = 3; }
        $attribute = elephc_xml_sx_mutate($handle, 21, $qualifiedName, $value, $namespace ?? "", $flags, 0);
        if ($attribute === -12) { __elephc_simplexml_warning("Warning: SimpleXMLElement::addAttribute(): Unable to locate parent Element\n"); }
        elseif ($attribute === -13) { __elephc_simplexml_warning("Warning: SimpleXMLElement::addAttribute(): Attribute requires prefix for namespace\n"); }
        elseif ($attribute === -14) { __elephc_simplexml_warning("Warning: SimpleXMLElement::addAttribute(): Attribute already exists\n"); }
        if ($attribute > 0) { elephc_xml_sx_free($attribute); }
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
        if ($result < 0) {
            __elephc_simplexml_report_diagnostics('SimpleXMLElement::xpath()');
            if ($result !== -1) {
                $type = 'undefined';
                if ($result === -3) { $type = 'bool'; }
                elseif ($result === -4) { $type = 'number'; }
                elseif ($result === -5) { $type = 'string'; }
                __elephc_simplexml_warning('Warning: SimpleXMLElement::xpath(): XPath expression must return a node set, ' . $type . " returned\n");
            }
            return false;
        }
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
        if ($mode === 4) {
            $values = ["__elephc_projection_seed" => ""];
            unset($values["__elephc_projection_seed"]);
            foreach ($this as $name => $attribute) { $values[$name] = (string)$attribute; }
            if (count($values) > 0) { $result["@attributes"] = $values; }
            return $result;
        }
        if ($mode !== 3 && elephc_xml_sx_int($handle, 4) === 2) {
            $result["0"] = (string)$this;
            return $result;
        }
        $base_handle = $base->__elephc_handle_value();
        $attributes = __elephc_simplexml_wrap(elephc_xml_sx_select($base_handle, 8, "", "", 0, 0), get_class($this));
        if ($mode !== 1 && $attributes instanceof SimpleXMLElement) {
            if ($attributes->count() > 0) {
                $values = ["__elephc_projection_seed" => ""];
                unset($values["__elephc_projection_seed"]);
                foreach ($attributes as $name => $attribute) { $values[$name] = (string)$attribute; }
                $result["@attributes"] = $values;
            }
        }
        if ($mode === 3) { return $result; }
        if ($mode === 2 && elephc_xml_sx_int($handle, 6) !== 0) {
            $i = 0;
            foreach ($this as $element) { $result[(string)$i] = (string)$element; $i++; }
            return $result;
        }
        $children = __elephc_simplexml_wrap(elephc_xml_sx_select($base_handle, 7, "", "", 0, 0), get_class($this));
        if ($children instanceof SimpleXMLElement) {
            if ($children->count() > 0) {
                foreach ($children as $name => $child) {
                    $raw = $child->__elephc_handle_value();
                    $value = elephc_xml_sx_int($raw, 6) !== 0 ? (string)$child : $child;
                    if (array_key_exists($name, $result)) {
                        $old = $result[$name];
                        if (!is_array($old)) { $old = [$old]; }
                        $old[] = $value;
                        $result[$name] = $old;
                    } else { $result[$name] = $value; }
                }
                return $result;
            }
        }
        $text = (string)$base;
        if ($text !== "") { $result["0"] = $text; }
        return $result;
    }
    public function __elephc_json(): mixed { return (object)$this->__elephc_array(); }
    public function __debugInfo(): ?array { return $this->__elephc_array(); }
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
        if ($kind === 3 || $kind === 4) {
            return $this->__elephc_dom_bind_node(new DOMText(), $document, $parent, "#text");
        }
        if ($kind === 8) {
            return $this->__elephc_dom_bind_node(new DOMComment(), $document, $parent, "#comment");
        }
        if ($kind === 7) {
            return $this->__elephc_dom_bind_node(new DOMProcessingInstruction($this->getName(), ""), $document, $parent, $this->getName());
        }
        return $this->__elephc_dom_bind_node(new DOMElement(), $document, $parent, $this->getName());
    }
    private function __elephc_dom_bind_node(DOMNode $node, mixed $document, mixed $parent, string $name): DOMNode {
        $node->nodeName = $name;
        $node->ownerDocument = $document;
        $node->parentNode = $parent;
        $node->__elephc_simplexml = $this;
        return $node;
    }
    public function rewind(): void {
        $this->__elephc_current = null; $this->__elephc_position = 0;
        $handle = $this->__elephc_handle;
        elephc_xml_sx_iterator($handle, 0);
    }
    public function next(): void {
        $this->__elephc_current = null; $this->__elephc_position = $this->__elephc_position + 1;
        $handle = $this->__elephc_handle;
        elephc_xml_sx_iterator($handle, 1);
    }
    public function valid(): bool {
        $handle = $this->__elephc_handle;
        return $this->__elephc_position >= 0 && elephc_xml_sx_iterator($handle, 2) !== 0;
    }
    public function current(): SimpleXMLElement {
        if (!$this->valid()) { throw new Error("Iterator not initialized or already consumed"); }
        $cached = $this->__elephc_current;
        if ($cached instanceof SimpleXMLElement) { return $cached; }
        $handle = $this->__elephc_handle;
        $child = elephc_xml_sx_iterator($handle, 3);
        $current = __elephc_simplexml_wrap($child, get_class($this));
        $this->__elephc_current = $current;
        return $current;
    }
    public function key(): string { return $this->current()->getName(); }
    public function hasChildren(): bool { return $this->valid() && $this->current()->count() > 0; }
    public function getChildren(): ?SimpleXMLElement {
        $handle = $this->__elephc_handle;
        if (!$this->valid() || elephc_xml_sx_int($handle, 5) >= 3) { return null; }
        return $this->current();
    }
}

class SimpleXMLIterator extends SimpleXMLElement {}

function simplexml_load_string(string $data, ?string $class_name = "SimpleXMLElement", int $options = 0, string $namespace_or_prefix = "", bool $is_prefix = false): SimpleXMLElement|false {
    $class = __elephc_simplexml_class($class_name);
    $handle = elephc_xml_sx_load($data, strlen($data), $options, $namespace_or_prefix, (int)$is_prefix, "");
    if ($handle === 0) {
        __elephc_simplexml_report_diagnostics('simplexml_load_string()');
        return false;
    }
    $object = __elephc_simplexml_wrap($handle, $class);
    __elephc_simplexml_report_diagnostics('simplexml_load_string()');
    return $object;
}
function simplexml_load_file(string $filename, ?string $class_name = "SimpleXMLElement", int $options = 0, string $namespace_or_prefix = "", bool $is_prefix = false): SimpleXMLElement|false {
    if (strpos($filename, "\0") !== false) { throw new ValueError('simplexml_load_file(): Argument #1 ($filename) must not contain any null bytes'); }
    $class = __elephc_simplexml_class($class_name);
    $handle = __elephc_simplexml_file_handle($filename, $options, $namespace_or_prefix, $is_prefix, 'simplexml_load_file()');
    if ($handle === 0) {
        __elephc_simplexml_report_diagnostics('simplexml_load_file()');
        return false;
    }
    $object = __elephc_simplexml_wrap($handle, $class);
    __elephc_simplexml_report_diagnostics('simplexml_load_file()');
    return $object;
}
function __elephc_simplexml_load_dom(DOMDocument $document, string $data, int $options): bool {
    $handle = elephc_xml_sx_load_dom($data, strlen($data), $options);
    $diagnostics = elephc_xml_sx_diagnostics();
    if ($handle !== 0) {
        $object = __elephc_simplexml_wrap($handle, 'SimpleXMLElement');
        $document->__elephc_simplexml = $object;
        $root = $object->__elephc_dom_node($document, $document);
        $document->childNodes = new DOMNodeList([$root]);
    }
    __elephc_simplexml_report_text($diagnostics, 'DOMDocument::loadXML()', true);
    return $handle !== 0;
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
            if ($child instanceof DOMNode) {
                if (isset($selections[$i])) {
                    $selected = $selections[$i];
                    if ($selected instanceof SimpleXMLElement) { __elephc_simplexml_bind_dom($child, $selected); }
                }
            }
            $i++;
        }
    }
}

function simplexml_import_dom(object $node, ?string $class_name = "SimpleXMLElement"): ?SimpleXMLElement {
    $class = __elephc_simplexml_class($class_name);
    if ($node instanceof DOMNode) { return __elephc_simplexml_import_node($node, $class); }
    throw new TypeError('simplexml_import_dom(): Argument #1 ($node) must be a valid XML node');
}
function __elephc_simplexml_import_node(DOMNode $node, string $class): ?SimpleXMLElement {
    $bound = $node->__elephc_simplexml;
    if ($node instanceof DOMElement && $bound instanceof SimpleXMLElement) {
        $handle = $bound->__elephc_handle_value();
        return __elephc_simplexml_wrap(elephc_xml_sx_alias($handle), $class);
    }
    $document = $node->ownerDocument;
    if ($node instanceof DOMDocument) { $document = $node; }
    if (!($document instanceof DOMDocument)) {
        __elephc_simplexml_warning("Warning: simplexml_import_dom(): Imported Node must have associated Document\n");
        return null;
    }
    if ((!($node instanceof DOMDocument) && !($node instanceof DOMElement))
        || ($node instanceof DOMDocument && !($document->documentElement instanceof DOMElement))) {
        __elephc_simplexml_warning("Warning: simplexml_import_dom(): Invalid Nodetype to import\n");
        return null;
    }
    if ($bound instanceof SimpleXMLElement) {
        $handle = $bound->__elephc_handle_value();
        return __elephc_simplexml_wrap(elephc_xml_sx_alias($handle), $class);
    }
    $result = __elephc_simplexml_import_document($document, $class);
    if (!($result instanceof SimpleXMLElement)) { return null; }
    __elephc_simplexml_bind_dom($document, $result);
    $bound = $node->__elephc_simplexml;
    if ($bound instanceof SimpleXMLElement) {
        $handle = $bound->__elephc_handle_value();
        return __elephc_simplexml_wrap(elephc_xml_sx_alias($handle), $class);
    }
    return null;
}
function __elephc_simplexml_import_document(DOMDocument $document, string $class): SimpleXMLElement|false {
    return simplexml_load_string($document->saveXML(), $class);
}
"###;
