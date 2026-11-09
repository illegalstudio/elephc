//! Purpose:
//! The PHP form of the libxml2-backed legacy DOM prelude, kept as the transcription source and
//! parse-parity oracle for the AST builders in `crate::dom_html_prelude::legacy`.
//!
//! Called from:
//! - `crate::dom_html_prelude::oracle_tests` and the transcription driver
//!   (`synthetic_class::transcribe::tests::dump_prelude_on_request`).
//!
//! Key details:
//! - NOT compiled into programs: only the builders are. When this text changes, re-transcribe.
//! - Every node object wraps one handle of the native engine (`crates/elephc-xml/src/dom`);
//!   `__ElephcDOM::wrap()` keeps one object per handle, so `===` holds as in PHP, and mints
//!   objects without running user-visible constructors (php-src never calls them either).
//! - DOM properties are virtual `get` hooks reading native state on every access, which is
//!   how php-src's property handlers behave.
//! - Tree-operation results follow the engine's encoding: a positive handle, 0 for false, a
//!   negative DOMException code, or -100 for php's "Document Fragment is empty" warning.

/// The legacy DOM prelude as PHP source (with its `<?php` header).
pub(super) const SRC: &str = r##"<?php

extern "elephc_xml" {
    function elephc_xml_dom_document_new(string $version, string $encoding): int;
    function elephc_xml_dom_load(int $document, string $data, int $length, int $options): int;
    function elephc_xml_dom_load_file(int $document, string $filename, int $options): int;
    function elephc_xml_dom_load_html(int $document, string $data, int $length, string $filename, int $options): int;
    function elephc_xml_dom_save(int $document, int $node, int $format, int $noDecl, int $noEmptyTags): ?string;
    function elephc_xml_dom_save_html(int $document, int $node, int $format): ?string;
    function elephc_xml_dom_type(int $node): int;
    function elephc_xml_dom_node(int $node, int $relation): int;
    function elephc_xml_dom_string(int $node, int $field): ?string;
    function elephc_xml_dom_set_text(int $node, string $value, int $literal): void;
    function elephc_xml_dom_child(int $node, int $index): int;
    function elephc_xml_dom_child_count(int $node): int;
    function elephc_xml_dom_attribute(int $node, string $name): ?string;
    function elephc_xml_dom_attribute_node(int $node, string $name): int;
    function elephc_xml_dom_set_attribute(int $node, string $name, string $value): int;
    function elephc_xml_dom_remove_attribute(int $node, string $name): int;
    function elephc_xml_dom_attribute_at(int $node, int $index): int;
    function elephc_xml_dom_create(int $document, int $kind, string $name, string $value): int;
    function elephc_xml_dom_insert(int $parent, int $child, int $reference): int;
    function elephc_xml_dom_remove_child(int $parent, int $child): int;
    function elephc_xml_dom_clone(int $node, int $deep): int;
    function elephc_xml_dom_tag_count(int $node, string $name): int;
    function elephc_xml_dom_tag_item(int $node, string $name, int $index): int;
}

final class DOMException extends Exception {
}

final class __ElephcDOM {
    public static array $objects = [];
    public static bool $minting = false;

    public static function wrap(int $handle): mixed {
        if ($handle <= 0) {
            return null;
        }
        if (isset(__ElephcDOM::$objects[$handle])) {
            $cached = __ElephcDOM::$objects[$handle];
            if ($cached instanceof DOMNode) {
                return $cached;
            }
        }
        __ElephcDOM::$minting = true;
        $object = __ElephcDOM::mint(elephc_xml_dom_type($handle));
        __ElephcDOM::$minting = false;
        $object->__elephc_bind($handle);
        __ElephcDOM::$objects[$handle] = $object;
        return $object;
    }

    public static function mint(int $type): DOMNode {
        if ($type === 1) {
            return new DOMElement("");
        }
        if ($type === 2) {
            return new DOMAttr("");
        }
        if ($type === 3) {
            return new DOMText();
        }
        if ($type === 4) {
            return new DOMCdataSection("");
        }
        if ($type === 7) {
            return new DOMProcessingInstruction("");
        }
        if ($type === 8) {
            return new DOMComment();
        }
        if ($type === 9 || $type === 13) {
            return new DOMDocument();
        }
        if ($type === 11) {
            return new DOMDocumentFragment();
        }
        return new DOMNode();
    }

