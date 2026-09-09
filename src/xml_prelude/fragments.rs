//! Purpose:
//! The PHP form of the xml prelude, kept as the parse-parity oracle for the built
//! declarations in `crate::xml_prelude::build` and as the readable reference for the
//! dispatch rules the surface implements.
//!
//! Called from:
//! - `crate::xml_prelude::oracle_tests` and the transcription driver
//!   (`synthetic_class::transcribe::tests::dump_prelude_on_request`).
//!
//! Key details:
//! - NOT compiled into the prelude: `build.rs` carries the same declarations as AST. When
//!   this text changes, re-transcribe and update the builders, or the oracle fails.
//! - The `XMLParser` dispatch mirrors php-src `ext/xml/compat.c` + `xml.c`: C-level
//!   "installed" flags route events, PHP-level callables receive them, entity references
//!   follow `get_entity()`, and `xml_parse_into_struct()` accumulates like
//!   `xml_startElementHandler` / `xml_characterDataHandler`.

/// The complete xml prelude as PHP source (with its `<?php` header).
pub(super) const SRC: &str = r##"<?php

// -- elephc_xml bridge --

extern "elephc_xml" {
    function elephc_xml_parser_create(int $namespaces, string $separator): int;
    function elephc_xml_parser_free(int $parser): int;
    function elephc_xml_parser_set_option(int $parser, int $option, int $value): int;
    function elephc_xml_parser_get_option(int $parser, int $option): int;
    function elephc_xml_parser_set_target_encoding(int $parser, string $name): int;
    function elephc_xml_encoding_supported(string $name): int;
    function elephc_xml_parser_target_encoding(int $parser): string;
    function elephc_xml_parser_feed(int $parser, string $data, int $len, int $is_final): int;
    function elephc_xml_parser_next(int $parser): int;
    function elephc_xml_parser_event_string(int $parser, int $field): string;
    function elephc_xml_parser_event_int(int $parser, int $field): int;
    function elephc_xml_parser_event_attr_name(int $parser, int $index): string;
    function elephc_xml_parser_event_attr_value(int $parser, int $index): string;
    function elephc_xml_parser_event_ns_has_prefix(int $parser, int $index): int;
    function elephc_xml_parser_event_ns_prefix(int $parser, int $index): string;
    function elephc_xml_parser_event_ns_uri(int $parser, int $index): string;
    function elephc_xml_parser_error_code(int $parser): int;
    function elephc_xml_parser_well_formed(int $parser): int;
    function elephc_xml_parser_line(int $parser): int;
    function elephc_xml_parser_column(int $parser): int;
    function elephc_xml_parser_byte_index(int $parser): int;
    function elephc_xml_parser_stop(int $parser, int $code): int;
    function elephc_xml_error_string(int $code): string;
    function elephc_xml_writer_create(): int;
    function elephc_xml_writer_free(int $writer): int;
    function elephc_xml_writer_valid_name(string $name): int;
    function elephc_xml_writer_set_indent(int $writer, int $enable): int;
    function elephc_xml_writer_set_indent_string(int $writer, string $indent): int;
    function elephc_xml_writer_start_document(int $writer, int $has_version, string $version, int $has_encoding, string $encoding, int $has_standalone, string $standalone): int;
    function elephc_xml_writer_end_document(int $writer): int;
    function elephc_xml_writer_start_comment(int $writer): int;
    function elephc_xml_writer_end_comment(int $writer): int;
    function elephc_xml_writer_write_comment(int $writer, string $content): int;
    function elephc_xml_writer_start_element(int $writer, string $name): int;
    function elephc_xml_writer_start_element_ns(int $writer, int $has_prefix, string $prefix, string $name, int $has_uri, string $uri): int;
    function elephc_xml_writer_end_element(int $writer): int;
    function elephc_xml_writer_full_end_element(int $writer): int;
    function elephc_xml_writer_write_element(int $writer, string $name, int $has_content, string $content): int;
    function elephc_xml_writer_write_element_ns(int $writer, int $has_prefix, string $prefix, string $name, int $has_uri, string $uri, int $has_content, string $content): int;
    function elephc_xml_writer_start_attribute(int $writer, string $name): int;
    function elephc_xml_writer_start_attribute_ns(int $writer, int $has_prefix, string $prefix, string $name, int $has_uri, string $uri): int;
    function elephc_xml_writer_end_attribute(int $writer): int;
    function elephc_xml_writer_write_attribute(int $writer, string $name, string $value): int;
    function elephc_xml_writer_write_attribute_ns(int $writer, int $has_prefix, string $prefix, string $name, int $has_uri, string $uri, string $value): int;
    function elephc_xml_writer_start_pi(int $writer, string $target): int;
    function elephc_xml_writer_end_pi(int $writer): int;
    function elephc_xml_writer_write_pi(int $writer, string $target, string $content): int;
    function elephc_xml_writer_start_cdata(int $writer): int;
    function elephc_xml_writer_end_cdata(int $writer): int;
    function elephc_xml_writer_write_cdata(int $writer, string $content): int;
    function elephc_xml_writer_text(int $writer, string $content): int;
    function elephc_xml_writer_write_raw(int $writer, string $content): int;
    function elephc_xml_writer_start_dtd(int $writer, string $name, int $has_public_id, string $public_id, int $has_system_id, string $system_id): int;
    function elephc_xml_writer_end_dtd(int $writer): int;
    function elephc_xml_writer_write_dtd(int $writer, string $name, int $has_public_id, string $public_id, int $has_system_id, string $system_id, int $has_content, string $content): int;
    function elephc_xml_writer_start_dtd_element(int $writer, string $name): int;
    function elephc_xml_writer_end_dtd_element(int $writer): int;
    function elephc_xml_writer_write_dtd_element(int $writer, string $name, string $content): int;
    function elephc_xml_writer_start_dtd_attlist(int $writer, string $name): int;
    function elephc_xml_writer_end_dtd_attlist(int $writer): int;
    function elephc_xml_writer_write_dtd_attlist(int $writer, string $name, string $content): int;
    function elephc_xml_writer_start_dtd_entity(int $writer, string $name, int $is_param): int;
    function elephc_xml_writer_end_dtd_entity(int $writer): int;
    function elephc_xml_writer_write_dtd_entity(int $writer, string $name, string $content, int $is_param, int $flags, string $public_id, string $system_id, string $notation): int;
    function elephc_xml_writer_output(int $writer): string;
    function elephc_xml_writer_take_output(int $writer): string;
    function elephc_xml_writer_output_len(int $writer): int;
    function elephc_xml_writer_output_has_nul(int $writer): int;
    function elephc_xml_writer_output_hex(int $writer, int $take): string;
}

// -- ext/xml: the XMLParser object --

// PHP 8 models a parser as a final XMLParser object minted by xml_parser_create() /
// xml_parser_create_ns(); `new XMLParser()` throws. The bridge parser lives behind the
// integer handle; every PHP-visible handler slot, the xml_set_object() target and the
// xml_parse_into_struct() accumulation state live here, so the dispatch rules of
// php-src's ext/xml/compat.c and xml.c are written once, in PHP, and shared by AOT and eval.
final class XMLParser {
    private static bool $__elephc_minting = false;

