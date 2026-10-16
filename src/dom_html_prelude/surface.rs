//! Purpose:
//! Elephc-PHP source for the Termwind-facing DOM HTML subset: a small HTML
//! tokenizer/tree builder plus `DOMDocument` / `DOMNode` / `DOMElement` /
//! `DOMText` / `DOMComment` / `DOMProcessingInstruction` / `DOMNodeList`.
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
//! - Attribute maps are stored as indexed `[name, value]` pairs. Nested
//!   string-keyed arrays lose their keys when they pass through mixed
//!   token/tree slots, which dropped `class` on Termwind's styled `div`.
//! - Sibling pointers are assigned through typed `DOMNode` parameters on
//!   `instanceof`-narrowed locals, then those locals are stored. Writes on
//!   `mixed` array elements copy the object and drop `nextSibling`.
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

function __elephc_dom_is_letter(string $c): bool {
    if ($c === "") {
        return false;
    }
    $_o = ord($c);
    return ($_o >= 65 && $_o <= 90) || ($_o >= 97 && $_o <= 122);
}

function __elephc_dom_starts_markup(string $html, int $i): bool {
    if (__elephc_dom_char($html, $i) !== "<") {
        return false;
    }
    $_next = __elephc_dom_char($html, $i + 1);
    if ($_next === "!") {
        return substr($html, $i, 4) === "<!--"
            || (strtoupper(substr($html, $i, 9)) === "<!DOCTYPE"
                && (__elephc_dom_is_ws(__elephc_dom_char($html, $i + 9))
                    || __elephc_dom_char($html, $i + 9) === ">"));
    }
    if ($_next === "/") {
        $_name_start = __elephc_dom_char($html, $i + 2);
        return __elephc_dom_is_letter($_name_start) || $_name_start === "_" || $_name_start === ":";
    }
    return __elephc_dom_is_letter($_next) || $_next === "?";
}

function __elephc_dom_is_boolean_attr(string $name): bool {
    return $name === "checked" || $name === "compact" || $name === "declare"
        || $name === "defer" || $name === "disabled" || $name === "ismap"
        || $name === "multiple" || $name === "nohref" || $name === "noresize"
        || $name === "noshade" || $name === "nowrap" || $name === "readonly"
        || $name === "selected";
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
        || $name === "hr" || $name === "img" || $name === "input"
        || $name === "link" || $name === "meta" || $name === "param";
}

function __elephc_dom_skip_ws(string $html, int $i): int {
    $_len = strlen($html);
    while ($i < $_len && __elephc_dom_is_ws(__elephc_dom_char($html, $i))) {
        $i = $i + 1;
    }
    return $i;
}

function __elephc_dom_read_name(string $html, int $i): mixed {
    $_start = $i;
    $_len = strlen($html);
    while ($i < $_len && __elephc_dom_is_name(__elephc_dom_char($html, $i))) {
        $i = $i + 1;
    }
    return [strtolower(substr($html, $_start, $i - $_start)), $i];
}

function __elephc_dom_read_attr_value(string $html, int $i): mixed {
    $_len = strlen($html);
    $i = __elephc_dom_skip_ws($html, $i);
    $_q = __elephc_dom_char($html, $i);
    if ($_q === "\"" || $_q === "'") {
        $i = $i + 1;
        $_start = $i;
        while ($i < $_len && __elephc_dom_char($html, $i) !== $_q) {
            $i = $i + 1;
        }
        $_val = __elephc_dom_decode_entities(substr($html, $_start, $i - $_start));
        if ($i < $_len) {
            $i = $i + 1;
        }
        return [$_val, $i];
    }
    $_start = $i;
    while ($i < $_len) {
        $_c = __elephc_dom_char($html, $i);
        if (__elephc_dom_is_ws($_c) || $_c === ">") {
            break;
        }
        $i = $i + 1;
    }
    return [__elephc_dom_decode_entities(substr($html, $_start, $i - $_start)), $i];
}