    public static function fail(int $code): void {
        $messages = [
            3 => "Hierarchy Request Error",
            4 => "Wrong Document Error",
            7 => "No Modification Allowed Error",
            8 => "Not Found Error",
            11 => "Invalid State Error",
        ];
        throw new DOMException($messages[$code] ?? "Invalid State Error", $code);
    }

    public static function tree(int $result): DOMNode|false {
        if ($result === -100) {
            trigger_error("Document Fragment is empty", E_USER_WARNING);
            return false;
        }
        if ($result < 0) {
            __ElephcDOM::fail(-$result);
        }
        $node = __ElephcDOM::wrap($result);
        return $node instanceof DOMNode ? $node : false;
    }

    public static function document(): int {
        return elephc_xml_dom_document_new("1.0", "");
    }

    public static function nodes(DOMNode $context, array $nodes): array {
        $document = $context instanceof DOMDocument ? $context : $context->ownerDocument;
        $converted = [];
        foreach ($nodes as $node) {
            if ($node instanceof DOMNode) {
                $converted[] = $node;
            } elseif ($document instanceof DOMDocument) {
                $converted[] = $document->createTextNode((string) $node);
            }
        }
        return $converted;
    }
}

class DOMNode {
    private int $__elephc_handle = 0;

    public string $nodeName {
        get => (string) elephc_xml_dom_string($this->__elephc_handle, 0);
    }
    public ?string $nodeValue {
        get => elephc_xml_dom_string($this->__elephc_handle, 1);
        set(mixed $value) {
            elephc_xml_dom_set_text($this->__elephc_handle, (string) $value, 0);
        }
    }
    public int $nodeType {
        get => elephc_xml_dom_type($this->__elephc_handle);
    }
    public mixed $parentNode {
        get => __ElephcDOM::wrap(elephc_xml_dom_node($this->__elephc_handle, 0));
    }
    public mixed $parentElement {
        get {
            $parent = __ElephcDOM::wrap(elephc_xml_dom_node($this->__elephc_handle, 0));
            return $parent instanceof DOMElement ? $parent : null;
        }
    }
    public mixed $childNodes {
        get => DOMNodeList::__elephc_list($this->__elephc_handle, "");
    }
    public mixed $firstChild {
        get => __ElephcDOM::wrap(elephc_xml_dom_node($this->__elephc_handle, 1));
    }
    public mixed $lastChild {
        get => __ElephcDOM::wrap(elephc_xml_dom_node($this->__elephc_handle, 2));
    }
    public mixed $previousSibling {
        get => __ElephcDOM::wrap(elephc_xml_dom_node($this->__elephc_handle, 3));
    }
    public mixed $nextSibling {
        get => __ElephcDOM::wrap(elephc_xml_dom_node($this->__elephc_handle, 4));
    }
    public mixed $attributes {
        get => elephc_xml_dom_type($this->__elephc_handle) === 1 ? DOMNamedNodeMap::__elephc_map($this->__elephc_handle) : null;
    }
    public mixed $ownerDocument {
        get {
            $owner = __ElephcDOM::wrap(elephc_xml_dom_node($this->__elephc_handle, 5));
            return $owner instanceof DOMDocument ? $owner : null;
        }
    }
    public ?string $namespaceURI {
        get => elephc_xml_dom_string($this->__elephc_handle, 5);
    }
    public string $prefix {
        get => (string) elephc_xml_dom_string($this->__elephc_handle, 4);
    }
    public ?string $localName {
        get => elephc_xml_dom_string($this->__elephc_handle, 3);
    }
    public string $textContent {
        get => (string) elephc_xml_dom_string($this->__elephc_handle, 2);
        set(mixed $value) {
            elephc_xml_dom_set_text($this->__elephc_handle, (string) $value, 1);
        }
    }

    public function __elephc_bind(int $handle): void {
        $this->__elephc_handle = $handle;
    }

    public function __elephc_handle(): int {
        return $this->__elephc_handle;
    }

