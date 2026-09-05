//! Purpose:
//! Elephc-PHP source for the Termwind-facing DOM HTML subset: a small HTML
//! tokenizer/tree builder plus `DOMDocument` / `DOMNode` / `DOMElement` /
//! `DOMText` / `DOMComment` / `DOMNodeList`.
//!
//! Called from:
//! - `crate::dom_html_prelude::parsed_prelude`, which tokenizes and
//!   `parse_internal`s this fragment on first injection.
//!
//! Key details:
//! - This is an interim userland surface, not a second native libxml stack.
//!   Draft PR #654 (`crates/elephc-dom`, libxml2 + Lexbor) remains the path
//!   to full PHP 8.5 DOM. Same class names and `LIBXML_*` integers so that
//!   work can replace this prelude without changing Termwind call sites.
//! - Written as PHP (not `synthetic_class` builders) because the HTML walk
//!   is a few hundred lines of straightforward string/stack code; mysqli
//!   and curl still use the same delivery form.

/// The Termwind DOM HTML prelude fragment (no `<?php` header).
pub(super) const SRC: &str = r###"
function __elephc_dom_char(string $s, int $i): string {
    if ($i < 0 || $i >= strlen($s)) {
        return "";
    }
    return substr($s, $i, 1);
}

function __elephc_dom_is_ws(string $c): bool {
    return $c === " " || $c === "\t" || $c === "\n" || $c === "\r";
}

function __elephc_dom_is_name(string $c): bool {
    if ($c === "" ) {
        return false;
    }
    $_o = ord($c);
    if ($_o >= 65 && $_o <= 90) {
        return true;
    }
    if ($_o >= 97 && $_o <= 122) {
        return true;
    }
    if ($_o >= 48 && $_o <= 57) {
        return true;
    }
    return $c === "-" || $c === "_" || $c === ":";
}

function __elephc_dom_is_void(string $name): bool {
    return $name === "area" || $name === "base" || $name === "br" || $name === "col"
        || $name === "embed" || $name === "hr" || $name === "img" || $name === "input"
        || $name === "link" || $name === "meta" || $name === "param" || $name === "source"
        || $name === "track" || $name === "wbr";
}

function __elephc_dom_skip_ws(string $html, int $i): int {
    $_len = strlen($html);
    while ($i < $_len && __elephc_dom_is_ws(__elephc_dom_char($html, $i))) {
        $i = $i + 1;
    }
    return $i;
}

function __elephc_dom_read_name(string $html, int $i): array {
    $_start = $i;
    $_len = strlen($html);
    while ($i < $_len && __elephc_dom_is_name(__elephc_dom_char($html, $i))) {
        $i = $i + 1;
    }
    return [strtolower(substr($html, $_start, $i - $_start)), $i];
}

function __elephc_dom_read_attr_value(string $html, int $i): array {
    $_len = strlen($html);
    $i = __elephc_dom_skip_ws($html, $i);
    $_q = __elephc_dom_char($html, $i);
    if ($_q === "\"" || $_q === "'") {
        $i = $i + 1;
        $_start = $i;
        while ($i < $_len && __elephc_dom_char($html, $i) !== $_q) {
            $i = $i + 1;
        }
        $_val = html_entity_decode(substr($html, $_start, $i - $_start));
        if ($i < $_len) {
            $i = $i + 1;
        }
        return [$_val, $i];
    }
    $_start = $i;
    while ($i < $_len) {
        $_c = __elephc_dom_char($html, $i);
        if (__elephc_dom_is_ws($_c) || $_c === ">" || $_c === "/") {
            break;
        }
        $i = $i + 1;
    }
    return [html_entity_decode(substr($html, $_start, $i - $_start)), $i];
}

function __elephc_dom_read_attrs(string $html, int $i): array {
    $_attrs = [];
    $_self = 0;
    $_len = strlen($html);
    while ($i < $_len) {
        $i = __elephc_dom_skip_ws($html, $i);
        $_c = __elephc_dom_char($html, $i);
        if ($_c === "" || $_c === ">") {
            if ($_c === ">") {
                $i = $i + 1;
            }
            break;
        }
        if ($_c === "/") {
            $_self = 1;
            $i = $i + 1;
            $i = __elephc_dom_skip_ws($html, $i);
            if (__elephc_dom_char($html, $i) === ">") {
                $i = $i + 1;
            }
            break;
        }
        $_pair = __elephc_dom_read_name($html, $i);
        $_name = $_pair[0];
        $i = $_pair[1];
        if ($_name === "") {
            $i = $i + 1;
            continue;
        }
        $i = __elephc_dom_skip_ws($html, $i);
        $_val = "";
        if (__elephc_dom_char($html, $i) === "=") {
            $i = $i + 1;
            $_av = __elephc_dom_read_attr_value($html, $i);
            $_val = $_av[0];
            $i = $_av[1];
        }
        $_attrs[$_name] = $_val;
    }
    return [$_attrs, $_self, $i];
}