    public int $__elephc_handle = 0;
    public mixed $__elephc_object = null;
    public mixed $__elephc_start_handler = null;
    public mixed $__elephc_end_handler = null;
    public mixed $__elephc_cdata_handler = null;
    public mixed $__elephc_pi_handler = null;
    public mixed $__elephc_default_handler = null;
    public mixed $__elephc_unparsed_handler = null;
    public mixed $__elephc_notation_handler = null;
    public mixed $__elephc_extref_handler = null;
    public mixed $__elephc_start_ns_handler = null;
    public mixed $__elephc_end_ns_handler = null;
    // Method names bound through xml_set_object(), one per slot, "" when the slot holds an
    // ordinary callable; xml_set_object() re-targets these on the new object.
    public string $__elephc_start_method = "";
    public string $__elephc_end_method = "";
    public string $__elephc_cdata_method = "";
    public string $__elephc_pi_method = "";
    public string $__elephc_default_method = "";
    public string $__elephc_unparsed_method = "";
    public string $__elephc_notation_method = "";
    public string $__elephc_extref_method = "";
    public string $__elephc_start_ns_method = "";
    public string $__elephc_end_ns_method = "";
    // php-src installs its C-level SAX callbacks when the matching xml_set_*_handler() is
    // called, even with a null PHP handler, and compat.c routes events on those C-level
    // flags (a start tag reaches the default handler only while no element handler was ever
    // installed). These mirror that installed-ness independently of the PHP callables.
    public bool $__elephc_element_installed = false;
    public bool $__elephc_cdata_installed = false;
    public bool $__elephc_pi_installed = false;
    public bool $__elephc_default_installed = false;
    public bool $__elephc_unparsed_installed = false;
    public bool $__elephc_notation_installed = false;
    public bool $__elephc_extref_installed = false;
    public bool $__elephc_start_ns_installed = false;
    public int $__elephc_skip_tagstart = 0;
    public bool $__elephc_skip_white = false;
    public bool $__elephc_parsing = false;
    public int $__elephc_level = 0;
    // xml_parse_into_struct() state (php-src: parser->data / info / ltags / lastwasopen /
    // ctag_index / curtag).
    public bool $__elephc_collecting = false;
    public bool $__elephc_collect_index = false;
    public array $__elephc_struct_values = [];
    public array $__elephc_struct_index = [];
    public array $__elephc_ltags = [];
    public bool $__elephc_lastwasopen = false;
    public int $__elephc_ctag_index = 0;
    public int $__elephc_struct_next = 0;
    public int $__elephc_curtag = 0;

    public function __construct() {
        if (!self::$__elephc_minting) {
            throw new Error("Cannot directly construct XMLParser, use xml_parser_create() or xml_parser_create_ns() instead");
        }
    }

    public function __destruct() {
        $raw = $this->__elephc_handle;
        if ($raw !== 0) {
            $this->__elephc_handle = 0;
            elephc_xml_parser_free($raw);
        }
    }

    // PHP marks XMLParser uncloneable: `clone $parser` throws an Error instead of minting a
    // second object. elephc clones shallowly first and only then runs this hook on the copy,
    // so at this point $this is the copy and already holds the original's bridge handle.
    // Dropping that handle BEFORE throwing means the copy's destructor (which skips handle
    // 0) can never free the parser out from under the original. `final` keeps a subclass
    // from overriding the guard (PHP throws for subclasses too, before any hook runs).
    final public function __clone(): void {
        $this->__elephc_handle = 0;
        throw new Error("Trying to clone an uncloneable object of class " . get_class($this));
    }

    public function __debugInfo(): array {
        return [];
    }

    public function __serialize(): array {
        throw new Exception("Serialization of 'XMLParser' is not allowed");
    }

    public function __unserialize(array $data): void {
        throw new Exception("Unserialization of 'XMLParser' is not allowed");
    }

    // Mints a parser for xml_parser_create() ($namespaces false) or xml_parser_create_ns().
    // Only the first byte of $separator is used, exactly like php-src's compat layer.
    public static function __elephc_create(string $function, ?string $encoding, bool $namespaces, string $separator): XMLParser {
        $target = $encoding ?? "";
        if ($target !== "" && elephc_xml_encoding_supported($target) === 0) {
            throw new ValueError($function . "(): Argument #1 (\$encoding) is not a supported source encoding");
        }
        self::$__elephc_minting = true;
        $parser = new self();
        self::$__elephc_minting = false;
        $raw = elephc_xml_parser_create($namespaces ? 1 : 0, substr($separator, 0, 1));
        $parser->__elephc_handle = $raw;
        if ($target !== "") {
            elephc_xml_parser_set_target_encoding($raw, $target);
        }
        return $parser;
    }

    // Normalizes one handler argument (callable|string|null) into the stored callable and
    // the method name it was bound from, applying php-src's rules: null or "" clears the
    // slot, a callable is kept as is, and any other string names a method of the object
    // registered through xml_set_object().
    public function __elephc_bind_handler(mixed $handler, string $function, int $argument, string $parameter): array {
        if ($handler === null || $handler === "") {
            return [null, ""];
        }
        if (is_callable($handler)) {
            return [$handler, ""];
        }
        // php-src's second parse attempt is coercive: a scalar becomes the method name
        // (`false` becomes "", which clears the handler).
        if (is_int($handler) || is_float($handler) || is_bool($handler)) {
            $handler = (string) $handler;
            if ($handler === "") {
                return [null, ""];
            }
        }
        if (!is_string($handler)) {
            throw new TypeError($function . "(): Argument #" . $argument . " (\$" . $parameter . ") must be of type callable|string|null");
        }
        $object = $this->__elephc_object;
        if ($object === null) {
            throw new ValueError($function . "(): Argument #" . $argument . " (\$" . $parameter . ") an object must be set via xml_set_object() to be able to lookup method");
        }
        // Bound methods are resolved through the callable probe, which sees the public
        // methods php-src's function-table lookup would find.
        if (!is_callable([$object, $handler])) {
            throw new ValueError($function . "(): Argument #" . $argument . " (\$" . $parameter . ") method " . get_class($object) . "::" . $handler . "() does not exist");
        }
        // The method name is kept as the caller spelled it; php-src stores the declared
        // spelling, which only its xml_set_object() swap diagnostic reveals.
        return [[$object, $handler], $handler];
    }

    // xml_set_object(): every method-name handler is re-targeted on the new object, which
    // must declare each bound method.
    public function __elephc_set_object(mixed $object): bool {
        if (!is_object($object)) {
            throw new TypeError("xml_set_object(): Argument #2 (\$object) must be of type object, " . gettype($object) . " given");
        }
        $methods = [
            $this->__elephc_start_method, $this->__elephc_end_method, $this->__elephc_cdata_method,
            $this->__elephc_pi_method, $this->__elephc_default_method, $this->__elephc_unparsed_method,
            $this->__elephc_notation_method, $this->__elephc_extref_method, $this->__elephc_start_ns_method,
            $this->__elephc_end_ns_method,
        ];
        $setters = [
            "xml_set_element_handler", "xml_set_element_handler", "xml_set_character_data_handler",
            "xml_set_processing_instruction_handler", "xml_set_default_handler", "xml_set_unparsed_entity_decl_handler",
            "xml_set_notation_decl_handler", "xml_set_external_entity_ref_handler", "xml_set_start_namespace_decl_handler",
            "xml_set_end_namespace_decl_handler",
        ];
        foreach ($methods as $slot => $method) {
            if ($method !== "" && !is_callable([$object, $method])) {
                throw new ValueError("xml_set_object(): Argument #2 (\$object) cannot safely swap to object of class " . get_class($object) . " as method \"" . $method . "\" does not exist, which was set via " . $setters[$slot] . "()");
            }
        }
        $this->__elephc_object = $object;
        if ($this->__elephc_start_method !== "") {
            $this->__elephc_start_handler = [$object, $this->__elephc_start_method];
        }
        if ($this->__elephc_end_method !== "") {
            $this->__elephc_end_handler = [$object, $this->__elephc_end_method];
        }
        if ($this->__elephc_cdata_method !== "") {
            $this->__elephc_cdata_handler = [$object, $this->__elephc_cdata_method];
        }
        if ($this->__elephc_pi_method !== "") {
            $this->__elephc_pi_handler = [$object, $this->__elephc_pi_method];
        }
        if ($this->__elephc_default_method !== "") {
            $this->__elephc_default_handler = [$object, $this->__elephc_default_method];
        }
        if ($this->__elephc_unparsed_method !== "") {
            $this->__elephc_unparsed_handler = [$object, $this->__elephc_unparsed_method];
        }
        if ($this->__elephc_notation_method !== "") {
            $this->__elephc_notation_handler = [$object, $this->__elephc_notation_method];
        }
        if ($this->__elephc_extref_method !== "") {
            $this->__elephc_extref_handler = [$object, $this->__elephc_extref_method];
        }
        if ($this->__elephc_start_ns_method !== "") {
            $this->__elephc_start_ns_handler = [$object, $this->__elephc_start_ns_method];
        }
        if ($this->__elephc_end_ns_method !== "") {
            $this->__elephc_end_ns_handler = [$object, $this->__elephc_end_ns_method];
        }
        return true;
    }