function __elephc_dom_read_attrs(string $html, int $i): mixed {
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
            $i = $i + 1;
            $i = __elephc_dom_skip_ws($html, $i);
            if (__elephc_dom_char($html, $i) === ">") {
                $_self = 1;
                $i = $i + 1;
                break;
            }
            continue;
        }
        $_pair = __elephc_dom_read_name($html, $i);
        $_name = (string) $_pair[0];
        $i = (int) $_pair[1];
        if ($_name === "") {
            $i = $i + 1;
            continue;
        }
        $i = __elephc_dom_skip_ws($html, $i);
        $_val = "";
        if (__elephc_dom_char($html, $i) === "=") {
            $i = $i + 1;
            $_av = __elephc_dom_read_attr_value($html, $i);
            $_val = (string) $_av[0];
            $i = (int) $_av[1];
        } elseif (__elephc_dom_is_boolean_attr($_name)) {
            $_val = $_name;
        }
        // Indexed [name, value] pairs: nested string-keyed maps lose their
        // keys when stored through mixed tree/token slots.
        if (!__elephc_dom_has_attr($_attrs, $_name)) {
            $_attrs[] = [$_name, $_val];
        }
    }
    return [$_attrs, $_self, $i];
}

// Presence is separate from value: the first empty attribute still suppresses duplicates.
function __elephc_dom_has_attr(mixed $attrs, string $name): bool {
    if (!is_array($attrs)) {
        return false;
    }
    foreach ($attrs as $_pair) {
        if (is_array($_pair) && (string) $_pair[0] === $name) {
            return true;
        }
    }
    return false;
}

function __elephc_dom_attr_get(mixed $attrs, string $name): string {
    if (!is_array($attrs)) {
        return "";
    }
    foreach ($attrs as $_pair) {
        if (is_array($_pair) && (string) $_pair[0] === $name) {
            return (string) $_pair[1];
        }
    }
    return "";
}

// Recovery can split one text run around a discarded delimiter, but not into separate nodes.
function __elephc_dom_append_text_token(array $tokens, string $text): array {
    if ($text === "") {
        return $tokens;
    }
    $_count = count($tokens);
    if ($_count > 0 && $tokens[$_count - 1]["k"] === "text") {
        $_last = $tokens[$_count - 1];
        $_last["v"] = (string) $_last["v"] . $text;
        $tokens[$_count - 1] = $_last;
    } else {
        $tokens[] = ["k" => "text", "v" => $text];
    }
    return $tokens;
}

