//! Purpose:
//! Table-driven DOM XInclude fallback, mutation-epoch, nested-include, and cycle regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_xinclude_matrix` through Rust's test harness.
//!
//! Key details:
//! - Live collections and stale wrappers are observed after each native tree replacement.
//! - The cycle loader is an isolated PHP stream wrapper, avoiding ambient files and network access.

use crate::support::compile_and_run;

/// Pins nested fallback replacement, collection epochs, invalidated wrappers, and cyclic resource failures.
#[test]
fn dom_xinclude_mutation_and_errors_match_php_oracle_matrix() {
    for (case, source, expected) in [
        (
            "DOM-XINCLUDE-FALLBACK-01",
            r#"<?php
$document = new DOMDocument();
$document->loadXML(
    "<root xmlns:xi=\"http://www.w3.org/2001/XInclude\">"
    . "<xi:include href=\"missing-one.xml\"><xi:fallback><outer>"
    . "<xi:include href=\"missing-two.xml\"><xi:fallback><inner/></xi:fallback>"
    . "</xi:include></outer></xi:fallback></xi:include></root>"
);
$nodes = $document->getElementsByTagName("*");
$stale = $document->documentElement->firstElementChild;
set_error_handler(function () { return true; });
$changed = $document->xinclude();
restore_error_handler();
echo "fallback|" . $changed . "|" . $nodes->length . "|" . $nodes->item(1)->nodeName . "|";
try {
    echo $stale->nodeName;
} catch (DOMException $error) {
    echo $error->getCode();
}
"#,
            "fallback|2|3|outer|11",
        ),
        (
            "DOM-XINCLUDE-CYCLE-02",
            r#"<?php
class CycleXincludeStream {
    public mixed $context;
    private string $data = "";
    private int $offset = 0;

    public function url_stat($path, $flags): array {
        return [];
    }

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        $this->data = "<document xmlns:xi=\"http://www.w3.org/2001/XInclude\">"
            . "<xi:include href=\"cycle://loop\"/></document>";
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
}

stream_wrapper_register("cycle", CycleXincludeStream::class);
libxml_use_internal_errors(true);
libxml_clear_errors();
$document = new DOMDocument();
$document->loadXML(
    "<root xmlns:xi=\"http://www.w3.org/2001/XInclude\">"
    . "<xi:include href=\"cycle://loop\"/></root>"
);
$changed = $document->xinclude();
$error = libxml_get_last_error();
echo "cycle|" . $changed . "|" . $document->documentElement->firstElementChild->nodeName
    . "|" . count(libxml_get_errors()) . "|" . $error->code;
"#,
            "cycle|-1|document|1|1600",
        ),
    ] {
        assert_eq!(compile_and_run(source), expected, "{case}");
    }
}