    public function __elephc_set_element_handler(mixed $start_handler, mixed $end_handler): bool {
        $start = $this->__elephc_bind_handler($start_handler, "xml_set_element_handler", 2, "start_handler");
        $end = $this->__elephc_bind_handler($end_handler, "xml_set_element_handler", 3, "end_handler");
        $this->__elephc_start_handler = $start[0];
        $this->__elephc_start_method = (string) $start[1];
        $this->__elephc_end_handler = $end[0];
        $this->__elephc_end_method = (string) $end[1];
        $this->__elephc_element_installed = true;
        return true;
    }

    public function __elephc_set_character_data_handler(mixed $handler): bool {
        $bound = $this->__elephc_bind_handler($handler, "xml_set_character_data_handler", 2, "handler");
        $this->__elephc_cdata_handler = $bound[0];
        $this->__elephc_cdata_method = (string) $bound[1];
        $this->__elephc_cdata_installed = true;
        return true;
    }

    public function __elephc_set_processing_instruction_handler(mixed $handler): bool {
        $bound = $this->__elephc_bind_handler($handler, "xml_set_processing_instruction_handler", 2, "handler");
        $this->__elephc_pi_handler = $bound[0];
        $this->__elephc_pi_method = (string) $bound[1];
        $this->__elephc_pi_installed = true;
        return true;
    }

    public function __elephc_set_default_handler(mixed $handler): bool {
        $bound = $this->__elephc_bind_handler($handler, "xml_set_default_handler", 2, "handler");
        $this->__elephc_default_handler = $bound[0];
        $this->__elephc_default_method = (string) $bound[1];
        $this->__elephc_default_installed = true;
        return true;
    }

    public function __elephc_set_unparsed_entity_decl_handler(mixed $handler): bool {
        $bound = $this->__elephc_bind_handler($handler, "xml_set_unparsed_entity_decl_handler", 2, "handler");
        $this->__elephc_unparsed_handler = $bound[0];
        $this->__elephc_unparsed_method = (string) $bound[1];
        $this->__elephc_unparsed_installed = true;
        return true;
    }

    public function __elephc_set_notation_decl_handler(mixed $handler): bool {
        $bound = $this->__elephc_bind_handler($handler, "xml_set_notation_decl_handler", 2, "handler");
        $this->__elephc_notation_handler = $bound[0];
        $this->__elephc_notation_method = (string) $bound[1];
        $this->__elephc_notation_installed = true;
        return true;
    }

    public function __elephc_set_external_entity_ref_handler(mixed $handler): bool {
        $bound = $this->__elephc_bind_handler($handler, "xml_set_external_entity_ref_handler", 2, "handler");
        $this->__elephc_extref_handler = $bound[0];
        $this->__elephc_extref_method = (string) $bound[1];
        $this->__elephc_extref_installed = true;
        return true;
    }

    public function __elephc_set_start_namespace_decl_handler(mixed $handler): bool {
        $bound = $this->__elephc_bind_handler($handler, "xml_set_start_namespace_decl_handler", 2, "handler");
        $this->__elephc_start_ns_handler = $bound[0];
        $this->__elephc_start_ns_method = (string) $bound[1];
        $this->__elephc_start_ns_installed = true;
        return true;
    }

    // Kept for API completeness: PHP's libxml-backed ext/xml never invokes the end
    // namespace declaration handler, and neither does this implementation.
    public function __elephc_set_end_namespace_decl_handler(mixed $handler): bool {
        $bound = $this->__elephc_bind_handler($handler, "xml_set_end_namespace_decl_handler", 2, "handler");
        $this->__elephc_end_ns_handler = $bound[0];
        $this->__elephc_end_ns_method = (string) $bound[1];
        return true;
    }

    public function __elephc_set_option(int $option, mixed $value): bool {
        $raw = $this->__elephc_handle;
        if ($option === 1) {
            elephc_xml_parser_set_option($raw, 1, $value ? 1 : 0);
            return true;
        }
        if ($option === 4) {
            $this->__elephc_skip_white = (bool) $value;
            return true;
        }
        if ($option === 5) {
            if ($this->__elephc_parsing) {
                throw new Error("Cannot change option XML_OPTION_PARSE_HUGE while parsing");
            }
            elephc_xml_parser_set_option($raw, 5, $value ? 1 : 0);
            return true;
        }
        if ($option === 3) {
            $offset = (int) $value;
            if ($offset < 0 || $offset > 2147483647) {
                return false;
            }
            $this->__elephc_skip_tagstart = $offset;
            return true;
        }
        if ($option === 2) {
            $name = (string) $value;
            if (elephc_xml_parser_set_target_encoding($raw, $name) === 0) {
                throw new ValueError("xml_parser_set_option(): Argument #3 (\$value) is not a supported target encoding");
            }
            return true;
        }
        throw new ValueError("xml_parser_set_option(): Argument #2 (\$option) must be a XML_OPTION_* constant");
    }

    public function __elephc_get_option(int $option): mixed {
        $raw = $this->__elephc_handle;
        if ($option === 1) {
            return elephc_xml_parser_get_option($raw, 1) === 1;
        }
        if ($option === 3) {
            return $this->__elephc_skip_tagstart;
        }
        if ($option === 4) {
            return $this->__elephc_skip_white;
        }
        if ($option === 5) {
            return elephc_xml_parser_get_option($raw, 5) === 1;
        }
        if ($option === 2) {
            return elephc_xml_parser_target_encoding($raw);
        }
        throw new ValueError("xml_parser_get_option(): Argument #2 (\$option) must be a XML_OPTION_* constant");
    }

    public function __elephc_error_code(): int {
        $raw = $this->__elephc_handle;
        return elephc_xml_parser_error_code($raw);
    }

    public function __elephc_line(): int {
        $raw = $this->__elephc_handle;
        return elephc_xml_parser_line($raw);
    }

    public function __elephc_column(): int {
        $raw = $this->__elephc_handle;
        return elephc_xml_parser_column($raw);
    }

    public function __elephc_byte_index(): int {
        $raw = $this->__elephc_handle;
        return elephc_xml_parser_byte_index($raw);
    }

    // xml_parse(): feeds one chunk and drains every event it completes. A handler that
    // throws stops the parser the way php-src's xmlStopParser does, and the exception
    // propagates to the caller.
    public function __elephc_parse(string $data, bool $is_final): int {
        if ($this->__elephc_parsing) {
            throw new Error("Parser must not be called recursively");
        }
        $raw = $this->__elephc_handle;
        elephc_xml_parser_feed($raw, $data, strlen($data), $is_final ? 1 : 0);
        $this->__elephc_parsing = true;
        $completed = false;
        try {
            $this->__elephc_drain();
            $completed = true;
        } finally {
            $this->__elephc_parsing = false;
            if (!$completed) {
                // php-src's handlers return early while an exception is pending, so libxml2
                // still consumes the rest of the chunk (positions advance, a final chunk ends
                // the document) before xml_parse() lets the exception out.
                $this->__elephc_discard();
            }
        }
        return elephc_xml_parser_well_formed($raw);
    }

    // Consumes the buffered events without dispatching them (see __elephc_parse()).
    private function __elephc_discard(): void {
        $raw = $this->__elephc_handle;
        while (elephc_xml_parser_next($raw) > 0) {
        }
    }

    // xml_parse_into_struct(): the same drain with the struct-building side installed
    // (php-src installs its element and character-data C handlers for the call).
    public function __elephc_parse_into_struct(string $data, bool $with_index): int {
        if ($this->__elephc_parsing) {
            throw new Error("Parser must not be called recursively");
        }
        $this->__elephc_collecting = true;
        $this->__elephc_collect_index = $with_index;
        $this->__elephc_struct_values = [];
        $this->__elephc_struct_index = [];
        $this->__elephc_ltags = [];
        $this->__elephc_level = 0;
        $this->__elephc_lastwasopen = false;
        $this->__elephc_ctag_index = 0;
        $this->__elephc_struct_next = 0;
        $this->__elephc_element_installed = true;
        $this->__elephc_cdata_installed = true;
        try {
            $status = $this->__elephc_parse($data, true);
        } finally {
            $this->__elephc_collecting = false;
        }
        return $status;
    }

    // Hands the accumulated struct arrays to xml_parse_into_struct()'s output parameters.
    public function __elephc_struct_values(): mixed {
        // Entries are appended by explicit index, which builds hash storage; PHP's
        // $values is a list, and array_values() hands the caller exactly that shape.
        $values = array_values($this->__elephc_struct_values);
        $this->__elephc_struct_values = [];
        return $values;
    }