    public function appendChild(DOMNode $node) {
        return __ElephcDOM::tree(elephc_xml_dom_insert($this->__elephc_handle, $node->__elephc_handle(), 0));
    }

    public function insertBefore(DOMNode $node, ?DOMNode $child = null) {
        $reference = $child === null ? 0 : $child->__elephc_handle();
        return __ElephcDOM::tree(elephc_xml_dom_insert($this->__elephc_handle, $node->__elephc_handle(), $reference));
    }

    public function removeChild(DOMNode $child) {
        return __ElephcDOM::tree(elephc_xml_dom_remove_child($this->__elephc_handle, $child->__elephc_handle()));
    }

    public function replaceChild(DOMNode $node, DOMNode $child) {
        $inserted = __ElephcDOM::tree(elephc_xml_dom_insert($this->__elephc_handle, $node->__elephc_handle(), $child->__elephc_handle()));
        if ($inserted === false) {
            return false;
        }
        return __ElephcDOM::tree(elephc_xml_dom_remove_child($this->__elephc_handle, $child->__elephc_handle()));
    }

    public function cloneNode(bool $deep = false) {
        return __ElephcDOM::wrap(elephc_xml_dom_clone($this->__elephc_handle, $deep ? 1 : 0));
    }

    public function hasChildNodes(): bool {
        return elephc_xml_dom_node($this->__elephc_handle, 1) !== 0;
    }

    public function hasAttributes(): bool {
        return elephc_xml_dom_type($this->__elephc_handle) === 1 && elephc_xml_dom_attribute_at($this->__elephc_handle, 0) !== 0;
    }

    public function isSameNode(DOMNode $otherNode): bool {
        return $otherNode->__elephc_handle() === $this->__elephc_handle;
    }
}

class DOMNodeList implements IteratorAggregate, Countable {
    private int $__elephc_root = 0;
    private string $__elephc_tag = "";

    public int $length {
        get => $this->__elephc_tag === "" ? elephc_xml_dom_child_count($this->__elephc_root) : elephc_xml_dom_tag_count($this->__elephc_root, $this->__elephc_tag);
    }

    public static function __elephc_list(int $root, string $tag): DOMNodeList {
        $list = new DOMNodeList();
        $list->__elephc_root = $root;
        $list->__elephc_tag = $tag;
        return $list;
    }

    public function item(int $index) {
        if ($index < 0) {
            return null;
        }
        if ($this->__elephc_tag === "") {
            return __ElephcDOM::wrap(elephc_xml_dom_child($this->__elephc_root, $index));
        }
        return __ElephcDOM::wrap(elephc_xml_dom_tag_item($this->__elephc_root, $this->__elephc_tag, $index));
    }

    public function count(): int {
        return $this->length;
    }

    public function getIterator(): Iterator {
        $items = [];
        $count = $this->length;
        for ($index = 0; $index < $count; $index++) {
            $items[] = $this->item($index);
        }
        return new ArrayIterator($items);
    }
}

class DOMNamedNodeMap implements IteratorAggregate, Countable {
    private int $__elephc_element = 0;

    public int $length {
        get {
            $count = 0;
            while (elephc_xml_dom_attribute_at($this->__elephc_element, $count) !== 0) {
                $count++;
            }
            return $count;
        }
    }

    public static function __elephc_map(int $element): DOMNamedNodeMap {
        $map = new DOMNamedNodeMap();
        $map->__elephc_element = $element;
        return $map;
    }

    public function getNamedItem(string $qualifiedName): ?DOMNode {
        $node = __ElephcDOM::wrap(elephc_xml_dom_attribute_node($this->__elephc_element, $qualifiedName));
        return $node instanceof DOMNode ? $node : null;
    }

    public function item(int $index): ?DOMNode {
        if ($index < 0) {
            return null;
        }
        $node = __ElephcDOM::wrap(elephc_xml_dom_attribute_at($this->__elephc_element, $index));
        return $node instanceof DOMNode ? $node : null;
    }

    public function count(): int {
        return $this->length;
    }

    public function getIterator(): Iterator {
        $items = [];
        $count = $this->length;
        for ($index = 0; $index < $count; $index++) {
            $node = $this->item($index);
            $items[$node->nodeName] = $node;
        }
        return new ArrayIterator($items);
    }
}

