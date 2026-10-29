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
foreach ($nodeViewNodes as $nodeViewNode) { NodeViewWrites($nodeViewNode); }
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
    echo (int)(simplexml_import_dom($nodeViewInvalid) === null), '|';
}
restore_error_handler();
foreach ([$nodeViewRoot->missing, new stdClass()] as $nodeViewInvalid) {
    try { simplexml_import_dom($nodeViewInvalid); } catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
unset($nodeViewRoot, $nodeViewNodes, $nodeViewNode, $nodeViewAttributeNodes, $nodeViewAttribute,
    $nodeViewChildren, $nodeViewAttributes, $nodeViewList, $nodeViewControls, $nodeViewAliasRoot, $nodeViewAlias,
    $nodeViewPlain, $nodeViewInvalid, $error);