    public function __elephc_struct_index(): mixed {
        $index = $this->__elephc_struct_index;
        $this->__elephc_struct_index = [];
        return $index;
    }

    // Applies XML_OPTION_SKIP_TAGSTART. The unstripped case copies the name instead of
    // returning the parameter itself: a string parameter returned as-is is borrowed from
    // the caller, and the callers store the result as an owned array key.
    private function __elephc_strip(string $name): string {
        $offset = $this->__elephc_skip_tagstart;
        if ($offset === 0) {
            return substr($name, 0);
        }
        if ($offset >= strlen($name)) {
            return "";
        }
        return substr($name, $offset);
    }

    private function __elephc_add_to_info(string $tag): void {
        if ($this->__elephc_collect_index) {
            // `??` rather than an isset() ternary: the ternary's array read leaks its copy
            // in the runtime, and php-src only fills the index when the caller asked for it.
            $positions = $this->__elephc_struct_index[$tag] ?? [];
            $positions[] = $this->__elephc_curtag;
            $this->__elephc_struct_index[$tag] = $positions;
        }
        $this->__elephc_curtag++;
    }

    // Appends one struct entry; entries are written by explicit index so the array
    // property keeps one storage shape for pushes and in-place updates alike.
    private function __elephc_append_entry(array $entry): int {
        $next = $this->__elephc_struct_next;
        $this->__elephc_struct_values[$next] = $entry;
        $this->__elephc_struct_next = $next + 1;
        return $next;
    }

    private function __elephc_drain(): void {
        $raw = $this->__elephc_handle;
        while (true) {
            $kind = elephc_xml_parser_next($raw);
            if ($kind <= 0) {
                return;
            }
            if ($kind === 1) {
                $this->__elephc_on_start($raw);
            } elseif ($kind === 2) {
                $this->__elephc_on_end($raw);
            } elseif ($kind === 3) {
                $this->__elephc_on_characters(elephc_xml_parser_event_string($raw, 0));
            } elseif ($kind === 4) {
                $this->__elephc_on_pi($raw);
            } elseif ($kind === 5) {
                if ($this->__elephc_default_installed) {
                    $this->__elephc_call_default("<!--" . elephc_xml_parser_event_string($raw, 0) . "-->");
                }
            } elseif ($kind === 6) {
                if (!$this->__elephc_on_entity_ref($raw)) {
                    return;
                }
            } elseif ($kind === 7) {
                $this->__elephc_on_notation_decl($raw);
            } elseif ($kind === 8) {
                $this->__elephc_on_unparsed_entity_decl($raw);
            }
        }
    }

    private function __elephc_call_default(string $data): void {
        $handler = $this->__elephc_default_handler;
        if ($handler !== null) {
            call_user_func($handler, $this, $data);
        }
    }

    private function __elephc_on_start(int $raw): void {
        $ns_count = elephc_xml_parser_event_int($raw, 1);
        if ($ns_count > 0 && $this->__elephc_start_ns_installed) {
            $handler = $this->__elephc_start_ns_handler;
            for ($i = 0; $i < $ns_count; $i++) {
                if ($handler !== null) {
                    // PHP hands `false` for the default namespace's missing prefix; the two
                    // call shapes stay separate so no local carries a string|false union.
                    if (elephc_xml_parser_event_ns_has_prefix($raw, $i) === 1) {
                        call_user_func($handler, $this, elephc_xml_parser_event_ns_prefix($raw, $i), elephc_xml_parser_event_ns_uri($raw, $i));
                    } else {
                        call_user_func($handler, $this, false, elephc_xml_parser_event_ns_uri($raw, $i));
                    }
                }
            }
        }
        if (!$this->__elephc_element_installed) {
            if ($this->__elephc_default_installed) {
                $this->__elephc_call_default(elephc_xml_parser_event_string($raw, 1));
            }
            return;
        }
        $this->__elephc_level++;
        $name = elephc_xml_parser_event_string($raw, 0);
        $stripped = $this->__elephc_strip($name);
        $attributes = [];
        $count = elephc_xml_parser_event_int($raw, 0);
        for ($i = 0; $i < $count; $i++) {
            $attributes[elephc_xml_parser_event_attr_name($raw, $i)] = elephc_xml_parser_event_attr_value($raw, $i);
        }
        $handler = $this->__elephc_start_handler;
        if ($handler !== null) {
            // Handed over through a fresh copy: a local grown element by element from `[]`
            // is boxed with the wrong array tag when it becomes a dynamic callable
            // argument, and a `mixed` handler parameter then sees an empty array.
            $attribute_map = $attributes;
            call_user_func($handler, $this, $stripped, $attribute_map);
        }
        if ($this->__elephc_collecting) {
            $level = $this->__elephc_level;
            if ($level <= 255) {
                $this->__elephc_add_to_info($stripped);
                $tag = ["tag" => $stripped, "type" => "open", "level" => $level];
                $this->__elephc_ltags[$level - 1] = $name;
                $this->__elephc_lastwasopen = true;
                if ($count > 0) {
                    $tag["attributes"] = $attributes;
                }
                $this->__elephc_ctag_index = $this->__elephc_append_entry($tag);
            }
        }
    }

    private function __elephc_on_end(int $raw): void {
        if (!$this->__elephc_element_installed) {
            if ($this->__elephc_default_installed) {
                $this->__elephc_call_default(elephc_xml_parser_event_string($raw, 1));
            }
            return;
        }
        $name = elephc_xml_parser_event_string($raw, 0);
        $stripped = $this->__elephc_strip($name);
        $handler = $this->__elephc_end_handler;
        if ($handler !== null) {
            call_user_func($handler, $this, $stripped);
        }
        if ($this->__elephc_collecting) {
            if ($this->__elephc_lastwasopen) {
                // Nested element writes on an array property must be explicit
                // read-modify-write sequences in elephc-PHP.
                $index = $this->__elephc_ctag_index;
                $entry = $this->__elephc_struct_values[$index];
                $entry["type"] = "complete";
                $this->__elephc_struct_values[$index] = $entry;
            } else {
                $this->__elephc_add_to_info($stripped);
                $this->__elephc_append_entry(["tag" => $stripped, "type" => "close", "level" => $this->__elephc_level]);
            }
            $this->__elephc_lastwasopen = false;
        }
        $this->__elephc_level--;
    }

    private function __elephc_on_characters(string $data): void {
        if (!$this->__elephc_cdata_installed) {
            if ($this->__elephc_default_installed) {
                $this->__elephc_call_default($data);
            }
            return;
        }
        $handler = $this->__elephc_cdata_handler;
        if ($handler !== null) {
            call_user_func($handler, $this, $data);
        }
        if (!$this->__elephc_collecting) {
            return;
        }
        $doprint = false;
        if ($this->__elephc_skip_white) {
            $length = strlen($data);
            for ($i = 0; $i < $length; $i++) {
                $byte = $data[$i];
                if ($byte !== " " && $byte !== "\t" && $byte !== "\n") {
                    $doprint = true;
                    break;
                }
            }
        }
        $keep = $doprint || !$this->__elephc_skip_white;
        if ($this->__elephc_lastwasopen) {
            $index = $this->__elephc_ctag_index;
            $entry = $this->__elephc_struct_values[$index];
            if (isset($entry["value"])) {
                $entry["value"] = $entry["value"] . $data;
                $this->__elephc_struct_values[$index] = $entry;
            } elseif ($keep) {
                $entry["value"] = $data;
                $this->__elephc_struct_values[$index] = $entry;
            }
            return;
        }
        $last = $this->__elephc_struct_next - 1;
        if ($last >= 0) {
            $previous = $this->__elephc_struct_values[$last];
            if ($previous["type"] === "cdata") {
                $previous["value"] = $previous["value"] . $data;
                $this->__elephc_struct_values[$last] = $previous;
                return;
            }
        }
        $level = $this->__elephc_level;
        if ($level <= 255 && $level > 0 && $keep) {
            $stripped = $this->__elephc_strip($this->__elephc_ltags[$level - 1]);
            $this->__elephc_add_to_info($stripped);
            $this->__elephc_append_entry(["tag" => $stripped, "value" => $data, "type" => "cdata", "level" => $level]);
        }
    }

