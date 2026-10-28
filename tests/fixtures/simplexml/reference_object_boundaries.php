<?php
function &referenceReturnBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->missing->child;
    return $node;
}
function &presentReferenceBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->present;
    return $node;
}
$referenceRoot = new SimpleXMLElement('<r><present>ok</present></r>');
try { $referenceResult =& referenceReturnBoundaryXml($referenceRoot); }
catch (TypeError $error) { echo explode(', called in ', $error->getMessage())[0], "\n"; }
$referenceResult =& presentReferenceBoundaryXml($referenceRoot);
echo $referenceResult->getName(), "\n";
