<?php
// Differential check of model.php (the PHP source html_parser/{tables,parser}.rs were transcribed from)
// against php -n: php fuzz.php SEED /tmp/cases.ser && php harness.php /tmp/cases.ser
// Re-transcribe after editing model.php: ELEPHC_TRANSCRIBE_WHICH=file ELEPHC_TRANSCRIBE_IN=model.php
// ELEPHC_TRANSCRIBE_SPLIT=html_parser_declarations ELEPHC_TRANSCRIBE_OUT=out.rs cargo test --lib dump_prelude_on_request
require __DIR__ . '/model.php';
function __elephc_dom_new_node(string $kind, string $name, string $value, mixed $attrs, mixed $children): mixed {
    return ["kind" => $kind, "name" => $name, "value" => $value, "attrs" => $attrs, "children" => $children];
}
function dumpTree(array $n, int $d = 0): string {
    $pad = str_repeat(' ', $d);
    $k = $n['kind'];
    switch ($k) {
        case 'document': $o = ''; break;
        case 'element':
            $at = [];
            foreach ($n['attrs'] as $p) { $at[] = $p[0] . '=' . $p[1]; }
            sort($at);
            $o = $pad . 'E ' . $n['name'] . ' [' . implode(',', $at) . "]\n"; break;
        case 'text': $o = $pad . 'T ' . bin2hex($n['value']) . "\n"; break;
        case 'cdata': $o = $pad . 'C ' . bin2hex($n['value']) . "\n"; break;
        case 'comment': $o = $pad . 'M ' . bin2hex($n['value']) . "\n"; break;
        case 'pi': $o = $pad . 'P ' . $n['name'] . ' ' . bin2hex($n['value']) . "\n"; break;
        case 'doctype': $o = $pad . 'D ' . $n['name'] . '|' . $n['value'] . '|' . $n['attrs'][0] . "\n"; break;
    }
    foreach ($n['children'] as $c) { $o .= dumpTree($c, $d + 1); }
    return $o;
}
function dumpDom(DOMNode $n, int $d = 0): string {
    $pad = str_repeat(' ', $d);
    $o = '';
    switch ($n->nodeType) {
        case XML_HTML_DOCUMENT_NODE: break;
        case XML_ELEMENT_NODE:
            $at = [];
            foreach ($n->attributes as $a) { $at[] = $a->name . '=' . $a->value; }
            sort($at);
            $o = $pad . 'E ' . $n->nodeName . ' [' . implode(',', $at) . "]\n"; break;
        case XML_TEXT_NODE: $o = $pad . 'T ' . bin2hex((string) $n->nodeValue) . "\n"; break;
        case XML_CDATA_SECTION_NODE: $o = $pad . 'C ' . bin2hex((string) $n->nodeValue) . "\n"; break;
        case XML_COMMENT_NODE: $o = $pad . 'M ' . bin2hex((string) $n->nodeValue) . "\n"; break;
        case XML_PI_NODE: $o = $pad . 'P ' . $n->nodeName . ' ' . bin2hex((string) $n->nodeValue) . "\n"; break;
        case XML_DOCUMENT_TYPE_NODE: $o = $pad . 'D ' . $n->name . '|' . $n->publicId . '|' . $n->systemId . "\n"; break;
        default: $o = $pad . '? ' . $n->nodeType . "\n";
    }
    foreach ($n->childNodes as $c) { $o .= dumpDom($c, $d + 1); }
    return $o;
}
function mine(string $html, int $opts): string {
    $p = new __ElephcHtmlParser($html, $opts);
    $p->parse();
    return dumpTree($p->tree(0));
}
function theirs(string $html, int $opts): string {
    $d = new DOMDocument();
    @$d->loadHTML($html, $opts);
    return dumpDom($d);
}
$cases = unserialize(file_get_contents($argv[1]));
$bad = 0; $n = 0;
foreach ($cases as $c) { if ($c === "") continue;
    foreach ([0, 256 | 4, 8192 | 4, 256] as $opts) {
        $n++;
        $a = mine($c, $opts); $b = theirs($c, $opts);
        if ($a !== $b) {
            $bad++;
            if ($bad <= 8) { echo "--- DIFF opts=$opts input=" . bin2hex($c) . "\nmine:\n$a\ntheirs:\n$b\n"; }
        }
    }
}
echo "$n runs, $bad diffs\n";