    private function __elephc_on_pi(int $raw): void {
        $target = elephc_xml_parser_event_string($raw, 0);
        $has_data = (elephc_xml_parser_event_int($raw, 3) & 1) === 1;
        if (!$this->__elephc_pi_installed) {
            if ($this->__elephc_default_installed) {
                $this->__elephc_call_default("<?" . $target . " " . ($has_data ? elephc_xml_parser_event_string($raw, 1) : "(null)") . "?>");
            }
            return;
        }
        $handler = $this->__elephc_pi_handler;
        if ($handler !== null) {
            if ($has_data) {
                call_user_func($handler, $this, $target, elephc_xml_parser_event_string($raw, 1));
            } else {
                call_user_func($handler, $this, $target, false);
            }
        }
    }

    // php-src compat.c get_entity(): predefined entities expand unless only a default
    // handler is installed, internal entities are handed to the default handler unexpanded
    // or their replacement text to the character-data handler, external parsed entities
    // consult the external-entity-ref handler (a false answer stops the parse with
    // XML_ERROR_EXTERNAL_ENTITY_HANDLING), and anything else only reaches the default
    // handler before the parser reports its own error. Returns false when the parse stops.
    private function __elephc_on_entity_ref(int $raw): bool {
        $kind = elephc_xml_parser_event_int($raw, 2);
        $name = elephc_xml_parser_event_string($raw, 0);
        if ($kind === 2) {
            if (!$this->__elephc_extref_installed) {
                return true;
            }
            $continue = 0;
            $handler = $this->__elephc_extref_handler;
            if ($handler !== null) {
                $flags = elephc_xml_parser_event_int($raw, 3);
                $system_id = elephc_xml_parser_event_string($raw, 1);
                // A handler that throws counts as "return 0" for php-src, which stops the
                // parser with error 21 before the exception reaches the caller.
                $returned = false;
                try {
                    if (($flags & 2) === 2) {
                        $result = call_user_func($handler, $this, $name, "", $system_id, elephc_xml_parser_event_string($raw, 2));
                    } else {
                        $result = call_user_func($handler, $this, $name, "", $system_id, false);
                    }
                    $returned = true;
                    $continue = (int) $result;
                } finally {
                    if (!$returned) {
                        elephc_xml_parser_stop($raw, 21);
                    }
                }
            }
            if ($continue === 0) {
                elephc_xml_parser_stop($raw, 21);
                return false;
            }
            return true;
        }
        $predefined = $kind === 0;
        $expandable = $kind === 0 || $kind === 1;
        if ($this->__elephc_default_installed && !($predefined && $this->__elephc_cdata_installed)) {
            $this->__elephc_call_default("&" . $name . ";");
        } elseif ($this->__elephc_cdata_installed && $expandable) {
            $this->__elephc_on_characters(elephc_xml_parser_event_string($raw, 1));
        }
        return true;
    }

    private function __elephc_on_notation_decl(int $raw): void {
        if (!$this->__elephc_notation_installed) {
            return;
        }
        $handler = $this->__elephc_notation_handler;
        if ($handler === null) {
            return;
        }
        $flags = elephc_xml_parser_event_int($raw, 3);
        $name = elephc_xml_parser_event_string($raw, 0);
        $has_system = ($flags & 4) === 4;
        $has_public = ($flags & 2) === 2;
        if ($has_system && $has_public) {
            call_user_func($handler, $this, $name, false, elephc_xml_parser_event_string($raw, 1), elephc_xml_parser_event_string($raw, 2));
        } elseif ($has_system) {
            call_user_func($handler, $this, $name, false, elephc_xml_parser_event_string($raw, 1), false);
        } elseif ($has_public) {
            call_user_func($handler, $this, $name, false, false, elephc_xml_parser_event_string($raw, 2));
        } else {
            call_user_func($handler, $this, $name, false, false, false);
        }
    }

    private function __elephc_on_unparsed_entity_decl(int $raw): void {
        if (!$this->__elephc_unparsed_installed) {
            return;
        }
        $handler = $this->__elephc_unparsed_handler;
        if ($handler === null) {
            return;
        }
        $flags = elephc_xml_parser_event_int($raw, 3);
        $name = elephc_xml_parser_event_string($raw, 0);
        $system_id = elephc_xml_parser_event_string($raw, 1);
        $notation = elephc_xml_parser_event_string($raw, 3);
        if (($flags & 2) === 2) {
            call_user_func($handler, $this, $name, false, $system_id, elephc_xml_parser_event_string($raw, 2), $notation);
        } else {
            call_user_func($handler, $this, $name, false, $system_id, false, $notation);
        }
    }
}

// -- ext/xml: procedural surface --

function xml_parser_create(?string $encoding = null): XMLParser {
    return XMLParser::__elephc_create("xml_parser_create", $encoding, false, ":");
}

function xml_parser_create_ns(?string $encoding = null, string $separator = ":"): XMLParser {
    return XMLParser::__elephc_create("xml_parser_create_ns", $encoding, true, $separator);
}

function xml_set_object(XMLParser $parser, mixed $object): bool {
    return $parser->__elephc_set_object($object);
}

// The nine handler setters are registry builtins (`crate::builtins::xml`), so the checker
// can type an unannotated handler closure's parameters from the event it will receive;
// their lowering calls these `__elephc_`-prefixed twins.

function __elephc_xml_set_element_handler(XMLParser $parser, mixed $start_handler, mixed $end_handler): bool {
    return $parser->__elephc_set_element_handler($start_handler, $end_handler);
}

function __elephc_xml_set_character_data_handler(XMLParser $parser, mixed $handler): bool {
    return $parser->__elephc_set_character_data_handler($handler);
}

function __elephc_xml_set_processing_instruction_handler(XMLParser $parser, mixed $handler): bool {
    return $parser->__elephc_set_processing_instruction_handler($handler);
}

function __elephc_xml_set_default_handler(XMLParser $parser, mixed $handler): bool {
    return $parser->__elephc_set_default_handler($handler);
}

function __elephc_xml_set_unparsed_entity_decl_handler(XMLParser $parser, mixed $handler): bool {
    return $parser->__elephc_set_unparsed_entity_decl_handler($handler);
}

function __elephc_xml_set_notation_decl_handler(XMLParser $parser, mixed $handler): bool {
    return $parser->__elephc_set_notation_decl_handler($handler);
}

function __elephc_xml_set_external_entity_ref_handler(XMLParser $parser, mixed $handler): bool {
    return $parser->__elephc_set_external_entity_ref_handler($handler);
}

function __elephc_xml_set_start_namespace_decl_handler(XMLParser $parser, mixed $handler): bool {
    return $parser->__elephc_set_start_namespace_decl_handler($handler);
}

function __elephc_xml_set_end_namespace_decl_handler(XMLParser $parser, mixed $handler): bool {
    return $parser->__elephc_set_end_namespace_decl_handler($handler);
}

function xml_parse(XMLParser $parser, string $data, bool $is_final = false): int {
    return $parser->__elephc_parse($data, $is_final);
}

// The xml_parse_into_struct() registry builtin composes these three: run the parse with the
// struct side installed, then hand each accumulated array to its output variable.
function __elephc_xml_parse_into_struct(XMLParser $parser, string $data, bool $with_index): int {
    return $parser->__elephc_parse_into_struct($data, $with_index);
}

function __elephc_xml_struct_values(XMLParser $parser): mixed {
    return $parser->__elephc_struct_values();
}

function __elephc_xml_struct_index(XMLParser $parser): mixed {
    return $parser->__elephc_struct_index();
}

function xml_get_error_code(XMLParser $parser): int {
    return $parser->__elephc_error_code();
}

function xml_error_string(int $error_code): ?string {
    return elephc_xml_error_string($error_code);
}

function xml_get_current_line_number(XMLParser $parser): int {
    return $parser->__elephc_line();
}

function xml_get_current_column_number(XMLParser $parser): int {
    return $parser->__elephc_column();
}

function xml_get_current_byte_index(XMLParser $parser): int {
    return $parser->__elephc_byte_index();
}

function xml_parser_free(XMLParser $parser): bool {
    // php-src: "Parser cannot be freed while it is parsing" (E_WARNING) and false; elephc
    // has no warning channel from a prelude, so only the return value is reproduced.
    if ($parser->__elephc_parsing) {
        return false;
    }
    return true;
}

