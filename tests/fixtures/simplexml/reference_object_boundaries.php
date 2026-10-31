<?php
function &referenceReturnBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->missing->child;
    return $node;
}
function &presentReferenceBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->present;
    return $node;
}
function &finallyReferenceBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->present;
    try { return $node; }
    finally { $node = $root->changed; }
}
$referenceRoot = new SimpleXMLElement('<r><present>ok</present><changed>later</changed></r>');
try { $referenceResult =& referenceReturnBoundaryXml($referenceRoot); }
catch (TypeError $error) { echo explode(', called in ', $error->getMessage())[0], "\n"; }
$referenceResult =& presentReferenceBoundaryXml($referenceRoot);
echo $referenceResult->getName(), "\n";
$finallyReferenceResult =& finallyReferenceBoundaryXml($referenceRoot);
echo $finallyReferenceResult->getName(), "\n";
$finallyReferenceResult['id'] = 'alias';
echo (string)$referenceRoot->changed['id'], "\n";
unset($referenceResult, $finallyReferenceResult, $referenceRoot, $error);