function __elephc_dom_tokenize(string $html, int $flags): array {
    $_tokens = [];
    $_i = 0;
    $_len = strlen($html);
    $_noblanks = ($flags & 256) !== 0;
    while ($_i < $_len) {
        $_c = __elephc_dom_char($html, $_i);
        if ($_c !== "<") {
            $_start = $_i;
            while ($_i < $_len && __elephc_dom_char($html, $_i) !== "<") {
                $_i = $_i + 1;
            }
            $_text = html_entity_decode(substr($html, $_start, $_i - $_start));
            if ($_noblanks && trim($_text) === "") {
                continue;
            }
            if ($_text !== "") {
                $_tokens[] = ["k" => "text", "v" => $_text];
            }
            continue;
        }
        if (substr($html, $_i, 4) === "<!--") {
            $_i = $_i + 4;
            $_end = strpos($html, "-->", $_i);
            if ($_end === false) {
                $_body = substr($html, $_i);
                $_i = $_len;
            } else {
                $_body = substr($html, $_i, $_end - $_i);
                $_i = $_end + 3;
            }
            $_tokens[] = ["k" => "comment", "v" => $_body];
            continue;
        }
        if (substr($html, $_i, 2) === "<!" || substr($html, $_i, 2) === "<?") {
            $_gt = strpos($html, ">", $_i);
            if ($_gt === false) {
                break;
            }
            $_i = $_gt + 1;
            continue;
        }
        if (substr($html, $_i, 2) === "</") {
            $_i = $_i + 2;
            $_pair = __elephc_dom_read_name($html, $_i);
            $_name = $_pair[0];
            $_i = $_pair[1];
            $_gt = strpos($html, ">", $_i);
            if ($_gt === false) {
                $_i = $_len;
            } else {
                $_i = $_gt + 1;
            }
            $_tokens[] = ["k" => "end", "n" => $_name];
            continue;
        }
        $_i = $_i + 1;
        $_pair = __elephc_dom_read_name($html, $_i);
        $_name = $_pair[0];
        $_i = $_pair[1];
        if ($_name === "") {
            $_tokens[] = ["k" => "text", "v" => "<"];
            continue;
        }
        $_ap = __elephc_dom_read_attrs($html, $_i);
        $_attrs = $_ap[0];
        $_self = $_ap[1];
        $_i = $_ap[2];
        if ($_self === 1 || __elephc_dom_is_void($_name)) {
            $_self = 1;
        }
        $_tokens[] = ["k" => "start", "n" => $_name, "a" => $_attrs, "void" => $_self];
    }
    return $_tokens;
}

function __elephc_dom_text_of(array $node): string {
    if ($node["kind"] === "text" || $node["kind"] === "comment") {
        return (string) $node["value"];
    }
    $_out = "";
    foreach ($node["children"] as $_child) {
        $_out = $_out . __elephc_dom_text_of($_child);
    }
    return $_out;
}

function __elephc_dom_new_node(string $kind, string $name, string $value, array $attrs, array $children): array {
    return [
        "kind" => $kind,
        "name" => $name,
        "value" => $value,
        "attrs" => $attrs,
        "children" => $children,
    ];
}

function __elephc_dom_append_child(array $stack, array $node): array {
    $_top = $stack[count($stack) - 1];
    $_children = $_top["children"];
    $_children[] = $node;
    $_top["children"] = $_children;
    $stack[count($stack) - 1] = $_top;
    return $stack;
}

