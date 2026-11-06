<?php
$dom = new DOMDocument();
// Comments and processing instructions remain nodes, not the element's descendant text.
$html = '<div class="text-green-500">Hi<!--note--><?renderer theme=green><span> there</span><b>!</b></div>';
$dom->loadHTML(
    $html,
    LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL
);

$body = $dom->getElementsByTagName("body")->item(0);
foreach ($body->childNodes as $node) {
    echo $node->nodeName;
    echo " class=";
    echo $node->getAttribute("class");
    echo " text=";
    echo $node->nodeValue;
    echo "\n";
}
