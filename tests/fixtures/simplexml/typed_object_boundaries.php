<?php
interface BoundaryXmlPeer {}
class BoundaryXmlElement extends SimpleXMLElement implements BoundaryXmlPeer {
    public function relativeVariadic(self ...$nodes): void { echo 'relative_variadic|'; }
}
function reportBoundaryXml(TypeError $error): void {
    $message = explode(', called in ', $error->getMessage())[0];
    if (str_contains($message, '{closure')) {
        $message = substr($message, strpos($message, 'Argument #'));
    }
    echo $message, "\n";
}
function acceptBoundaryXml(SimpleXMLElement $node): void { echo 'entered|'; }
function acceptSpecificBoundaryXml(BoundaryXmlElement $node): void { echo 'specific|'; }
function acceptObjectBoundaryXml(object $node): void { echo 'object|'; }
function acceptPeerBoundaryXml(BoundaryXmlPeer $node): void { echo 'peer|'; }
function acceptUnionBoundaryXml(SimpleXMLElement|SimpleXMLIterator $node): void { echo 'union|'; }
function acceptNullableUnionBoundaryXml(SimpleXMLElement|SimpleXMLIterator|null $node): void { echo 'nullable|'; }
function acceptRefBoundaryXml(SimpleXMLElement &$node): void { echo 'reference|'; }
function acceptIterableBoundaryXml(iterable $nodes): void { echo 'iterable|'; }
function variadicBoundaryXml(int $prefix, SimpleXMLElement ...$nodes): void { echo 'variadic|'; }
function referenceVariadicBoundaryXml(SimpleXMLElement &...$nodes): void { echo 'reference_variadic|'; }
function returnIterableBoundaryXml(SimpleXMLElement $root): iterable { return $root->missing->child; }
function returnBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    return $root->missing->child;
}
function nullableBoundaryXml(SimpleXMLElement $root): ?SimpleXMLElement {
    return $root->missing->child;
}
function returnUnionBoundaryXml(SimpleXMLElement $root): SimpleXMLElement|SimpleXMLIterator {
    return $root->missing->child;
}
function &referenceReturnBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->missing->child;
    return $node;
}
function &presentReferenceBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->present;
    return $node;
}
class BoundaryXmlBox {
    public SimpleXMLElement $node;
    public ?SimpleXMLElement $nullable = null;
    public static SimpleXMLElement $staticNode;
    public SimpleXMLElement|int $union = 1;
    public iterable $items;
    public static iterable $staticItems;
    public ?iterable $nullableItems = null;
}
class BoundaryXmlConstructor {
    public function __construct(SimpleXMLElement $node) { echo 'constructor|'; }
}
class BoundaryXmlStaticBase {
    public static SimpleXMLElement $node;
    public static function store(SimpleXMLElement $root): void { static::$node = $root->missing->child; }
}
class BoundaryXmlStaticChild extends BoundaryXmlStaticBase {
    public static SimpleXMLElement $node;
}
class BoundaryXmlMethods {
    public function accept(SimpleXMLElement $node): void { echo 'method|'; }
    public static function acceptStatic(SimpleXMLElement $node): void { echo 'static|'; }
}
class BoundaryXmlHook {
    public SimpleXMLElement $node {
        set { echo 'hook|'; $this->node = $value; }
    }
}
$root = new BoundaryXmlElement('<r><present>ok</present></r>');
try { acceptBoundaryXml($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { acceptSpecificBoundaryXml($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { acceptObjectBoundaryXml($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { acceptPeerBoundaryXml($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { acceptUnionBoundaryXml($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { acceptBoundaryXml(node: $root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$missing = $root->missing->child;
try { acceptBoundaryXml($missing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { acceptRefBoundaryXml($missing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { acceptIterableBoundaryXml($missing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { variadicBoundaryXml(1, $root, $missing); } catch (TypeError $error) { reportBoundaryXml($error); }
$arguments = [$root, $missing];
try { variadicBoundaryXml(1, ...$arguments); } catch (TypeError $error) { reportBoundaryXml($error); }
try { variadicBoundaryXml(1, good: $root, bad: $missing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { referenceVariadicBoundaryXml($missing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { returnBoundaryXml($root); } catch (TypeError $error) { reportBoundaryXml($error); }
try { returnUnionBoundaryXml($root); } catch (TypeError $error) { reportBoundaryXml($error); }
try { returnIterableBoundaryXml($root); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $returned =& referenceReturnBoundaryXml($root); } catch (TypeError $error) { reportBoundaryXml($error); }
$box = new BoundaryXmlBox();
$box->node = $root->present;
try { $box->node = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
$property = 'node';
try { $box->{$property} = $missing; } catch (TypeError $error) { reportBoundaryXml($error); }
echo $box->node->getName(), "\n";
try { $box->union = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
var_dump($box->union);
$box->items = $root;
try { $box->items = $missing; } catch (TypeError $error) { reportBoundaryXml($error); }
BoundaryXmlBox::$staticItems = $root;
try { BoundaryXmlBox::$staticItems = $missing; } catch (TypeError $error) { reportBoundaryXml($error); }
$box->nullableItems = $missing;
var_dump($box->nullableItems);
BoundaryXmlBox::$staticNode = $root->present;
try { BoundaryXmlBox::$staticNode = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
echo BoundaryXmlBox::$staticNode->getName(), "\n";
BoundaryXmlStaticChild::$node = $root->present;
try { BoundaryXmlStaticChild::store($root); } catch (TypeError $error) { reportBoundaryXml($error); }
echo BoundaryXmlStaticChild::$node->getName(), "\n";
try { new BoundaryXmlConstructor($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$methods = new BoundaryXmlMethods();
try { $methods->accept($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { BoundaryXmlMethods::acceptStatic($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $root->relativeVariadic($missing); } catch (TypeError $error) { reportBoundaryXml($error); }
$callback = acceptBoundaryXml(...);
try { $callback($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$callback = $argc > 0 ? 'acceptBoundaryXml' : 'unusedBoundaryXml';
try { $callback($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$closure = function (SimpleXMLElement $node): void { echo 'closure|'; };
try { $closure($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$hook = new BoundaryXmlHook();
try { $hook->node = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
$box->nullable = $root->missing->child;
var_dump($box->nullable, nullableBoundaryXml($root));
$returned =& presentReferenceBoundaryXml($root);
echo $returned->getName(), "\n";
acceptBoundaryXml($root->missing);
$present = $root->present;
acceptRefBoundaryXml($present);
acceptPeerBoundaryXml($present);
acceptNullableUnionBoundaryXml($root->missing->child);
acceptIterableBoundaryXml($root);
variadicBoundaryXml(1, $root, named: $root);
referenceVariadicBoundaryXml($root);
echo "done\n";
