<?php
function NodeViewRhs(): string { echo 'rhs|'; return str_repeat('x', 2); }
function NodeViewThrowRhs(): string { throw new RuntimeException('rhs-error'); }
function NodeViewKey(): int { echo 'key|'; return 0; }
function NodeViewWrites(SimpleXMLElement $value): void {
    try { $value->b->c = NodeViewRhs(); } catch (Error $error) { echo $error->getMessage(), '|'; }
    try { $value->b[] = NodeViewRhs(); } catch (Error $error) { echo $error->getMessage(), '|'; }
    try { $value->b[NodeViewKey()] = NodeViewRhs(); } catch (Error $error) { echo $error->getMessage(), '|'; }
    try { $value->b->c .= NodeViewRhs(); } catch (Error $error) { echo $error->getMessage(), '|'; }
    try { $value->b->c++; } catch (Error $error) { echo $error->getMessage(), '|'; }
    try { unset($value->b[NodeViewKey()]); } catch (Error $error) { echo $error->getMessage(), '|'; }
    try { unset($value->b['q']); } catch (Error $error) { echo $error->getMessage(), '|'; }
    try { $value->b->c = NodeViewThrowRhs(); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
    $value->b = 'ignored'; $value->b .= 'ignored'; $value->b++; unset($value->b);
    echo 'direct|';
}
$nodeViewRoot = new SimpleXMLElement('<r><a id="1"/><!--note--><?target info?></r>');
$nodeViewNodes = [$nodeViewRoot->a['id'], $nodeViewRoot->a->attributes()->id,
    $nodeViewRoot->xpath('//comment()')[0], $nodeViewRoot->xpath('//processing-instruction()')[0]];
foreach ($nodeViewNodes as $nodeViewNode) {
    if ($nodeViewNode instanceof SimpleXMLElement) { NodeViewWrites($nodeViewNode); }
    else { throw new Error('Expected a SimpleXML node'); }
}
echo $nodeViewRoot->asXML();
$nodeViewAttributeNodes = [$nodeViewRoot->a['id'], $nodeViewRoot->a->attributes()->id,
    $nodeViewRoot->a->attributes()['id']];
foreach ($nodeViewRoot->a->attributes() as $nodeViewAttribute) { $nodeViewAttributeNodes[] = $nodeViewAttribute; }
foreach ($nodeViewAttributeNodes as $nodeViewNode) {
    $nodeViewChildren = $nodeViewNode->children(); $nodeViewAttributes = $nodeViewNode->attributes();
    echo (int)($nodeViewChildren instanceof SimpleXMLElement), ':', count($nodeViewChildren), ':', $nodeViewChildren->count(), '|';
    echo (int)($nodeViewAttributes instanceof SimpleXMLElement), ':', count($nodeViewAttributes), ':', $nodeViewAttributes->count(), '|';
    foreach ($nodeViewChildren as $nodeViewBad) { echo 'bad-child|'; }
    foreach ($nodeViewAttributes as $nodeViewBad) { echo 'bad-attribute|'; }
}
$nodeViewControls = new SimpleXMLElement('<controls><a id="1"/><z id="2"/></controls>');
foreach ([$nodeViewControls->a->attributes(), $nodeViewControls->xpath('//z/@id')[0]] as $nodeViewList) {
    echo (int)($nodeViewList->children() === null), ':', (int)($nodeViewList->attributes() === null), '|';
    $nodeViewList->b->c = 'ignored';
}
echo $nodeViewControls->asXML();
class NodeViewAlias extends SimpleXMLElement {}
$nodeViewAliasRoot = new SimpleXMLElement('<r><a>first</a><a>second</a></r>');
$nodeViewAlias = simplexml_import_dom($nodeViewAliasRoot->a, NodeViewAlias::class);
echo get_class($nodeViewAlias), ':', $nodeViewAlias->getName(), '|';
$nodeViewAlias['shared'] = 'yes';
echo (string)$nodeViewAliasRoot->a['shared'], ':', (int)isset($nodeViewAliasRoot->a[1]['shared']), '|';
$nodeViewPlain = simplexml_import_dom($nodeViewAliasRoot, null);
echo get_class($nodeViewPlain), ':', $nodeViewPlain->getName(), '|';
unset($nodeViewAliasRoot->a[0]);
echo $nodeViewAlias->asXML(), $nodeViewAliasRoot->asXML();
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
foreach ([$nodeViewRoot->a['id'], $nodeViewRoot->xpath('//comment()')[0],
    $nodeViewRoot->xpath('//processing-instruction()')[0]] as $nodeViewInvalid) {
    if ($nodeViewInvalid instanceof SimpleXMLElement) {
        echo (int)(simplexml_import_dom($nodeViewInvalid) === null), '|';
    } else { throw new Error('Expected an invalid XML node'); }
}
restore_error_handler();
foreach ([$nodeViewRoot->missing, new stdClass()] as $nodeViewInvalid) {
    if ($nodeViewInvalid instanceof SimpleXMLElement) {
        try { simplexml_import_dom($nodeViewInvalid); } catch (TypeError $error) { echo $error->getMessage(), '|'; }
    } elseif ($nodeViewInvalid instanceof stdClass) {
        try { simplexml_import_dom($nodeViewInvalid); } catch (TypeError $error) { echo $error->getMessage(), '|'; }
    } else { throw new Error('Expected an invalid XML object'); }
}
function NodeViewFilteredAttribute(string $data): void {
    $root = new SimpleXMLElement($data); $selected = $root->xpath('//@*')[0];
    echo 'filter:', (int)($selected->b === null), ':', (int)($selected['b'] === null), ':',
        (int)($selected->zzz === null), ':', (int)isset($selected->b), ':', (int)isset($selected['b']), ':',
        (int)empty($selected->b), ':', (int)empty($selected['b']), '|';
    echo (string)$selected->id, ':', (string)$selected['id'], ':', (string)$selected[0], '|';
    unset($selected->b, $selected['b']); echo $root->asXML();
    $selected->b = 'new'; $selected['b'] = 'next'; $selected->b->c = 'ignored';
    echo $root->asXML();
    echo (int)($selected->b === null), ':', (int)isset($selected['b']), '|';
}
NodeViewFilteredAttribute('<r id="1" b="old"/>');
NodeViewFilteredAttribute('<r xmlns:p="urn:p" p:id="1" p:b="old"/>');
$nodeViewFiltered = simplexml_load_string('<r xmlns:p="u"><p:a><p:b>PB</p:b><b>UB</b></p:a><a>U</a></r>', null, 0, 'p', true);
if ($nodeViewFiltered instanceof SimpleXMLElement) {
    $nodeViewImported = simplexml_import_dom($nodeViewFiltered);
    echo $nodeViewImported->a->asXML(), '|';
    $nodeViewImportedChild = simplexml_import_dom($nodeViewFiltered->a);
    echo (string)$nodeViewImportedChild->b, ':', (string)$nodeViewFiltered->a->b, '|';
}
unset($nodeViewRoot, $nodeViewNodes, $nodeViewNode, $nodeViewAttributeNodes, $nodeViewAttribute,
    $nodeViewChildren, $nodeViewAttributes, $nodeViewList, $nodeViewControls, $nodeViewAliasRoot, $nodeViewAlias,
    $nodeViewPlain, $nodeViewInvalid, $nodeViewFiltered, $nodeViewImported, $nodeViewImportedChild, $error);
