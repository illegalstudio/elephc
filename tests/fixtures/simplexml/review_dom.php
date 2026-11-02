<?php
class ReviewDomIgnoredText {
    public function __toString(): string { echo 'cast|'; return 'ignored'; }
}
// REVIEW_BODY

foreach ([false, true] as $liveHtml) {
    $liveDocument = new DOMDocument();
    if ($liveHtml) { $liveDocument->loadHTML('<div><a></a><b></b></div>'); }
    else { $liveDocument->loadXML('<r><a/><b/></r>'); }
    $documentRootBefore = $liveDocument->documentElement;
    $liveRoot = $liveHtml ? $liveDocument->getElementsByTagName('div')->item(0) : $liveDocument->documentElement;
    if ($liveRoot instanceof DOMElement) {
        $liveA = $liveRoot->childNodes->item(0);
        $liveB = $liveRoot->childNodes->item(1);
        $liveXml = simplexml_import_dom($liveRoot);
        if ($liveXml instanceof SimpleXMLElement && $liveA instanceof DOMElement && $liveB instanceof DOMElement) {
            var_dump($documentRootBefore === $liveDocument->documentElement, $liveRoot === $liveA->parentNode);
            var_dump($liveA === $liveDocument->getElementsByTagName('a')->item(0), $liveA->nextSibling === $liveB);
            unset($liveXml->b);
            var_dump($liveA->nextSibling, $liveB->parentNode, $liveB->previousSibling, $liveB->nextSibling);
            $liveXml->addChild('c', 'new');
            $liveC = $liveA->nextSibling;
            if ($liveC instanceof DOMElement) {
                echo $liveC->nodeName, '|';
                var_dump($liveC === $liveRoot->childNodes->item(1), $liveC->previousSibling === $liveA);
            }
            unset($liveXml->a);
            var_dump($liveA->parentNode, $liveA->previousSibling, $liveA->nextSibling);
        }
    }
}
$serialDocument = new DOMDocument();
$serialDocument->loadHTML('<div title="&#xE9;" data-x="&#x1F600;">&#xE9;<script>a &lt; b</script></div>');
$serialElement = $serialDocument->getElementsByTagName('div')->item(0);
$serialWholeBefore = $serialDocument->saveXML();
$serialNodeBefore = $serialDocument->saveXML($serialElement);
$serialXml = simplexml_import_dom($serialDocument);
var_dump($serialWholeBefore === $serialDocument->saveXML(), $serialNodeBefore === $serialDocument->saveXML($serialElement));
echo $serialDocument->getElementsByTagName('script')->item(0)->childNodes->item(0)->nodeName, '|';
if ($serialXml instanceof SimpleXMLElement && $serialElement instanceof DOMElement) {
    $serialElement->setAttribute('new', 'é');
    $serialXml->body->div['title'] = 'native';
    echo $serialElement->getAttribute('title'), ':', (string)$serialXml->body->div['new'], '|';
    var_dump($serialNodeBefore !== $serialDocument->saveXML($serialElement));
    $serialElement->nodeValue = 'A&unknown;B';
    var_dump(str_contains($serialDocument->saveXML($serialElement), 'A&unknown;B'), $serialElement->textContent === 'AB');
}
$rawDocument = new DOMDocument();
$rawDocument->loadHTML('<body><script>a<b</script><style>a<b</style></body>');
$rawXml = simplexml_import_dom($rawDocument);
foreach (['script', 'style'] as $rawTag) {
    $rawElement = $rawDocument->getElementsByTagName($rawTag)->item(0);
    if ($rawXml instanceof SimpleXMLElement && $rawElement instanceof DOMElement) {
        echo $rawElement->childNodes->item(0)->nodeName, '|';
        $rawXml->body->{$rawTag} = 'c<d';
        echo $rawElement->childNodes->item(0)->nodeName, ':', $rawDocument->saveXML($rawElement), '|';
        $rawElement->nodeValue = 'e<f';
        echo $rawElement->childNodes->item(0)->nodeName, ':', $rawDocument->saveXML($rawElement), '|';
        $rawElement->textContent = 'g<h';
        echo $rawElement->childNodes->item(0)->nodeName, ':', $rawDocument->saveXML($rawElement), '|';
    }
}
$cacheDocument = new DOMDocument();
$cacheDocument->loadXML('<r/>');
$cacheRoot = $cacheDocument->documentElement;
$cacheXml = simplexml_import_dom($cacheDocument);
$cacheRetained = [];
$cacheReleased = true;
if ($cacheXml instanceof SimpleXMLElement && $cacheRoot instanceof DOMElement) {
    for ($cacheIndex = 0; $cacheIndex < 256; $cacheIndex++) {
        $cacheXml->a = 'value';
        $cacheRetained[] = $cacheXml->a[0];
        $cacheNode = $cacheRoot->childNodes->item(0);
        if ($cacheNode instanceof DOMNode) {
            if ($cacheNode !== $cacheRoot->childNodes->item(0)) { $cacheReleased = false; }
            if (property_exists($cacheDocument, '__elephc_dom_nodes')) {
                if (count($cacheDocument->__elephc_dom_nodes) === 0) { $cacheReleased = false; }
            }
            $cacheWeak = WeakReference::create($cacheNode);
            unset($cacheNode);
            if ($cacheWeak->get() !== null) { $cacheReleased = false; }
        } else { $cacheReleased = false; }
        unset($cacheXml->a);
    }
}
var_dump($cacheReleased);
$cacheBounded = true;
if (property_exists($cacheDocument, '__elephc_dom_nodes')) { $cacheBounded = count($cacheDocument->__elephc_dom_nodes) < 128; }
var_dump($cacheBounded);
unset($cacheRetained, $cacheWeak, $cacheXml, $cacheRoot, $cacheDocument);
$literalDocument = new DOMDocument();
$literalDocument->loadHTML('<html xmlns="http://www.w3.org/1999/xhtml"><body><!-- ---- --><p xlink:href="#a" x-on:click="go" :class="active">A &amp; B</p><i>I</i></body></html>', LIBXML_NOERROR | LIBXML_NOWARNING);
$literalBody = $literalDocument->getElementsByTagName('body')->item(0);
$beforeChildren = $literalBody->childNodes;
echo $beforeChildren->item(1)->nextSibling->nodeName, ':', $beforeChildren->item(2)->previousSibling->nodeName, '|';
$literalXml = simplexml_import_dom($literalDocument);
echo count($literalXml->xpath('//p')), ':', count($literalXml->getDocNamespaces()), ':', (string)$literalXml['xmlns'], '|';
echo (string)$literalXml->body->p['xlink:href'], ':', (string)$literalXml->body->p['x-on:click'], ':', (string)$literalXml->body->p[':class'], '|';
echo (string)$literalXml->body->p, '|', $literalXml->asXML(), '|';
$afterChildren = $literalBody->childNodes;
echo $afterChildren->item(0)->nextSibling->nodeName, ':', $afterChildren->item(1)->previousSibling->nodeName, ':', $afterChildren->item(1)->nextSibling->nodeName, ':', $afterChildren->item(2)->previousSibling->nodeName, '|';
$literalXml->body->p['xlink:href'] = '#changed';
echo $literalDocument->getElementsByTagName('p')->item(0)->getAttribute('xlink:href'), '|';
$xmlSiblingDocument = new DOMDocument();
$xmlSiblingDocument->loadXML('<r><a/><b/></r>');
$xmlSiblingChildren = $xmlSiblingDocument->documentElement->childNodes;
echo $xmlSiblingChildren->item(0)->nextSibling->nodeName, ':', $xmlSiblingChildren->item(1)->previousSibling->nodeName, '|';
$outerDocument = new DOMDocument();
$outerDocument->loadHTML('<!-- lead --><?before data><html><body><p>x</p></body></html><!-- tail --><?after data>', LIBXML_NOERROR | LIBXML_NOWARNING | LIBXML_HTML_NODEFDTD);
$outerBefore = $outerDocument->saveXML();
$outerXml = simplexml_import_dom($outerDocument);
$outerAfter = $outerDocument->saveXML();
var_dump($outerBefore === $outerAfter);
$outerXml->body->p = 'changed';
$outerChanged = $outerDocument->saveXML();
echo (int)str_contains($outerChanged, '<!-- lead -->'), ':', (int)str_contains($outerChanged, '<!-- tail -->'), ':', (int)str_contains($outerChanged, 'before'), ':', (int)str_contains($outerChanged, 'after'), ':', (int)str_contains($outerChanged, '<p>changed</p>'), '|';
$metadataDocument = new DOMDocument();
$metadataDocument->loadXML('<r><![CDATA[raw]]><?work data?></r>');
$metadataChildren = $metadataDocument->documentElement->childNodes;
echo $metadataChildren->item(0)->nodeName, ':', $metadataChildren->item(0)->nodeValue, '|';
$metadataPi = $metadataChildren->item(1);
if ($metadataPi instanceof DOMProcessingInstruction) {
    echo $metadataPi->target, ':', $metadataPi->data, '|';
    $metadataPi->nodeValue = 'changed';
    echo $metadataPi->data, '|';
    $metadataPi->data = 'again';
    echo $metadataPi->nodeValue, ':', $metadataDocument->saveXML($metadataPi), '|';
} else { throw new Error('Expected the metadata processing instruction'); }
$scriptDocument = new DOMDocument();
$scriptDocument->loadHTML('<html><body><script>if (a < b) go()</script></body></html>', LIBXML_NOERROR | LIBXML_NOWARNING);
echo $scriptDocument->getElementsByTagName('script')->item(0)->childNodes->item(0)->nodeName, '|';
$scriptXml = simplexml_import_dom($scriptDocument);
echo $scriptDocument->getElementsByTagName('script')->item(0)->childNodes->item(0)->nodeName, '|';
$allDocument = new DOMDocument();
$allDocument->loadXML('<!--before--><?first one?><r><a/></r><?last two?><!--after-->');
$allChildren = $allDocument->childNodes;
echo $allChildren->length, ':';
foreach ($allChildren as $node) { echo $node->nodeName, ','; }
echo '|', $allDocument->documentElement->previousSibling->nodeName, ':', $allDocument->documentElement->nextSibling->nodeName, '|';
var_dump($allDocument->saveXML($allDocument) === $allDocument->saveXML());
echo $allDocument->saveXML($allDocument), '|';
$encodedDocument = new DOMDocument();
$encodedDocument->loadXML('<?xml version="1.0" encoding="ISO-8859-1"?><r>caf' . chr($argc > 0 ? 233 : 234) . '</r>');
echo bin2hex($encodedDocument->saveXML()), ':', bin2hex($encodedDocument->saveXML($encodedDocument)), '|';
$namespaceRetirementXml = new SimpleXMLElement('<r><a><b xmlns:p="urn:p"><p:c p:k="v"/></b></a></r>');
$namespaceRetirementXml->registerXPathNamespace('q', 'urn:p');
$retainedNamespaceChild = $namespaceRetirementXml->xpath('//q:c')[0];
$namespaceRetirementXml->a->b = 't';
$namespaceRetirementXml->a = 't2';
echo $retainedNamespaceChild->asXML(), ':', json_encode($retainedNamespaceChild->getNamespaces()), '|';
$namespaceSiblingsXml = new SimpleXMLElement('<r><p:i xmlns:p="urn:p">one</p:i></r>');
$namespaceSiblingGroup = $namespaceSiblingsXml->children('urn:p')->i;
$namespaceSiblingGroup[1]->v = 'two';
$retainedNamespaceSibling = $namespaceSiblingGroup[1];
unset($namespaceSiblingGroup[0]);
echo $retainedNamespaceSibling->asXML(), ':', json_encode($retainedNamespaceSibling->getNamespaces()), '|';
$cloneNamespaceSource = new SimpleXMLElement('<r><a xmlns:p="urn:p"><p:c p:k="v"/></a></r>');
$cloneNamespaceRoot = clone $cloneNamespaceSource->a;
$cloneNamespaceChild = $cloneNamespaceRoot->children('urn:p')->c[0];
$cloneNamespaceRoot[0] = 'replacement';
unset($cloneNamespaceRoot[0], $cloneNamespaceRoot);
echo $cloneNamespaceChild->asXML(), ':', json_encode($cloneNamespaceChild->getNamespaces()), '|';
$doctypeDocument = new DOMDocument();
$doctypeDocument->loadXML('<!DOCTYPE r><r/>');
echo $doctypeDocument->documentElement->nodeName, ':', $doctypeDocument->childNodes->length, '|';
$qualifiedDocument = new DOMDocument();
$qualifiedDocument->loadXML('<p:r xmlns:p="urn:p" xmlns:q="urn:p" p:a="1" xml:lang="it"><p:c/></p:r>');
$qualifiedRoot = $qualifiedDocument->documentElement;
$qualifiedChild = $qualifiedRoot->childNodes->item(0);
echo $qualifiedRoot->nodeName, ':', $qualifiedChild->nodeName, '|';
echo $qualifiedDocument->getElementsByTagName('r')->length, ':', $qualifiedDocument->getElementsByTagName('c')->length, ':', $qualifiedDocument->getElementsByTagName('p:c')->length, '|';
echo $qualifiedRoot->getAttribute('p:a'), ':', $qualifiedRoot->getAttribute('q:a'), ':', $qualifiedRoot->getAttribute('xml:lang'), ':', $qualifiedRoot->getAttribute('xmlns:p'), '|';
$qualifiedXml = simplexml_import_dom($qualifiedDocument);
$qualifiedAlias = $qualifiedXml->attributes('urn:p')['a'];
$qualifiedRoot->setAttribute('q:a', '3&');
$qualifiedRoot->setAttribute('p:new', 'added');
$qualifiedRoot->setAttribute('xml:lang', 'en');
echo $qualifiedRoot->getAttribute('p:a'), ':', $qualifiedRoot->getAttribute('q:a'), ':', $qualifiedRoot->getAttribute('p:new'), ':', $qualifiedRoot->getAttribute('xml:lang'), ':', $qualifiedAlias, '|';
var_dump($qualifiedRoot->setAttribute('xmlns:p', 'changed'));
echo $qualifiedDocument->saveXML(), '|';
foreach (['', '1bad', 'a b', chr(255), chr(0), chr(192) . chr(175), chr(237) . chr(160) . chr(128), chr(244) . chr(144) . chr(128) . chr(128)] as $badAttributeName) {
    try { $qualifiedRoot->setAttribute($badAttributeName, 'wrong'); echo 'wrong|'; }
    catch (ValueError $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    catch (DOMException $error) { echo get_class($error), ':', $error->getCode(), ':', $error->getMessage(), '|'; }
}
foreach (['é', '名', 'a' . chr(204) . chr(129), chr(240) . chr(144) . chr(128) . chr(128)] as $unicodeAttributeName) {
    $qualifiedRoot->setAttribute($unicodeAttributeName, 'valid');
    echo $qualifiedRoot->getAttribute($unicodeAttributeName), '|';
}
$qualifiedRoot->setAttribute("nul\0tail", 'truncated');
echo $qualifiedRoot->getAttribute('nul'), '|';
$namespaceDocument = new DOMDocument();
$namespaceDocument->loadHTML('<p class="c">x</p>');
$namespaceElement = $namespaceDocument->getElementsByTagName('p')->item(0);
var_dump($namespaceElement->setAttribute('xmlns', 'urn:q'), $namespaceElement->setAttribute('xmlns', 'urn:r'));
echo $namespaceDocument->saveXML($namespaceElement), '|';
$namespaceImport = simplexml_import_dom($namespaceDocument);
var_dump($namespaceElement->getAttribute("xmlns\0tail"), $namespaceElement->setAttribute('xmlns', 'urn:s'));
echo json_encode($namespaceImport->getDocNamespaces(true)), ':', (string)$namespaceImport->body->p['xmlns'], '|';
$rawNamespaceDocument = new DOMDocument();
$rawNamespaceDocument->loadHTML('<p xmlns="urn:raw" xmlns:p="urn:raw:p" class="c">x</p>');
$rawNamespaceElement = $rawNamespaceDocument->getElementsByTagName('p')->item(0);
echo '[', $rawNamespaceElement->getAttribute('xmlns'), ']:[', $rawNamespaceElement->getAttribute('xmlns:p'), ']|';
var_dump($rawNamespaceElement->setAttribute('xmlns', 'urn:é&amp;"'));
echo $rawNamespaceDocument->saveXML($rawNamespaceElement), '|';
$rawNamespaceImport = simplexml_import_dom($rawNamespaceDocument);
echo $rawNamespaceElement->getAttribute('xmlns'), ':', (string)$rawNamespaceImport->body->p['xmlns'], '|';
$rawNamespaceAlias = $rawNamespaceImport->body->p['xmlns:p'];
$rawNamespaceElement->setAttribute('xmlns:p', 'urn:changed');
echo $rawNamespaceAlias, ':', $rawNamespaceElement->getAttribute('xmlns:p'), '|';
$emptyNamespaceDocument = new DOMDocument(); $emptyNamespaceDocument->loadHTML('<p/>');
$emptyNamespaceElement = $emptyNamespaceDocument->getElementsByTagName('p')->item(0);
var_dump($emptyNamespaceElement->setAttribute('xmlns', ''), $emptyNamespaceElement->setAttribute('xmlns', 'changed'));
$emptyNamespaceImport = simplexml_import_dom($emptyNamespaceDocument);
echo $emptyNamespaceDocument->saveXML($emptyNamespaceElement), '|';
$textDocument = new DOMDocument();
$textDocument->loadXML('<!--lead--><r>a<b>c</b><!--note--><?work data?></r>');
echo $textDocument->textContent, '|';
$textRoot = $textDocument->documentElement;
$oldTextChild = $textRoot->childNodes->item(1);
$textRoot->nodeValue = 'Tom &amp; Jerry &#233; &lt;';
echo $textRoot->nodeValue, ':', $textDocument->saveXML($textRoot), ':', $oldTextChild->nodeValue, '|';
$textRoot->nodeValue = 'A&unknown;B';
echo $textRoot->nodeValue, ':', $textDocument->saveXML($textRoot), '|';
$textRoot->nodeValue = 'R&D';
echo $textRoot->nodeValue, '|';
$textRoot->textContent = 'Tom &amp; Jerry &#233; &lt;';
echo $textRoot->textContent, ':', $textDocument->saveXML($textRoot), '|';
$textDocument->nodeValue = new ReviewDomIgnoredText();
$textDocument->textContent = new ReviewDomIgnoredText();
var_dump($textDocument->nodeValue);
echo $textDocument->textContent, '|';
$htmlTextDocument = new DOMDocument();
$htmlTextDocument->loadHTML('<p>a<b>c</b><!--note--><?work data?></p>');
echo $htmlTextDocument->textContent, '|';
$htmlTextDocument->nodeValue = 'ignored'; $htmlTextDocument->textContent = 'ignored';
echo $htmlTextDocument->textContent, '|';
$htmlTextImport = simplexml_import_dom($htmlTextDocument);
$htmlTextElement = $htmlTextDocument->getElementsByTagName('p')->item(0);
$htmlTextElement->nodeValue = 'A &amp; B';
echo $htmlTextElement->nodeValue, ':', (string)$htmlTextImport->body->p, '|';
$htmlTextElement->textContent = 'A &amp; B';
echo $htmlTextElement->textContent, ':', $htmlTextDocument->textContent, ':', $htmlTextDocument->saveXML($htmlTextElement), '|';
$trailingXml = new SimpleXMLElement('<r xmlns:a="urn:a"/>');
$trailingChild = $trailingXml->addChild('a:', 'v');
echo $trailingChild->getName(), ':', $trailingChild->asXML(), '|';
$trailingXml->addAttribute('b:', '1');
$trailingQualified = $trailingXml->addChild('a:', 'v', 'urn:a');
echo $trailingQualified->getName(), ':', $trailingQualified->asXML(), '|';
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$trailingXml->addAttribute('a:', '2', 'urn:a');
restore_error_handler();
echo $trailingXml->asXML(), '|';
$emptyChildNamespaceXml = new SimpleXMLElement('<r/>');
$emptyChildNamespaceXml->addChild('p:e', 'w', '');
$emptyChildNamespaceXml->addChild('c', 'v', '');
echo $emptyChildNamespaceXml->asXML(), ':', count($emptyChildNamespaceXml->xpath('//e')), ':', (string)$emptyChildNamespaceXml->e, ':', count($emptyChildNamespaceXml->xpath('//c')), '|';