function __elephc_dom_build_forest(array $tokens): array {
    $_stack = [];
    $_roots = [];
    foreach ($tokens as $_tok) {
        $_k = $_tok["k"];
        if ($_k === "text") {
            $_node = __elephc_dom_new_node("text", "#text", (string) $_tok["v"], [], []);
            if (count($_stack) === 0) {
                $_roots[] = $_node;
            } else {
                $_stack = __elephc_dom_append_child($_stack, $_node);
            }
        } elseif ($_k === "comment") {
            $_node = __elephc_dom_new_node("comment", "#comment", (string) $_tok["v"], [], []);
            if (count($_stack) === 0) {
                $_roots[] = $_node;
            } else {
                $_stack = __elephc_dom_append_child($_stack, $_node);
            }
        } elseif ($_k === "start") {
            $_node = __elephc_dom_new_node("element", (string) $_tok["n"], "", $_tok["a"], []);
            if ((int) $_tok["void"] === 1) {
                if (count($_stack) === 0) {
                    $_roots[] = $_node;
                } else {
                    $_stack = __elephc_dom_append_child($_stack, $_node);
                }
            } else {
                $_stack[] = $_node;
            }
        } else {
            if (count($_stack) === 0) {
                continue;
            }
            $_done = array_pop($_stack);
            if (count($_stack) === 0) {
                $_roots[] = $_done;
            } else {
                $_stack = __elephc_dom_append_child($_stack, $_done);
            }
        }
    }
    while (count($_stack) > 0) {
        $_done = array_pop($_stack);
        if (count($_stack) === 0) {
            $_roots[] = $_done;
        } else {
            $_stack = __elephc_dom_append_child($_stack, $_done);
        }
    }
    return $_roots;
}

function __elephc_dom_find_child(array $node, string $name): mixed {
    foreach ($node["children"] as $_child) {
        if ($_child["kind"] === "element" && $_child["name"] === $name) {
            return $_child;
        }
    }
    return null;
}

function __elephc_dom_wrap_html(array $roots): array {
    if (count($roots) === 1 && $roots[0]["kind"] === "element" && $roots[0]["name"] === "html") {
        $_html = $roots[0];
        if (__elephc_dom_find_child($_html, "body") === null) {
            $_html["children"] = [__elephc_dom_new_node("element", "body", "", [], $_html["children"])];
        }
        return $_html;
    }
    if (count($roots) === 1 && $roots[0]["kind"] === "element" && $roots[0]["name"] === "body") {
        return __elephc_dom_new_node("element", "html", "", [], $roots);
    }
    $_body = __elephc_dom_new_node("element", "body", "", [], $roots);
    return __elephc_dom_new_node("element", "html", "", [], [$_body]);
}

function __elephc_dom_parse_html(string $html, int $flags): array {
    $_tokens = __elephc_dom_tokenize($html, $flags);
    $_roots = __elephc_dom_build_forest($_tokens);
    $_html = __elephc_dom_wrap_html($_roots);
    $_html["value"] = __elephc_dom_text_of($_html);
    return __elephc_dom_new_node("document", "#document", $_html["value"], [], [$_html]);
}

function __elephc_dom_escape(string $s): string {
    return htmlspecialchars($s);
}

function __elephc_dom_serialize_node(mixed $node): string {
    if ($node instanceof DOMText) {
        return __elephc_dom_escape((string) $node->nodeValue);
    }
    if ($node instanceof DOMComment) {
        return "<!--" . (string) $node->nodeValue . "-->";
    }
    if ($node instanceof DOMDocument) {
        $_out = "";
        if ($node->childNodes !== null) {
            foreach ($node->childNodes as $_child) {
                $_out = $_out . __elephc_dom_serialize_node($_child);
            }
        }
        return $_out;
    }
    $_out = "<" . $node->nodeName;
    foreach ($node->__attrs as $_k => $_v) {
        $_out = $_out . " " . $_k . "=\"" . __elephc_dom_escape((string) $_v) . "\"";
    }
    $_inner = "";
    if ($node->childNodes !== null) {
        foreach ($node->childNodes as $_child) {
            $_inner = $_inner . __elephc_dom_serialize_node($_child);
        }
    }
    if ($_inner === "" && __elephc_dom_is_void($node->nodeName)) {
        return $_out . "/>";
    }
    return $_out . ">" . $_inner . "</" . $node->nodeName . ">";
}

function __elephc_dom_wire(array $siblings, mixed $parent): void {
    $_n = count($siblings);
    $_doc = $parent;
    if (!($parent instanceof DOMDocument)) {
        $_doc = $parent->ownerDocument;
    }
    for ($_i = 0; $_i < $_n; $_i++) {
        $_node = $siblings[$_i];
        $_node->parentNode = $parent;
        $_node->ownerDocument = $_doc;
        if ($_i > 0) {
            $_node->previousSibling = $siblings[$_i - 1];
        } else {
            $_node->previousSibling = null;
        }
        if ($_i + 1 < $_n) {
            $_node->nextSibling = $siblings[$_i + 1];
        } else {
            $_node->nextSibling = null;
        }
    }
}

