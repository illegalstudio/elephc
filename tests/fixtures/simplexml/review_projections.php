<?php
class ReviewProjectionDocument extends DOMDocument {
    public function saveXML(mixed $node = null, int $options = 0): string { echo 'wrong-save|'; return '<wrong/>'; }
}
class ReviewProjectionXml extends SimpleXMLElement {
    public function __destruct() { echo 'D:', parent::getName(), '|'; }
    public function count(): int { echo 'wrong-count|'; return parent::count(); }
    public function valid(): bool { echo 'wrong-valid|'; return parent::valid(); }
}
function reviewProjectionOccurrences(string $text, string $needle): int { return substr_count($text, $needle); }
// REVIEW_BODY

$projectedXml = simplexml_load_string('<r id="1"><a>x</a><b><c/></b></r>', 'ReviewProjectionXml');
echo 'J|', json_encode($projectedXml), '|';
echo 'P|'; $projectedPrint = print_r($projectedXml, true);
echo (int)str_contains($projectedPrint, 'ReviewProjectionXml Object'), ':', reviewProjectionOccurrences($projectedPrint, 'D:b|'), '|';
echo 'V|'; ob_start(); var_dump($projectedXml); $projectedDump = ob_get_clean();
echo (int)str_contains($projectedDump, 'object(ReviewProjectionXml)'), ':', reviewProjectionOccurrences($projectedDump, 'D:b|'), '|';
echo 'A|'; $projectedArray = (array)$projectedXml;
echo get_class($projectedArray['b']), '|'; unset($projectedArray);
echo 'O|'; $projectedVars = get_object_vars($projectedXml);
echo get_class($projectedVars['b']), '|'; unset($projectedVars);
echo 'B|'; $projectedInfo = $projectedXml->__debugInfo();
echo get_class($projectedInfo['b']), '|'; unset($projectedInfo);
echo 'R|'; unset($projectedXml); echo 'end|';
$projectionDocument = new ReviewProjectionDocument();
$projectionDocument->loadHTML('<html><body><a>value</a></body></html>');
$projectionImported = simplexml_import_dom($projectionDocument, 'ReviewProjectionXml');
echo get_class($projectionImported), '|'; unset($projectionImported);
echo 'nodes|'; unset($projectionDocument); echo 'done|';