function xml_parser_set_option(XMLParser $parser, int $option, mixed $value): bool {
    return $parser->__elephc_set_option($option, $value);
}

function xml_parser_get_option(XMLParser $parser, int $option): mixed {
    return $parser->__elephc_get_option($option);
}

// -- ext/xmlwriter: the XMLWriter object --

// The bridge writer always buffers in memory; openUri()/toStream() writers hand that buffer
// to their PHP stream on flush(), endDocument() and destruction, mirroring libxml2's
// output buffer over php-src's stream writer. Memory writers return strings from flush().
class XMLWriter {
    public int $__elephc_handle = 0;
    public mixed $__elephc_stream = null;
    // 1 / 2 when the writer targets php://output (php://stdout) / php://stderr.
    public int $__elephc_std_fd = 0;
    public bool $__elephc_uri_mode = false;

    public function __destruct() {
        $raw = $this->__elephc_handle;
        if ($raw !== 0) {
            if ($this->__elephc_uri_mode) {
                $this->__elephc_flush_to_stream($raw);
            }
            $this->__elephc_handle = 0;
            elephc_xml_writer_free($raw);
        }
    }

    // PHP marks XMLWriter uncloneable (user subclasses inherit that; the message names the
    // runtime class). elephc clones shallowly first and only then runs this hook on the
    // copy, so at this point $this is the copy and already holds the original's bridge
    // handle and output stream. Detaching both BEFORE throwing means the copy's destructor
    // (which skips handle 0) neither flushes nor frees the writer out from under the
    // original. `final` keeps a user subclass from overriding the guard: PHP throws for
    // subclasses too, before any hook runs, and an override would re-enable a copy that
    // shares the original's handle.
    final public function __clone(): void {
        $this->__elephc_handle = 0;
        $this->__elephc_uri_mode = false;
        $this->__elephc_std_fd = 0;
        $this->__elephc_stream = null;
        throw new Error("Trying to clone an uncloneable object of class " . get_class($this));
    }

    // php-src's XMLWriter carries no properties, so serialize() writes an empty object
    // and unserialize() hands back a fresh, unopened writer; the bridge handle and the
    // stream state must never travel (a restored copy would own the original's handle).
    public function __serialize(): array {
        return [];
    }

    public function __unserialize(array $data): void {
        $this->__elephc_handle = 0;
        $this->__elephc_stream = null;
        $this->__elephc_std_fd = 0;
        $this->__elephc_uri_mode = false;
    }

    public function __debugInfo(): array {
        return [];
    }

    private function __elephc_reset(): void {
        $raw = $this->__elephc_handle;
        if ($raw !== 0) {
            if ($this->__elephc_uri_mode) {
                $this->__elephc_flush_to_stream($raw);
            }
            elephc_xml_writer_free($raw);
        }
        $this->__elephc_handle = elephc_xml_writer_create();
        $this->__elephc_stream = null;
        $this->__elephc_std_fd = 0;
        $this->__elephc_uri_mode = false;
    }

    // The buffered output, peeked or taken. A C string cannot carry the NUL bytes of a
    // UTF-16 / UCS-4 document, so such output crosses the bridge hex-encoded.
    private function __elephc_output(int $raw, bool $take): string {
        if (elephc_xml_writer_output_has_nul($raw) === 1) {
            $decoded = hex2bin(elephc_xml_writer_output_hex($raw, $take ? 1 : 0));
            if (is_string($decoded)) {
                return $decoded;
            }
            return "";
        }
        if ($take) {
            return elephc_xml_writer_take_output($raw);
        }
        return elephc_xml_writer_output($raw);
    }

    private function __elephc_flush_to_stream(int $raw): int {
        $pending = $this->__elephc_output($raw, true);
        if ($pending === "") {
            return 0;
        }
        if ($this->__elephc_std_fd === 1) {
            $written = fwrite(STDOUT, $pending);
            return $written === false ? 0 : $written;
        }
        if ($this->__elephc_std_fd === 2) {
            $written = fwrite(STDERR, $pending);
            return $written === false ? 0 : $written;
        }
        $stream = $this->__elephc_stream;
        if ($stream === null) {
            return 0;
        }
        $written = fwrite($stream, $pending);
        return $written === false ? 0 : $written;
    }

    // Throws PHP's error for a writer that was never opened.
    private function __elephc_require(): int {
        $raw = $this->__elephc_handle;
        if ($raw === 0) {
            throw new Error("Invalid or uninitialized XMLWriter object");
        }
        return $raw;
    }

    private static function __elephc_check_name(string $name, string $function, string $argument, string $subject): void {
        if (elephc_xml_writer_valid_name($name) === 0) {
            throw new ValueError($function . "(): Argument " . $argument . " must be a valid " . $subject . ", \"" . $name . "\" given");
        }
    }

    public function __elephc_open_uri(string $uri, string $function): bool {
        if ($uri === "") {
            throw new ValueError($function . "(): Argument #1 (\$uri) must not be empty");
        }
        // The process's own output streams are written through STDOUT / STDERR rather
        // than an fopen() handle: releasing such a handle would close the descriptor for
        // the rest of the program, which PHP's php://output never does.
        if ($uri === "php://output" || $uri === "php://stdout") {
            $this->__elephc_reset();
            $this->__elephc_std_fd = 1;
            $this->__elephc_uri_mode = true;
            return true;
        }
        if ($uri === "php://stderr") {
            $this->__elephc_reset();
            $this->__elephc_std_fd = 2;
            $this->__elephc_uri_mode = true;
            return true;
        }
        // php-src opens the URI through the stream layer in "wb" mode; an unopenable
        // target answers false (the stream layer reports why).
        $stream = fopen($uri, "wb");
        if ($stream === false) {
            return false;
        }
        $this->__elephc_reset();
        $this->__elephc_stream = $stream;
        $this->__elephc_uri_mode = true;
        return true;
    }

    public function openUri(string $uri): bool {
        return $this->__elephc_open_uri($uri, "XMLWriter::openUri");
    }

    public static function toUri(string $uri): static {
        $writer = new static();
        if (!$writer->__elephc_open_uri($uri, "XMLWriter::toUri")) {
            throw new ValueError("XMLWriter::toUri(): Argument #1 (\$uri) must resolve to a valid file path");
        }
        return $writer;
    }

    public function openMemory(): bool {
        $this->__elephc_reset();
        return true;
    }

    public static function toMemory(): static {
        $writer = new static();
        $writer->openMemory();
        return $writer;
    }

    public static function toStream(mixed $stream): static {
        if (!is_resource($stream)) {
            throw new TypeError("XMLWriter::toStream(): Argument #1 (\$stream) must be of type resource, " . gettype($stream) . " given");
        }
        $writer = new static();
        $writer->__elephc_reset();
        $writer->__elephc_stream = $stream;
        $writer->__elephc_uri_mode = true;
        return $writer;
    }