function __elephc_dom_tokenize(string $html, int $flags): mixed {
    $_tokens = [];
    $_i = __elephc_dom_skip_ws($html, 0);
    $_len = strlen($html);
    $_unused_flags = $flags;
    while ($_i < $_len) {
        if (!__elephc_dom_starts_markup($html, $_i)) {
            $_start = $_i;
            $_text = "";
            while ($_i < $_len && !__elephc_dom_starts_markup($html, $_i)) {
                if (substr($html, $_i, 2) === "</") {
                    // Legacy libxml drops the invalid closing delimiter, not its following text.
                    $_text = $_text . substr($html, $_start, $_i - $_start);
                    $_i = $_i + 2;
                    $_start = $_i;
                    continue;
                }
                $_i = $_i + 1;
            }
            $_text = __elephc_dom_decode_entities($_text . substr($html, $_start, $_i - $_start));
            $_tokens = __elephc_dom_append_text_token($_tokens, $_text);
            continue;
        }
        if (substr($html, $_i, 4) === "<!--") {
            $_i = $_i + 4;
            $_comment_end = strpos($html, "-->", $_i);
            if ($_comment_end === false) {
                // Legacy libxml discards the unfinished comment and the remaining input.
                break;
            }
            $_body = substr($html, $_i, (int) $_comment_end - $_i);
            $_i = (int) $_comment_end + 3;
            $_tokens[] = ["k" => "comment", "v" => $_body];
            continue;
        }
        if (substr($html, $_i, 2) === "<?") {
            $_i = $_i + 2;
            $_start = $_i;
            $_first = __elephc_dom_char($html, $_i);
            if (!__elephc_dom_is_letter($_first) && $_first !== "_" && $_first !== ":") {
                continue;
            }
            while ($_i < $_len && (__elephc_dom_is_name(__elephc_dom_char($html, $_i))
                || __elephc_dom_char($html, $_i) === ".")) {
                $_i = $_i + 1;
            }
            $_target = substr($html, $_start, $_i - $_start);
            if ($_i === $_len) {
                // libxml keeps a bare unterminated target as text, without its opening delimiter.
                $_tokens = __elephc_dom_append_text_token($_tokens, $_target);
                continue;
            }
            $_data_start = __elephc_dom_skip_ws($html, $_i);
            $_pi_end = strpos($html, ">", $_data_start);
            if ($_pi_end === false) {
                break;
            }
            // Legacy HTML stops at >, so a preceding ? is part of the PI data.
            $_tokens[] = ["k" => "pi", "n" => $_target,
                "v" => substr($html, $_data_start, (int) $_pi_end - $_data_start)];
            $_i = (int) $_pi_end + 1;
            continue;
        }
        if (substr($html, $_i, 2) === "<!") {
            $_decl_end = strpos($html, ">", $_i);
            if ($_decl_end === false) {
                break;
            }
            $_i = (int) $_decl_end + 1;
            continue;
        }
        if (substr($html, $_i, 2) === "</") {
            $_i = $_i + 2;
        $_pair = __elephc_dom_read_name($html, $_i);
        $_name = (string) $_pair[0];
        $_i = (int) $_pair[1];
        $_close_end = strpos($html, ">", $_i);
            if ($_close_end === false) {
                $_i = $_len;
            } else {
                $_i = (int) $_close_end + 1;
            }
            $_tokens[] = ["k" => "end", "n" => $_name];
            continue;
        }
        $_i = $_i + 1;
        $_pair = __elephc_dom_read_name($html, $_i);
        $_name = (string) $_pair[0];
        $_i = (int) $_pair[1];
        if ($_name === "") {
            $_tokens = __elephc_dom_append_text_token($_tokens, "<");
            continue;
        }
        $_ap = __elephc_dom_read_attrs($html, $_i);
        $_attrs = $_ap[0];
        $_self = (int) $_ap[1];
        $_i = (int) $_ap[2];
        if ($_self === 1 || __elephc_dom_is_void($_name)) {
            $_self = 1;
        }
        $_tokens[] = ["k" => "start", "n" => $_name, "a" => $_attrs, "void" => $_self];
        if ($_self === 0 && ($_name === "script" || $_name === "style")) {
            $_start = $_i;
            while ($_i < $_len) {
                if (substr($html, $_i, 2) === "</"
                    && strtolower(substr($html, $_i + 2, strlen($_name))) === $_name) {
                    $_after = __elephc_dom_char($html, $_i + 2 + strlen($_name));
                    if ($_after === ">" || $_after === "/" || __elephc_dom_is_ws($_after)) {
                        break;
                    }
                }
                $_i++;
            }
            $_raw = substr($html, $_start, $_i - $_start);
            if ($_raw !== "") {
                $_tokens[] = ["k" => "cdata", "v" => $_raw];
            }
        }
    }
    return $_tokens;
}

function __elephc_dom_text_of(mixed $node): string {
    if ($node["kind"] === "text" || $node["kind"] === "cdata") {
        return (string) $node["value"];
    }
    if ($node["kind"] === "comment" || $node["kind"] === "pi") {
        return "";
    }
    $_out = "";
    foreach ($node["children"] as $_child) {
        $_out = $_out . __elephc_dom_text_of($_child);
    }
    return $_out;
}

function __elephc_dom_new_node(string $kind, string $name, string $value, mixed $attrs, mixed $children): mixed {
    return [
        "kind" => $kind,
        "name" => $name,
        "value" => $value,
        "attrs" => $attrs,
        "children" => $children,
    ];
}

function __elephc_dom_append_child(array $stack, mixed $node): array {
    $_top = $stack[count($stack) - 1];
    $_children = $_top["children"];
    // Ignored closing tags do not create a boundary between text siblings.
    $_count = count($_children);
    if ($node["kind"] === "text" && $_count > 0 && $_children[$_count - 1]["kind"] === "text") {
        $_last = $_children[$_count - 1];
        $_last["value"] = (string) $_last["value"] . (string) $node["value"];
        $_children[$_count - 1] = $_last;
    } else {
        $_children[] = $node;
    }
    $_top["children"] = $_children;
    $stack[count($stack) - 1] = $_top;
    return $stack;
}