class DOMDocumentFragment extends DOMNode {
    public function __construct() {
        if (!__ElephcDOM::$minting) {
            $this->__elephc_bind(elephc_xml_dom_create(__ElephcDOM::document(), 11, "", ""));
        }
    }
}

class DOMCharacterData extends DOMNode {
    public string $data {
        get => (string) elephc_xml_dom_string($this->__elephc_handle(), 1);
        set(string $value) {
            elephc_xml_dom_set_text($this->__elephc_handle(), $value, 0);
        }
    }
    public int $length {
        get => mb_strlen((string) elephc_xml_dom_string($this->__elephc_handle(), 1), "UTF-8");
    }

    public function appendData(string $data): true {
        $this->data = $this->data . $data;
        return true;
    }
}

class DOMText extends DOMCharacterData {
    public string $wholeText {
        get => $this->data;
    }

    public function __construct(string $data = "") {
        if (!__ElephcDOM::$minting) {
            $this->__elephc_bind(elephc_xml_dom_create(__ElephcDOM::document(), 3, "", $data));
        }
    }
}

class DOMCdataSection extends DOMText {
    public function __construct(string $data) {
        if (!__ElephcDOM::$minting) {
            $this->__elephc_bind(elephc_xml_dom_create(__ElephcDOM::document(), 4, "", $data));
        }
    }
}

class DOMComment extends DOMCharacterData {
    public function __construct(string $data = "") {
        if (!__ElephcDOM::$minting) {
            $this->__elephc_bind(elephc_xml_dom_create(__ElephcDOM::document(), 8, "", $data));
        }
    }
}

class DOMProcessingInstruction extends DOMNode {
    public string $target {
        get => $this->nodeName;
    }
    public string $data {
        get => (string) $this->nodeValue;
        set(string $value) {
            $this->nodeValue = $value;
        }
    }

    public function __construct(string $name, string $value = "") {
        if (!__ElephcDOM::$minting) {
            $this->__elephc_bind(elephc_xml_dom_create(__ElephcDOM::document(), 7, $name, $value));
        }
    }
}

class DOMAttr extends DOMNode {
    public string $name {
        get => $this->nodeName;
    }
    public bool $specified {
        get => true;
    }
    public string $value {
        get => (string) $this->nodeValue;
        set(string $value) {
            $this->nodeValue = $value;
        }
    }
    public ?DOMElement $ownerElement {
        get {
            $parent = $this->parentElement;
            return $parent instanceof DOMElement ? $parent : null;
        }
    }

    public function __construct(string $name, string $value = "") {
        if (!__ElephcDOM::$minting) {
            $this->__elephc_bind(elephc_xml_dom_create(__ElephcDOM::document(), 2, $name, ""));
            $this->nodeValue = $value;
        }
    }
}

class DOMElement extends DOMNode {
    public string $tagName {
        get => $this->nodeName;
    }

    public function __construct(string $qualifiedName, ?string $value = null, string $namespace = "") {
        if (!__ElephcDOM::$minting) {
            $this->__elephc_bind(elephc_xml_dom_create(__ElephcDOM::document(), 1, $qualifiedName, (string) $value));
        }
    }

    public function getAttribute(string $qualifiedName): string {
        return (string) elephc_xml_dom_attribute($this->__elephc_handle(), $qualifiedName);
    }

    public function hasAttribute(string $qualifiedName): bool {
        return elephc_xml_dom_attribute($this->__elephc_handle(), $qualifiedName) !== null;
    }

    public function setAttribute(string $qualifiedName, string $value) {
        return __ElephcDOM::wrap(elephc_xml_dom_set_attribute($this->__elephc_handle(), $qualifiedName, $value));
    }

    public function removeAttribute(string $qualifiedName): bool {
        return elephc_xml_dom_remove_attribute($this->__elephc_handle(), $qualifiedName) !== 0;
    }

    public function getAttributeNode(string $qualifiedName) {
        $node = __ElephcDOM::wrap(elephc_xml_dom_attribute_node($this->__elephc_handle(), $qualifiedName));
        return $node === null ? false : $node;
    }

