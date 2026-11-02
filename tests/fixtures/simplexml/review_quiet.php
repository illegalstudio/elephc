<?php
class ReviewQuietOwnerXml extends SimpleXMLElement {
    public function __destruct() { echo 'D|'; }
}
function reviewQuietOwnerMixed(mixed $node): void {
    $name = 'a';
    var_dump(isset($node->a), isset($node->{$name}), isset($node['id']), isset($node[0]));
}
class ReviewTrueHookXml extends SimpleXMLElement {
    public function __isset(string $name): bool { echo 'wrong-isset|'; return true; }
    public function __get(string $name): mixed { echo 'wrong-get|'; return 'wrong'; }
    public function selfProbe(): mixed { return $this->missing ?? 'self-fb'; }
}
function reviewNullableHookCoalesce(?ReviewTrueHookXml $node): mixed { return $node->missing ?? 'nullable-fb'; }
function reviewNativeHookCoalesce(): void {
    $node = new ReviewTrueHookXml('<r><a>native</a></r>');
    echo $node->missing ?? 'fallback', '|';
    $node->added ??= 'v';
    echo (string)$node->added, '|', $node->selfProbe(), '|', reviewNullableHookCoalesce($node), '|', reviewNullableHookCoalesce(null), '|';
}
// REVIEW_BODY

$quietXml = simplexml_load_string('<r id="1"><a>x</a></r>', 'ReviewQuietOwnerXml');
$quietAttributes = $quietXml->attributes();
$quietName = 'a';
echo 's|';
var_dump(isset($quietXml->a), isset($quietXml->{$quietName}), isset($quietXml['id']), isset($quietXml[0]), isset($quietXml[-1]));
var_dump(isset($quietAttributes->missing), $quietAttributes->missing);
echo 'm|'; reviewQuietOwnerMixed($quietXml);
echo $quietXml->missing ?? 'fb', '|c|', $quietXml->a ?? 'fb', '|e|';
var_dump(empty($quietXml->a), empty($quietXml->{$quietName}), empty($quietXml['id']));
echo 'u|'; unset($quietAttributes, $quietXml); echo 'end|';
reviewNativeHookCoalesce();

