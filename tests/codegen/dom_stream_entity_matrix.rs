//! Purpose:
//! Table-driven DOM URI, PHP stream-context, and external-entity callback regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_stream_entity_matrix` through Rust's test harness.
//!
//! Key details:
//! - Custom wrappers prove DOM enters the PHP stream layer instead of opening resources through libxml directly.
//! - Callback fixtures are compact and re-entrant-safe so they can be run as isolated focused tests.

use crate::support::compile_and_run;

/// Pins custom URI loading, active libxml stream contexts, and external loader callback delivery.
#[test]
fn dom_streams_and_entities_match_php_oracle_matrix() {
    for (case, source, expected) in [
        (
            "DOM-STREAM-CONTEXT-01",
            r#"<?php
class DomContextReadStream {
    public mixed $context;
    public static string $seen = "";
    private string $data = "<root><item>ctx</item></root>";
    private int $offset = 0;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        $options = $this->context ? stream_context_get_options($this->context) : [];
        self::$seen = $options["domctx"]["token"] ?? "none";
        return true;
    }

    public function stream_read($count): string {
        $chunk = substr($this->data, $this->offset, $count);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof(): bool {
        return $this->offset >= strlen($this->data);
    }

    public function url_stat($path, $flags): array {
        return [];
    }
}

stream_wrapper_register("domread", DomContextReadStream::class);
$context = stream_context_create(["domctx" => ["token" => "ctx"]]);
libxml_set_streams_context($context);
$document = new DOMDocument();
$loaded = $document->load("domread://host/path/document.xml");
echo "load|" . ($loaded ? "T" : "F") . "|" . $document->documentURI . "|"
    . $document->documentElement->textContent . "|" . DomContextReadStream::$seen;
"#,
            "load|T|domread://host/path/document.xml|ctx|ctx",
        ),
        (
            "DOM-STREAM-ENTITY-02",
            r#"<?php
class DomExternalLoader {
    public static int $calls = 0;
    public static string $system = "";

    public function __invoke($public, $system, $context): mixed {
        self::$calls++;
        self::$system = (string) $system;
        $nested = new DOMDocument();
        $nested->loadXML("<nested/>");
        return null;
    }
}

libxml_use_internal_errors(true);
libxml_clear_errors();
$loader = new DomExternalLoader();
libxml_set_external_entity_loader($loader);
$document = new DOMDocument();
$loaded = $document->loadXML(
    "<!DOCTYPE root SYSTEM \"memory://missing.dtd\"><root/>",
    LIBXML_DTDLOAD,
);
echo "entity|" . ($loaded ? "T" : "F") . "|" . DomExternalLoader::$calls . "|"
    . DomExternalLoader::$system . "|" . count(libxml_get_errors());
libxml_set_external_entity_loader(null);
"#,
            "entity|T|1|memory://missing.dtd|1",
        ),
    ] {
        assert_eq!(compile_and_run(source), expected, "{case}");
    }
}