function __elephc_dom_build_forest(mixed $tokens, int $flags): mixed {
    $_stack = [];
    $_roots = [];
    foreach ($tokens as $_tok) {
        $_k = $_tok["k"];
        if ($_k === "text" || $_k === "cdata") {
            if ($_k === "text" && ($flags & LIBXML_NOBLANKS) !== 0
                && trim((string) $_tok["v"]) === "" && count($_stack) > 0) {
                $_parent = $_stack[count($_stack) - 1];
                // Legacy HTML filters wrapper blanks, not title/body or other element text.
                if ($_parent["name"] === "html" || $_parent["name"] === "head") {
                    continue;
                }
            }
            $_node = __elephc_dom_new_node($_k, "#text", (string) $_tok["v"], [], []);
            if (count($_stack) === 0) {
                $_count = count($_roots);
                if ($_k === "text" && $_count > 0 && $_roots[$_count - 1]["kind"] === "text") {
                    $_last = $_roots[$_count - 1];
                    $_last["value"] = (string) $_last["value"] . (string) $_node["value"];
                    $_roots[$_count - 1] = $_last;
                } else {
                    $_roots[] = $_node;
                }
            } else {
                $_stack = __elephc_dom_append_child($_stack, $_node);
            }
        } elseif ($_k === "comment" || $_k === "pi") {
            $_name = "#comment";
            if ($_k === "pi") {
                $_name = (string) $_tok["n"];
            }
            $_node = __elephc_dom_new_node($_k, $_name, (string) $_tok["v"], [], []);
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
            $_match = -1;
            for ($_pos = count($_stack) - 1; $_pos >= 0; $_pos--) {
                if ($_stack[$_pos]["name"] === $_tok["n"]) {
                    $_match = $_pos;
                    break;
                }
            }
            if ($_match < 0) {
                continue;
            }
            while (count($_stack) > $_match) {
                $_done = array_pop($_stack);
                if (count($_stack) === 0) {
                    $_roots[] = $_done;
                } else {
                    $_stack = __elephc_dom_append_child($_stack, $_done);
                }
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

function __elephc_dom_find_child(mixed $node, string $name): mixed {
    foreach ($node["children"] as $_child) {
        if ($_child["kind"] === "element" && $_child["name"] === $name) {
            return $_child;
        }
    }
    return null;
}

function __elephc_dom_wrap_html(mixed $roots): mixed {
    if (count($roots) > 0 && $roots[0]["kind"] === "element" && $roots[0]["name"] === "html") {
        $_html = $roots[0];
        $_children = $_html["children"];
        if (__elephc_dom_find_child($_html, "body") === null) {
            // Imply a body only for original content, never around head or reopened roots.
            $_prefix_children = [];
            $_body_children = [];
            $_original_children = $_html["children"];
            for ($_child_index = 0; $_child_index < count($_original_children); $_child_index++) {
                $_child = $_original_children[$_child_index];
                $_prefix = $_child["kind"] === "comment" || $_child["kind"] === "pi"
                    || ($_child["kind"] === "element" && $_child["name"] === "head");
                if ($_child["kind"] === "text") {
                    $_text = (string) $_child["value"];
                    $_prefix = trim($_text) === "";
                }
                if (count($_body_children) === 0 && $_prefix) {
                    $_prefix_children[] = $_child;
                } else {
                    $_body_children[] = $_child;
                }
            }
            if (count($_body_children) > 0) {
                $_body = __elephc_dom_new_node("element", "body", "", [], $_body_children);
                $_prefix_children[] = $_body;
            }
            $_children = $_prefix_children;
        }
        if (count($roots) > 1) {
            // Legacy libxml reopens html beneath the original wrapper, not another body.
            $_later = [];
            for ($_i = 1; $_i < count($roots); $_i++) {
                $_later_root = $roots[$_i];
                $_later[] = $_later_root;
            }
            $_first_later = $_later[0];
            if (count($_later) === 1 && $_first_later["kind"] === "element" && $_first_later["name"] === "html") {
                $_reopened = $_first_later;
            } else {
                $_reopened = __elephc_dom_new_node("element", "html", "", [], $_later);
            }
            $_children[] = $_reopened;
        }
        $_html["children"] = $_children;
        return $_html;
    }
    if (count($roots) > 0 && $roots[0]["kind"] === "element" && $roots[0]["name"] === "body") {
        return __elephc_dom_new_node("element", "html", "", [], $roots);
    }
    $_body = __elephc_dom_new_node("element", "body", "", [], $roots);
    return __elephc_dom_new_node("element", "html", "", [], [$_body]);
}

function __elephc_dom_parse_html(string $html, int $flags): mixed {
    $_tokens = __elephc_dom_tokenize($html, $flags);
    $_roots = __elephc_dom_build_forest($_tokens, $flags);
    return __elephc_dom_document_from_roots($_roots, $flags);
}

// Keep wrapper selection separate from tokenization and balanced-tag construction.
function __elephc_dom_document_from_roots(mixed $roots, int $flags): mixed {
    $_document_children = [];
    $_content = [];
    $_trailing = [];
    $_closed_html = false;
    $_leading = true;
    foreach ($roots as $_root) {
        if ($_root["kind"] === "text" && ($_leading || $_closed_html)) {
            $_padding = (string) $_root["value"];
            if (trim($_padding) === "") {
                // Document padding is ignored even without fragment NOBLANKS filtering.
                continue;
            }
        }
        if ($_leading && ($_root["kind"] === "comment" || $_root["kind"] === "pi")) {
            $_document_children[] = $_root;
        } elseif ($_closed_html && ($_root["kind"] === "comment" || $_root["kind"] === "pi")) {
            // Nodes following an explicit document wrapper belong to the document, not its body.
            $_trailing[] = $_root;
        } else {
            $_leading = false;
            $_content[] = $_root;
            $_closed_html = count($_content) === 1 && $_root["kind"] === "element" && $_root["name"] === "html";
        }
    }
    if (($flags & LIBXML_NOBLANKS) !== 0) {
        while (count($_content) > 0) {
            $_last = $_content[count($_content) - 1];
            if ($_last["kind"] !== "text" || trim((string) $_last["value"]) !== "") {
                break;
            }
            // Wrapper blanks are filtered during construction; remove trailing document padding too.
            array_pop($_content);
        }
    }
    $_value = "";
    if (count($_content) > 0) {
        $_html = __elephc_dom_wrap_html($_content);
        $_value = __elephc_dom_text_of($_html);
        $_html["value"] = $_value;
        $_document_children[] = $_html;
    }
    foreach ($_trailing as $_node) {
        $_document_children[] = $_node;
    }
    return __elephc_dom_new_node("document", "#document", $_value, [], $_document_children);
}

function __elephc_dom_escape(string $s): string {
    return str_replace("\r", "&#13;", htmlspecialchars($s, ENT_NOQUOTES));
}

function __elephc_dom_escape_attr(string $s): string {
    $_escaped = str_replace("\r", "&#13;", htmlspecialchars($s, ENT_COMPAT));
    $_escaped = str_replace("\n", "&#10;", $_escaped);
    $_escaped = str_replace("\t", "&#9;", $_escaped);
    return __elephc_dom_escape_attr_non_ascii($_escaped);
}

// Insert numeric references after XML escaping so their ampersands are not escaped again.
function __elephc_dom_escape_attr_non_ascii(string $s): string {
    $_out = "";
    $_i = 0;
    $_length = strlen($s);
    while ($_i < $_length) {
        $_first = __elephc_dom_entity_byte($s, $_i);
        if ($_first < 194 || $_first > 244) {
            $_out = $_out . substr($s, $_i, 1);
            $_i++;
            continue;
        }
        $_width = 2;
        $_mask = 31;
        if ($_first >= 240) {
            $_width = 4;
            $_mask = 7;
        } elseif ($_first >= 224) {
            $_width = 3;
            $_mask = 15;
        }
        $_code = $_first & $_mask;
        $_valid = $_i + $_width <= $_length;
        for ($_offset = 1; $_offset < $_width; $_offset++) {
            $_next = __elephc_dom_entity_byte($s, $_i + $_offset);
            if ($_next < 128 || $_next > 191) {
                $_valid = false;
            }
            $_code = ($_code << 6) | ($_next & 63);
        }
        if (($_width === 3 && $_code < 2048) || ($_width === 4 && $_code < 65536)
            || ($_code >= 55296 && $_code <= 57343) || $_code > 1114111) {
            $_valid = false;
        }
        if (!$_valid) {
            // Encoding detection is outside this subset; do not drop undecodable input bytes.
            $_out = $_out . substr($s, $_i, 1);
            $_i++;
            continue;
        }
        $_out = $_out . "&#x" . strtoupper(dechex($_code)) . ";";
        $_i = $_i + $_width;
    }
    return $_out;
}

function __elephc_dom_serialize_node(mixed $node): string {
    if ($node instanceof DOMText) {
        if ($node->__elephc_raw_text) {
            return "<![CDATA[" . str_replace("]]>", "]]]]><![CDATA[>", (string) $node->nodeValue) . "]]>";
        }
        return __elephc_dom_escape((string) $node->nodeValue);
    }
    if ($node instanceof DOMComment) {
        return "<!--" . (string) $node->nodeValue . "-->";
    }
    if ($node instanceof DOMProcessingInstruction) {
        $_separator = "";
        if ((string) $node->nodeValue !== "") {
            $_separator = " ";
        }
        return "<?" . $node->nodeName . $_separator . (string) $node->nodeValue . "?>";
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
    $_attrs = $node->__attrs;
    if (is_array($_attrs)) {
        foreach ($_attrs as $_pair) {
            if (is_array($_pair)) {
                $_out = $_out . " " . (string) $_pair[0] . "=\"" . __elephc_dom_escape_attr((string) $_pair[1]) . "\"";
            }
        }
    }
    $_inner = "";
    if ($node->childNodes !== null) {
        foreach ($node->childNodes as $_child) {
            $_inner = $_inner . __elephc_dom_serialize_node($_child);
        }
    }
    if ($node->childNodes === null || $node->childNodes->length === 0) {
        return $_out . "/>";
    }
    return $_out . ">" . $_inner . "</" . $node->nodeName . ">";
}

function __elephc_dom_link_siblings(DOMNode $left, DOMNode $right): void {
    $left->nextSibling = $right;
    $right->previousSibling = $left;
}

function __elephc_dom_attach_parent(DOMNode $node, DOMNode $parent, mixed $doc): void {
    $node->parentNode = $parent;
    $node->ownerDocument = $doc;
}

function __elephc_dom_link_prev(int $has_prev, int $prev_kind, DOMElement $prev_el, DOMText $prev_text, DOMComment $prev_comment, DOMProcessingInstruction $prev_pi, DOMNode $right): void {
    if ($has_prev !== 1) {
        return;
    }
    if ($prev_kind === 1) {
        __elephc_dom_link_siblings($prev_el, $right);
    } elseif ($prev_kind === 2) {
        __elephc_dom_link_siblings($prev_text, $right);
    } elseif ($prev_kind === 3) {
        __elephc_dom_link_siblings($prev_comment, $right);
    } else {
        __elephc_dom_link_siblings($prev_pi, $right);
    }
}

function __elephc_dom_make_children(DOMDocument $doc, DOMNode $parent, mixed $trees): mixed {
    $_kids = [];
    $_has_prev = 0;
    $_prev_kind = 0;
    $_prev_el = new DOMElement();
    $_prev_text = new DOMText();
    $_prev_comment = new DOMComment();
    $_prev_pi = new DOMProcessingInstruction("");
    foreach ($trees as $_child) {
        $_kid = __elephc_dom_make_node($doc, $_child);
        if ($_kid instanceof DOMElement) {
            $_el_kid = $_kid;
            __elephc_dom_attach_parent($_el_kid, $parent, $doc);
            __elephc_dom_link_prev($_has_prev, $_prev_kind, $_prev_el, $_prev_text, $_prev_comment, $_prev_pi, $_el_kid);
            $_kids[] = $_el_kid;
            $_prev_el = $_el_kid;
            $_prev_kind = 1;
            $_has_prev = 1;
        } elseif ($_kid instanceof DOMText) {
            $_text_kid = $_kid;
            __elephc_dom_attach_parent($_text_kid, $parent, $doc);
            __elephc_dom_link_prev($_has_prev, $_prev_kind, $_prev_el, $_prev_text, $_prev_comment, $_prev_pi, $_text_kid);
            $_kids[] = $_text_kid;
            $_prev_text = $_text_kid;
            $_prev_kind = 2;
            $_has_prev = 1;
        } elseif ($_kid instanceof DOMComment) {
            $_comment_kid = $_kid;
            __elephc_dom_attach_parent($_comment_kid, $parent, $doc);
            __elephc_dom_link_prev($_has_prev, $_prev_kind, $_prev_el, $_prev_text, $_prev_comment, $_prev_pi, $_comment_kid);
            $_kids[] = $_comment_kid;
            $_prev_comment = $_comment_kid;
            $_prev_kind = 3;
            $_has_prev = 1;
        } elseif ($_kid instanceof DOMProcessingInstruction) {
            $_pi_kid = $_kid;
            __elephc_dom_attach_parent($_pi_kid, $parent, $doc);
            __elephc_dom_link_prev($_has_prev, $_prev_kind, $_prev_el, $_prev_text, $_prev_comment, $_prev_pi, $_pi_kid);
            $_kids[] = $_pi_kid;
            $_prev_pi = $_pi_kid;
            $_prev_kind = 4;
            $_has_prev = 1;
        }
    }
    return $_kids;
}

function __elephc_dom_make_node(DOMDocument $doc, mixed $tree): mixed {
    $_kind = (string) $tree["kind"];
    if ($_kind === "text" || $_kind === "cdata") {
        $_text = new DOMText();
        if ($_kind === "cdata") {
            $_text->nodeName = "#cdata-section";
            $_text->__elephc_raw_text = true;
        } else {
            $_text->nodeName = "#text";
        }
        $_text->nodeValue = (string) $tree["value"];
        $_text->ownerDocument = $doc;
        $_text->childNodes = new DOMNodeList([]);
        return $_text;
    }
    if ($_kind === "comment") {
        $_comment = new DOMComment();
        $_comment->nodeName = "#comment";
        $_comment->nodeValue = (string) $tree["value"];
        $_comment->ownerDocument = $doc;
        $_comment->childNodes = new DOMNodeList([]);
        return $_comment;
    }
    if ($_kind === "pi") {
        $_pi = new DOMProcessingInstruction((string) $tree["name"], (string) $tree["value"]);
        $_pi->ownerDocument = $doc;
        return $_pi;
    }
    $_el = new DOMElement();
    $_el->nodeName = (string) $tree["name"];
    $_el->__attrs = $tree["attrs"];
    $_el->ownerDocument = $doc;
    $_kids = __elephc_dom_make_children($doc, $_el, $tree["children"]);
    $_el->childNodes = new DOMNodeList($_kids);
    $_el->nodeValue = __elephc_dom_text_of($tree);
    return $_el;
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
    public mixed $__attrs = [];

    public function getAttribute(string $name): string {
        return __elephc_dom_attr_get($this->__attrs, $name);
    }

    public function getElementsByTagName(string $qualifiedName): DOMNodeList {
        return new DOMNodeList(__elephc_dom_collect($this, $qualifiedName));
    }
}

class __ElephcDOMNodeListIterator implements Iterator {
    private mixed $__items = [];
    private int $__length = 0;
    private int $__i = 0;

    public function __construct(mixed $items, int $length) {
        $this->__items = $items;
        $this->__length = $length;
    }

    public function rewind(): void { $this->__i = 0; }
    public function valid(): bool { return $this->__i < $this->__length; }
    public function key(): mixed { return $this->__i; }
    public function next(): void { $this->__i = $this->__i + 1; }

    public function current(): mixed {
        $_items = $this->__items;
        if (!is_array($_items)) { return null; }
        return $_items[$this->__i];
    }
}

class DOMNodeList implements IteratorAggregate, Countable {
    public int $length = 0;
    public mixed $__items = [];

    public function __construct(mixed $items = []) {
        $_list = [];
        if (is_array($items)) {
            $_list = $items;
        }
        $this->__items = $_list;
        $this->length = count($_list);
    }

    public function item(int $index): mixed {
        if ($index < 0 || $index >= $this->length) {
            return null;
        }
        $_items = $this->__items;
        if (!is_array($_items)) {
            return null;
        }
        return $_items[$index];
    }

    public function getIterator(): Iterator {
        return new __ElephcDOMNodeListIterator($this->__items, $this->length);
    }

    public function count(): int {
        return $this->length;
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
        $this->ownerDocument = null;
    }

    public function loadHTML(string $source, int $options = 0): bool {
        $this->__elephc_flags = $options;
        $_tree = __elephc_dom_parse_html($source, $options);
        $this->nodeName = "#document";
        $this->ownerDocument = null;
        $_kids = __elephc_dom_make_children($this, $this, $_tree["children"]);
        $this->childNodes = new DOMNodeList($_kids);
        $this->nodeValue = null;
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
    public bool $__elephc_raw_text = false;
}

class DOMComment extends DOMCharacterData {
}

class DOMProcessingInstruction extends DOMNode {
    public string $target = "";
    public string $data = "";

    public function __construct(string $target, string $data = "") {
        $this->target = $target;
        $this->data = $data;
        $this->nodeName = $target;
        $this->nodeValue = $data;
        $this->childNodes = new DOMNodeList([]);
    }
}
"###;