    public function getElementsByTagName(string $qualifiedName): DOMNodeList {
        return DOMNodeList::__elephc_list($this->__elephc_handle(), $qualifiedName);
    }

    public function append(...$nodes): void {
        foreach (__ElephcDOM::nodes($this, $nodes) as $node) {
            $this->appendChild($node);
        }
    }

    public function prepend(...$nodes): void {
        $current = $this->firstChild;
        $first = $current instanceof DOMNode ? $current : null;
        foreach (__ElephcDOM::nodes($this, $nodes) as $node) {
            $this->insertBefore($node, $first);
        }
    }

    public function replaceChildren(...$nodes): void {
        while ($this->firstChild instanceof DOMNode) {
            $this->removeChild($this->firstChild);
        }
        $this->append(...$nodes);
    }

    public function remove(): void {
        $parent = $this->parentNode;
        if ($parent instanceof DOMNode) {
            $parent->removeChild($this);
        }
    }

    public function before(...$nodes): void {
        $parent = $this->parentNode;
        if ($parent instanceof DOMNode) {
            foreach (__ElephcDOM::nodes($this, $nodes) as $node) {
                $parent->insertBefore($node, $this);
            }
        }
    }

    public function after(...$nodes): void {
        $parent = $this->parentNode;
        if ($parent instanceof DOMNode) {
            $sibling = $this->nextSibling;
            $next = $sibling instanceof DOMNode ? $sibling : null;
            foreach (__ElephcDOM::nodes($this, $nodes) as $node) {
                $parent->insertBefore($node, $next);
            }
        }
    }

    public function replaceWith(...$nodes): void {
        $this->before(...$nodes);
        $this->remove();
    }
}

class DOMDocument extends DOMNode {
    public bool $formatOutput = false;
    public bool $preserveWhiteSpace = true;
    public bool $validateOnParse = false;
    public bool $resolveExternals = false;
    public bool $recover = false;
    public bool $substituteEntities = false;
    public bool $strictErrorChecking = true;

    public ?DOMElement $documentElement {
        get {
            $root = __ElephcDOM::wrap(elephc_xml_dom_node($this->__elephc_handle(), 6));
            return $root instanceof DOMElement ? $root : null;
        }
    }
    public ?string $xmlVersion {
        get => elephc_xml_dom_string($this->__elephc_handle(), 6);
    }
    public ?string $version {
        get => elephc_xml_dom_string($this->__elephc_handle(), 6);
    }
    public ?string $xmlEncoding {
        get => elephc_xml_dom_string($this->__elephc_handle(), 7);
    }
    public ?string $encoding {
        get => elephc_xml_dom_string($this->__elephc_handle(), 7);
    }
    public ?string $documentURI {
        get => elephc_xml_dom_string($this->__elephc_handle(), 8);
    }

    public function __construct(string $version = "1.0", string $encoding = "") {
        if (!__ElephcDOM::$minting) {
            $this->__elephc_bind(elephc_xml_dom_document_new($version, $encoding));
            __ElephcDOM::$objects[$this->__elephc_handle()] = $this;
        }
    }

    public function loadXML(string $source, int $options = 0): bool {
        if ($source === "") {
            throw new ValueError("DOMDocument::loadXML(): Argument #1 (\$source) must not be empty");
        }
        $loaded = elephc_xml_dom_load($this->__elephc_handle(), $source, strlen($source), $options) !== 0;
        __elephc_simplexml_report_text(elephc_xml_sx_diagnostics(), "DOMDocument::loadXML()", true);
        return $loaded;
    }

    public function load(string $filename, int $options = 0): bool {
        if ($filename === "") {
            throw new ValueError("DOMDocument::load(): Argument #1 (\$filename) must not be empty");
        }
        $loaded = elephc_xml_dom_load_file($this->__elephc_handle(), $filename, $options) !== 0;
        __elephc_simplexml_report_text(elephc_xml_sx_diagnostics(), "DOMDocument::load()", true);
        return $loaded;
    }