    public function setIndent(bool $enable): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_set_indent($raw, $enable ? 1 : 0) === 1;
    }

    public function setIndentString(string $indentation): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_set_indent_string($raw, $indentation) === 1;
    }

    public function startComment(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_start_comment($raw) === 1;
    }

    public function endComment(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_comment($raw) === 1;
    }

    public function __elephc_start_attribute(string $name, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "attribute name");
        return elephc_xml_writer_start_attribute($raw, $name) === 1;
    }

    public function startAttribute(string $name): bool {
        return $this->__elephc_start_attribute($name, "XMLWriter::startAttribute", "#2");
    }

    public function endAttribute(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_attribute($raw) === 1;
    }

    public function __elephc_write_attribute(string $name, string $value, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "attribute name");
        return elephc_xml_writer_write_attribute($raw, $name, $value) === 1;
    }

    public function writeAttribute(string $name, string $value): bool {
        return $this->__elephc_write_attribute($name, $value, "XMLWriter::writeAttribute", "#2 (\$value)");
    }

    public function __elephc_start_attribute_ns(?string $prefix, string $name, ?string $namespace, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "attribute name");
        return elephc_xml_writer_start_attribute_ns($raw, $prefix === null ? 0 : 1, $prefix ?? "", $name, $namespace === null ? 0 : 1, $namespace ?? "") === 1;
    }

    public function startAttributeNs(?string $prefix, string $name, ?string $namespace): bool {
        return $this->__elephc_start_attribute_ns($prefix, $name, $namespace, "XMLWriter::startAttributeNs", "#3 (\$namespace)");
    }

    public function __elephc_write_attribute_ns(?string $prefix, string $name, ?string $namespace, string $value, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "attribute name");
        return elephc_xml_writer_write_attribute_ns($raw, $prefix === null ? 0 : 1, $prefix ?? "", $name, $namespace === null ? 0 : 1, $namespace ?? "", $value) === 1;
    }

    public function writeAttributeNs(?string $prefix, string $name, ?string $namespace, string $value): bool {
        return $this->__elephc_write_attribute_ns($prefix, $name, $namespace, $value, "XMLWriter::writeAttributeNs", "#3 (\$namespace)");
    }

    public function __elephc_start_element(string $name, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "element name");
        return elephc_xml_writer_start_element($raw, $name) === 1;
    }

    public function startElement(string $name): bool {
        return $this->__elephc_start_element($name, "XMLWriter::startElement", "#2");
    }

    public function endElement(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_element($raw) === 1;
    }

    public function fullEndElement(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_full_end_element($raw) === 1;
    }

    public function __elephc_start_element_ns(?string $prefix, string $name, ?string $namespace, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "element name");
        return elephc_xml_writer_start_element_ns($raw, $prefix === null ? 0 : 1, $prefix ?? "", $name, $namespace === null ? 0 : 1, $namespace ?? "") === 1;
    }

    public function startElementNs(?string $prefix, string $name, ?string $namespace): bool {
        return $this->__elephc_start_element_ns($prefix, $name, $namespace, "XMLWriter::startElementNs", "#3 (\$namespace)");
    }

    public function __elephc_write_element(string $name, ?string $content, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "element name");
        return elephc_xml_writer_write_element($raw, $name, $content === null ? 0 : 1, $content ?? "") === 1;
    }

    public function writeElement(string $name, ?string $content = null): bool {
        return $this->__elephc_write_element($name, $content, "XMLWriter::writeElement", "#2 (\$content)");
    }

    public function __elephc_write_element_ns(?string $prefix, string $name, ?string $namespace, ?string $content, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "element name");
        return elephc_xml_writer_write_element_ns($raw, $prefix === null ? 0 : 1, $prefix ?? "", $name, $namespace === null ? 0 : 1, $namespace ?? "", $content === null ? 0 : 1, $content ?? "") === 1;
    }

    public function writeElementNs(?string $prefix, string $name, ?string $namespace, ?string $content = null): bool {
        return $this->__elephc_write_element_ns($prefix, $name, $namespace, $content, "XMLWriter::writeElementNs", "#3 (\$namespace)");
    }

    public function __elephc_start_pi(string $target, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($target, $function, $argument, "PI target");
        return elephc_xml_writer_start_pi($raw, $target) === 1;
    }

    public function startPi(string $target): bool {
        return $this->__elephc_start_pi($target, "XMLWriter::startPi", "#2");
    }

    public function endPi(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_pi($raw) === 1;
    }

    public function __elephc_write_pi(string $target, string $content, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($target, $function, $argument, "PI target");
        return elephc_xml_writer_write_pi($raw, $target, $content) === 1;
    }

    public function writePi(string $target, string $content): bool {
        return $this->__elephc_write_pi($target, $content, "XMLWriter::writePi", "#2 (\$content)");
    }

    public function startCdata(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_start_cdata($raw) === 1;
    }

    public function endCdata(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_cdata($raw) === 1;
    }

    public function writeCdata(string $content): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_write_cdata($raw, $content) === 1;
    }

    public function text(string $content): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_text($raw, $content) === 1;
    }

    public function writeRaw(string $content): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_write_raw($raw, $content) === 1;
    }

    public function startDocument(?string $version = "1.0", ?string $encoding = null, ?string $standalone = null): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_start_document($raw, $version === null ? 0 : 1, $version ?? "", $encoding === null ? 0 : 1, $encoding ?? "", $standalone === null ? 0 : 1, $standalone ?? "") === 1;
    }

    public function endDocument(): bool {
        $raw = $this->__elephc_require();
        $result = elephc_xml_writer_end_document($raw) === 1;
        if ($this->__elephc_uri_mode) {
            $this->__elephc_flush_to_stream($raw);
        }
        return $result;
    }

    public function writeComment(string $content): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_write_comment($raw, $content) === 1;
    }

    public function startDtd(string $qualifiedName, ?string $publicId = null, ?string $systemId = null): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_start_dtd($raw, $qualifiedName, $publicId === null ? 0 : 1, $publicId ?? "", $systemId === null ? 0 : 1, $systemId ?? "") === 1;
    }

    public function endDtd(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_dtd($raw) === 1;
    }

    public function writeDtd(string $name, ?string $publicId = null, ?string $systemId = null, ?string $content = null): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_write_dtd($raw, $name, $publicId === null ? 0 : 1, $publicId ?? "", $systemId === null ? 0 : 1, $systemId ?? "", $content === null ? 0 : 1, $content ?? "") === 1;
    }

    public function __elephc_start_dtd_element(string $qualifiedName, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($qualifiedName, $function, $argument, "element name");
        return elephc_xml_writer_start_dtd_element($raw, $qualifiedName) === 1;
    }

    public function startDtdElement(string $qualifiedName): bool {
        return $this->__elephc_start_dtd_element($qualifiedName, "XMLWriter::startDtdElement", "#2");
    }

    public function endDtdElement(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_dtd_element($raw) === 1;
    }

    public function __elephc_write_dtd_element(string $name, string $content, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "element name");
        return elephc_xml_writer_write_dtd_element($raw, $name, $content) === 1;
    }

    public function writeDtdElement(string $name, string $content): bool {
        return $this->__elephc_write_dtd_element($name, $content, "XMLWriter::writeDtdElement", "#2 (\$content)");
    }

    public function __elephc_start_dtd_attlist(string $name, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "element name");
        return elephc_xml_writer_start_dtd_attlist($raw, $name) === 1;
    }

    public function startDtdAttlist(string $name): bool {
        return $this->__elephc_start_dtd_attlist($name, "XMLWriter::startDtdAttlist", "#2");
    }

    public function endDtdAttlist(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_dtd_attlist($raw) === 1;
    }

    public function __elephc_write_dtd_attlist(string $name, string $content, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "element name");
        return elephc_xml_writer_write_dtd_attlist($raw, $name, $content) === 1;
    }

    public function writeDtdAttlist(string $name, string $content): bool {
        return $this->__elephc_write_dtd_attlist($name, $content, "XMLWriter::writeDtdAttlist", "#2 (\$content)");
    }

    public function __elephc_start_dtd_entity(string $name, bool $isParam, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "attribute name");
        return elephc_xml_writer_start_dtd_entity($raw, $name, $isParam ? 1 : 0) === 1;
    }

    public function startDtdEntity(string $name, bool $isParam): bool {
        return $this->__elephc_start_dtd_entity($name, $isParam, "XMLWriter::startDtdEntity", "#2 (\$isParam)");
    }

    public function endDtdEntity(): bool {
        $raw = $this->__elephc_require();
        return elephc_xml_writer_end_dtd_entity($raw) === 1;
    }

    public function __elephc_write_dtd_entity(string $name, string $content, bool $isParam, ?string $publicId, ?string $systemId, ?string $notationData, string $function, string $argument): bool {
        $raw = $this->__elephc_require();
        self::__elephc_check_name($name, $function, $argument, "element name");
        $flags = ($publicId === null ? 0 : 1) | ($systemId === null ? 0 : 2) | ($notationData === null ? 0 : 4);
        return elephc_xml_writer_write_dtd_entity($raw, $name, $content, $isParam ? 1 : 0, $flags, $publicId ?? "", $systemId ?? "", $notationData ?? "") === 1;
    }

    public function writeDtdEntity(string $name, string $content, bool $isParam = false, ?string $publicId = null, ?string $systemId = null, ?string $notationData = null): bool {
        return $this->__elephc_write_dtd_entity($name, $content, $isParam, $publicId, $systemId, $notationData, "XMLWriter::writeDtdEntity", "#2 (\$content)");
    }

    // A URI writer has no memory buffer to expose: php-src answers "" without flushing.
    public function outputMemory(bool $flush = true): string {
        $raw = $this->__elephc_require();
        if ($this->__elephc_uri_mode) {
            return "";
        }
        return $this->__elephc_output($raw, $flush);
    }

    // Memory writers answer their buffer (emptied when $empty); URI writers write the
    // buffer to their stream and answer the byte count.
    public function flush(bool $empty = true): string|int {
        $raw = $this->__elephc_require();
        if ($this->__elephc_uri_mode) {
            return $this->__elephc_flush_to_stream($raw);
        }
        return $this->__elephc_output($raw, $empty);
    }
}

