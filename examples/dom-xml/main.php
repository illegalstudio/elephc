<?php
$document = new DOMDocument();
if (!$document->loadXML('<root><message>Hello DOM</message></root>')) {
    exit(1);
}

$root = $document->documentElement;
$message = $root->firstElementChild;
$status = $document->createElement('status', 'ready');
if ($status === false) {
    exit(2);
}

$message->insertAdjacentElement('beforebegin', $status);
$message->insertAdjacentText('beforeend', '!');

echo $document->saveXML();

$fragment = $document->createDocumentFragment();
$fragment->appendXML('<fragment><message>Legacy chunk</message></fragment>');
$document->documentElement->append($fragment);
echo $document->saveXML($document->documentElement);

echo $document->C14N(false, true), "\n";
$canonicalPath = '/tmp/elephc-dom-example-c14n.xml';
$canonicalBytes = $document->C14NFile($canonicalPath);
if ($canonicalBytes === false) {
    exit(4);
}
echo $canonicalBytes, ':', file_get_contents($canonicalPath), "\n";
unlink($canonicalPath);

$modern = Dom\XMLDocument::createFromString(
    '<root xmlns="urn:elephc:default" xmlns:demo="urn:elephc:dom-example">'
    . '<message>Hello modern DOM</message></root>',
);
$modernRoot = $modern->documentElement;
$namespaceInfos = $modernRoot->getInScopeNamespaces();
foreach ($namespaceInfos as $namespaceInfo) {
    echo $namespaceInfo->prefix ?? '(default)';
    echo '=', $namespaceInfo->namespaceURI, "\n";
}
echo count($modernRoot->getDescendantNamespaces()), " namespace records\n";

$modernMessage = $modernRoot->firstElementChild;
$badge = $modern->createElement('badge');
$badge->textContent = 'new';
$modernMessage->insertAdjacentElement(
    Dom\AdjacentPosition::BeforeBegin,
    $badge,
);
$modernMessage->insertAdjacentText(Dom\AdjacentPosition::BeforeEnd, '!');
$badge->rename('urn:elephc:dom-example', 'demo:badge');
$modernMessage->innerHTML = '<strong>Hello markup</strong>';
$modernMessage->insertAdjacentHTML(
    Dom\AdjacentPosition::AfterEnd,
    '<tail>done</tail>',
);
$modernFragment = $modern->createDocumentFragment();
$modernFragment->appendXml('<fragment><message>Modern chunk</message></fragment>');
$modernRoot->append($modernFragment);

echo $modern->saveXml();
echo $modern->C14N(), "\n";

$xinclude = new DOMDocument();
$xinclude->loadXML(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include href="missing.xml"><xi:fallback>'
    . '<included>Fallback content</included>'
    . '</xi:fallback></xi:include>'
    . '</root>',
);
if (@$xinclude->xinclude() !== 1) {
    exit(3);
}
echo $xinclude->saveXML();

$validation = new DOMDocument();
$validation->loadXML('<inventory><item/></inventory>');
$schema = '<schema xmlns="http://www.w3.org/2001/XMLSchema">'
    . '<element name="inventory"><complexType><sequence>'
    . '<element name="item"/></sequence></complexType></element></schema>';
$relaxNg = '<element name="inventory" '
    . 'xmlns="http://relaxng.org/ns/structure/1.0">'
    . '<element name="item"><empty/></element></element>';
echo $validation->schemaValidateSource($schema)
    ? "XSD validation passed\n"
    : "XSD validation failed\n";
echo $validation->relaxNGValidateSource($relaxNg)
    ? "Relax NG validation passed\n"
    : "Relax NG validation failed\n";
?>