    public function loadHTML(string $source, int $options = 0): bool {
        if ($source === "") {
            throw new ValueError("DOMDocument::loadHTML(): Argument #1 (\$source) must not be empty");
        }
        $loaded = elephc_xml_dom_load_html($this->__elephc_handle(), $source, strlen($source), "", $options) !== 0;
        __elephc_simplexml_report_text(elephc_xml_sx_diagnostics(), "DOMDocument::loadHTML()", true);
        return $loaded;
    }

    public function loadHTMLFile(string $filename, int $options = 0): bool {
        if ($filename === "") {
            throw new ValueError("DOMDocument::loadHTMLFile(): Argument #1 (\$filename) must not be empty");
        }
        $loaded = elephc_xml_dom_load_html($this->__elephc_handle(), "", 0, $filename, $options) !== 0;
        __elephc_simplexml_report_text(elephc_xml_sx_diagnostics(), "DOMDocument::loadHTMLFile()", true);
        return $loaded;
    }

    public function saveXML(?DOMNode $node = null, int $options = 0): string|false {
        $target = $node === null ? 0 : $node->__elephc_handle();
        $xml = elephc_xml_dom_save($this->__elephc_handle(), $target, $this->formatOutput ? 1 : 0, $options & 0x2, $options & 0x4);
        return $xml === null ? false : $xml;
    }

    public function saveHTML(?DOMNode $node = null): string|false {
        $target = $node === null ? 0 : $node->__elephc_handle();
        $html = elephc_xml_dom_save_html($this->__elephc_handle(), $target, $this->formatOutput ? 1 : 0);
        return $html === null ? false : $html;
    }

    public function createElement(string $localName, string $value = "") {
        $node = __ElephcDOM::wrap(elephc_xml_dom_create($this->__elephc_handle(), 1, $localName, $value));
        if ($node instanceof DOMElement) {
            return $node;
        }
        throw new DOMException("Invalid State Error", 11);
    }

    public function createTextNode(string $data): DOMText {
        $node = __ElephcDOM::wrap(elephc_xml_dom_create($this->__elephc_handle(), 3, "", $data));
        if (!($node instanceof DOMText)) {
            throw new DOMException("Invalid State Error", 11);
        }
        return $node;
    }

    public function createComment(string $data): DOMComment {
        $node = __ElephcDOM::wrap(elephc_xml_dom_create($this->__elephc_handle(), 8, "", $data));
        if (!($node instanceof DOMComment)) {
            throw new DOMException("Invalid State Error", 11);
        }
        return $node;
    }

    public function createCDATASection(string $data) {
        $node = __ElephcDOM::wrap(elephc_xml_dom_create($this->__elephc_handle(), 4, "", $data));
        if ($node instanceof DOMCdataSection) {
            return $node;
        }
        throw new DOMException("Invalid State Error", 11);
    }

    public function createProcessingInstruction(string $target, string $data = "") {
        $node = __ElephcDOM::wrap(elephc_xml_dom_create($this->__elephc_handle(), 7, $target, $data));
        if ($node instanceof DOMProcessingInstruction) {
            return $node;
        }
        throw new DOMException("Invalid State Error", 11);
    }

    public function createAttribute(string $localName) {
        $node = __ElephcDOM::wrap(elephc_xml_dom_create($this->__elephc_handle(), 2, $localName, ""));
        if ($node instanceof DOMAttr) {
            return $node;
        }
        throw new DOMException("Invalid State Error", 11);
    }

    public function createDocumentFragment(): DOMDocumentFragment {
        $node = __ElephcDOM::wrap(elephc_xml_dom_create($this->__elephc_handle(), 11, "", ""));
        if (!($node instanceof DOMDocumentFragment)) {
            throw new DOMException("Invalid State Error", 11);
        }
        return $node;
    }

    public function getElementsByTagName(string $qualifiedName): DOMNodeList {
        return DOMNodeList::__elephc_list($this->__elephc_handle(), $qualifiedName);
    }

    public function append(...$nodes): void {
        foreach (__ElephcDOM::nodes($this, $nodes) as $node) {
            $this->appendChild($node);
        }
    }

    public function prepend(...$nodes): void {
        $current = $this->firstChild;
        $first = $current instanceof DOMNode ? $current : null;
        foreach (__ElephcDOM::nodes($this, $nodes) as $node) {
            $this->insertBefore($node, $first);
        }
    }
}
"##;