// -- ext/xmlwriter: procedural surface --

// Declared as plain `XMLWriter` (php-src: `XMLWriter|false`): a `T|false` union is not
// narrowed by a `=== false` guard in elephc's checker, so an unopenable URI raises the
// `ValueError` the static `XMLWriter::toUri()` constructor raises in PHP instead of
// answering false. Documented in docs/php/xml.md.
function xmlwriter_open_uri(string $uri): XMLWriter {
    $writer = new XMLWriter();
    if (!$writer->__elephc_open_uri($uri, "xmlwriter_open_uri")) {
        throw new ValueError("xmlwriter_open_uri(): Argument #1 (\$uri) must resolve to a valid file path");
    }
    return $writer;
}

function xmlwriter_open_memory(): XMLWriter {
    $writer = new XMLWriter();
    $writer->openMemory();
    return $writer;
}

function xmlwriter_set_indent(XMLWriter $writer, bool $enable): bool {
    return $writer->setIndent($enable);
}

function xmlwriter_set_indent_string(XMLWriter $writer, string $indentation): bool {
    return $writer->setIndentString($indentation);
}

function xmlwriter_start_comment(XMLWriter $writer): bool {
    return $writer->startComment();
}

function xmlwriter_end_comment(XMLWriter $writer): bool {
    return $writer->endComment();
}

function xmlwriter_start_attribute(XMLWriter $writer, string $name): bool {
    return $writer->__elephc_start_attribute($name, "xmlwriter_start_attribute", "#2 (\$name)");
}

function xmlwriter_end_attribute(XMLWriter $writer): bool {
    return $writer->endAttribute();
}

function xmlwriter_write_attribute(XMLWriter $writer, string $name, string $value): bool {
    return $writer->__elephc_write_attribute($name, $value, "xmlwriter_write_attribute", "#2 (\$name)");
}

function xmlwriter_start_attribute_ns(XMLWriter $writer, ?string $prefix, string $name, ?string $namespace): bool {
    return $writer->__elephc_start_attribute_ns($prefix, $name, $namespace, "xmlwriter_start_attribute_ns", "#3 (\$name)");
}

function xmlwriter_write_attribute_ns(XMLWriter $writer, ?string $prefix, string $name, ?string $namespace, string $value): bool {
    return $writer->__elephc_write_attribute_ns($prefix, $name, $namespace, $value, "xmlwriter_write_attribute_ns", "#3 (\$name)");
}

function xmlwriter_start_element(XMLWriter $writer, string $name): bool {
    return $writer->__elephc_start_element($name, "xmlwriter_start_element", "#2 (\$name)");
}

function xmlwriter_end_element(XMLWriter $writer): bool {
    return $writer->endElement();
}

function xmlwriter_full_end_element(XMLWriter $writer): bool {
    return $writer->fullEndElement();
}

function xmlwriter_start_element_ns(XMLWriter $writer, ?string $prefix, string $name, ?string $namespace): bool {
    return $writer->__elephc_start_element_ns($prefix, $name, $namespace, "xmlwriter_start_element_ns", "#3 (\$name)");
}

function xmlwriter_write_element(XMLWriter $writer, string $name, ?string $content = null): bool {
    return $writer->__elephc_write_element($name, $content, "xmlwriter_write_element", "#2 (\$name)");
}

function xmlwriter_write_element_ns(XMLWriter $writer, ?string $prefix, string $name, ?string $namespace, ?string $content = null): bool {
    return $writer->__elephc_write_element_ns($prefix, $name, $namespace, $content, "xmlwriter_write_element_ns", "#3 (\$name)");
}

function xmlwriter_start_pi(XMLWriter $writer, string $target): bool {
    return $writer->__elephc_start_pi($target, "xmlwriter_start_pi", "#2 (\$target)");
}

function xmlwriter_end_pi(XMLWriter $writer): bool {
    return $writer->endPi();
}

function xmlwriter_write_pi(XMLWriter $writer, string $target, string $content): bool {
    return $writer->__elephc_write_pi($target, $content, "xmlwriter_write_pi", "#2 (\$target)");
}

function xmlwriter_start_cdata(XMLWriter $writer): bool {
    return $writer->startCdata();
}

function xmlwriter_end_cdata(XMLWriter $writer): bool {
    return $writer->endCdata();
}

function xmlwriter_write_cdata(XMLWriter $writer, string $content): bool {
    return $writer->writeCdata($content);
}

function xmlwriter_text(XMLWriter $writer, string $content): bool {
    return $writer->text($content);
}

function xmlwriter_write_raw(XMLWriter $writer, string $content): bool {
    return $writer->writeRaw($content);
}

function xmlwriter_start_document(XMLWriter $writer, ?string $version = "1.0", ?string $encoding = null, ?string $standalone = null): bool {
    return $writer->startDocument($version, $encoding, $standalone);
}

function xmlwriter_end_document(XMLWriter $writer): bool {
    return $writer->endDocument();
}

function xmlwriter_write_comment(XMLWriter $writer, string $content): bool {
    return $writer->writeComment($content);
}

function xmlwriter_start_dtd(XMLWriter $writer, string $qualifiedName, ?string $publicId = null, ?string $systemId = null): bool {
    return $writer->startDtd($qualifiedName, $publicId, $systemId);
}

function xmlwriter_end_dtd(XMLWriter $writer): bool {
    return $writer->endDtd();
}

function xmlwriter_write_dtd(XMLWriter $writer, string $name, ?string $publicId = null, ?string $systemId = null, ?string $content = null): bool {
    return $writer->writeDtd($name, $publicId, $systemId, $content);
}

function xmlwriter_start_dtd_element(XMLWriter $writer, string $qualifiedName): bool {
    return $writer->__elephc_start_dtd_element($qualifiedName, "xmlwriter_start_dtd_element", "#2 (\$qualifiedName)");
}

function xmlwriter_end_dtd_element(XMLWriter $writer): bool {
    return $writer->endDtdElement();
}

function xmlwriter_write_dtd_element(XMLWriter $writer, string $name, string $content): bool {
    return $writer->__elephc_write_dtd_element($name, $content, "xmlwriter_write_dtd_element", "#2 (\$name)");
}

function xmlwriter_start_dtd_attlist(XMLWriter $writer, string $name): bool {
    return $writer->__elephc_start_dtd_attlist($name, "xmlwriter_start_dtd_attlist", "#2 (\$name)");
}

function xmlwriter_end_dtd_attlist(XMLWriter $writer): bool {
    return $writer->endDtdAttlist();
}

function xmlwriter_write_dtd_attlist(XMLWriter $writer, string $name, string $content): bool {
    return $writer->__elephc_write_dtd_attlist($name, $content, "xmlwriter_write_dtd_attlist", "#2 (\$name)");
}

function xmlwriter_start_dtd_entity(XMLWriter $writer, string $name, bool $isParam): bool {
    return $writer->__elephc_start_dtd_entity($name, $isParam, "xmlwriter_start_dtd_entity", "#2 (\$name)");
}

function xmlwriter_end_dtd_entity(XMLWriter $writer): bool {
    return $writer->endDtdEntity();
}

function xmlwriter_write_dtd_entity(XMLWriter $writer, string $name, string $content, bool $isParam = false, ?string $publicId = null, ?string $systemId = null, ?string $notationData = null): bool {
    return $writer->__elephc_write_dtd_entity($name, $content, $isParam, $publicId, $systemId, $notationData, "xmlwriter_write_dtd_entity", "#2 (\$name)");
}

function xmlwriter_output_memory(XMLWriter $writer, bool $flush = true): string {
    return $writer->outputMemory($flush);
}

function xmlwriter_flush(XMLWriter $writer, bool $empty = true): string|int {
    return $writer->flush($empty);
}
"##;