function __elephc_dom_make_node(DOMDocument $doc, array $tree): mixed {
    $_kind = (string) $tree["kind"];
    if ($_kind === "text") {
        $_n = new DOMText();
        $_n->nodeName = "#text";
        $_n->nodeValue = (string) $tree["value"];
        $_n->ownerDocument = $doc;
        $_n->childNodes = new DOMNodeList([]);
        return $_n;
    }
    if ($_kind === "comment") {
        $_n = new DOMComment();
        $_n->nodeName = "#comment";
        $_n->nodeValue = (string) $tree["value"];
        $_n->ownerDocument = $doc;
        $_n->childNodes = new DOMNodeList([]);
        return $_n;
    }
    $_n = new DOMElement();
    $_n->nodeName = (string) $tree["name"];
    $_n->__attrs = $tree["attrs"];
    $_n->ownerDocument = $doc;
    $_kids = [];
    foreach ($tree["children"] as $_child) {
        $_kids[] = __elephc_dom_make_node($doc, $_child);
    }
    $_n->childNodes = new DOMNodeList($_kids);
    __elephc_dom_wire($_kids, $_n);
    $_n->nodeValue = __elephc_dom_text_of($tree);
    return $_n;
}

function __elephc_dom_collect(mixed $node, string $name): array {
    $_found = [];
    if ($node->childNodes === null) {
        return $_found;
    }
    foreach ($node->childNodes as $_child) {
        if ($_child instanceof DOMElement) {
            if ($name === "*" || $_child->nodeName === $name) {
                $_found[] = $_child;
            }
            $_more = __elephc_dom_collect($_child, $name);
            foreach ($_more as $_item) {
                $_found[] = $_item;
            }
        }
    }
    return $_found;
}

class DOMNode {
    public string $nodeName = "";
    public mixed $nodeValue = null;
    public mixed $childNodes = null;
    public mixed $previousSibling = null;
    public mixed $nextSibling = null;
    public mixed $parentNode = null;
    public mixed $ownerDocument = null;
    public array $__attrs = [];

    public function getAttribute(string $name): string {
        $_key = strtolower($name);
        if (array_key_exists($_key, $this->__attrs)) {
            return (string) $this->__attrs[$_key];
        }
        return "";
    }

    public function getElementsByTagName(string $qualifiedName): DOMNodeList {
        return new DOMNodeList(__elephc_dom_collect($this, strtolower($qualifiedName)));
    }
}

class DOMNodeList implements Iterator {
    public int $length = 0;
    public array $__items = [];
    private int $__i = 0;

    public function __construct(array $items = []) {
        $this->__items = $items;
        $this->length = count($items);
        $this->__i = 0;
    }

    public function item(int $index): mixed {
        if ($index < 0 || $index >= $this->length) {
            return null;
        }
        return $this->__items[$index];
    }

    public function rewind(): void {
        $this->__i = 0;
    }

    public function valid(): bool {
        return $this->__i < $this->length;
    }

    public function current(): mixed {
        return $this->__items[$this->__i];
    }

    public function key(): mixed {
        return $this->__i;
    }

    public function next(): void {
        $this->__i = $this->__i + 1;
    }
}

class DOMDocument extends DOMNode {
    public int $__elephc_flags = 0;

    public function __construct(string $version = "1.0", string $encoding = "") {
        $_unused_version = $version;
        $_unused_encoding = $encoding;
        $this->nodeName = "#document";
        $this->nodeValue = null;
        $this->childNodes = new DOMNodeList([]);
        $this->ownerDocument = $this;
    }

    public function loadHTML(string $source, int $options = 0): bool {
        $this->__elephc_flags = $options;
        $_tree = __elephc_dom_parse_html($source, $options);
        $this->nodeName = "#document";
        $this->ownerDocument = $this;
        $_kids = [];
        foreach ($_tree["children"] as $_child) {
            $_kids[] = __elephc_dom_make_node($this, $_child);
        }
        $this->childNodes = new DOMNodeList($_kids);
        __elephc_dom_wire($_kids, $this);
        $this->nodeValue = (string) $_tree["value"];
        return true;
    }

    public function saveXML(mixed $node = null): string {
        $_unused_flags = $this->__elephc_flags;
        if ($node === null) {
            return __elephc_dom_serialize_node($this);
        }
        return __elephc_dom_serialize_node($node);
    }
}

class DOMElement extends DOMNode {
}

class DOMCharacterData extends DOMNode {
}

class DOMText extends DOMCharacterData {
}

class DOMComment extends DOMCharacterData {
}
"###;
