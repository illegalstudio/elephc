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
function orderedVariadicBoundaryXml(SimpleXMLElement $first, SimpleXMLElement ...$nodes): void { echo 'entered|'; }
function referenceVariadicBoundaryXml(SimpleXMLElement &...$nodes): void { echo 'reference_variadic|'; }
function referenceStorageBoundaryXml(mixed $node): mixed { return $node; }
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
function aliasedReturnBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->missing->child;
    $alias =& $node;
    return $alias;
}
function presentAliasedBoundaryXml(SimpleXMLElement $root): SimpleXMLElement {
    $node = $root->present;
    $alias =& $node;
    return $alias;
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
class BoundaryXmlReturnBase extends BoundaryXmlElement {
    public function missingStatic(): static { return $this->missing->child; }
    public function missingSelf(): self { return $this->missing->child; }
    public static function missingStaticCall(self $root): static { return $root->missing->child; }
}
class BoundaryXmlReturnChild extends BoundaryXmlReturnBase {}
function staticStateProbeBoundaryXml(SimpleXMLElement $root): void {
    $missing = $root->missing->child;
    $state = 0;
    try { $state = 1; BoundaryXmlBox::$staticNode = $missing; $state = 2; }
    catch (TypeError $error) { echo $state, ':'; reportBoundaryXml($error); }
}
function reflectionStateProbeBoundaryXml(SimpleXMLElement $root): void {
    $property = new ReflectionProperty(BoundaryXmlBox::class, 'staticNode');
    $missing = $root->missing->child;
    $state = 0;
    try { $state = 1; $property->setValue(null, $missing); $state = 2; }
    catch (TypeError $error) { echo $state, ':'; reportBoundaryXml($error); }
}
class BoundaryXmlMethods {
    public function accept(SimpleXMLElement $node): void { echo 'method|'; }
    public static function acceptStatic(SimpleXMLElement $node): void { echo 'static|'; }
    public function __propset_node(SimpleXMLElement $node): void { echo 'entered|'; }
    public function __propget_node(SimpleXMLElement $root): SimpleXMLElement { return $root->missing->child; }
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
try { acceptIterableBoundaryXml($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { variadicBoundaryXml(1, $root, $root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$arguments = [$root, $root->missing->child];
try { variadicBoundaryXml(1, ...$arguments); } catch (TypeError $error) { reportBoundaryXml($error); }
try { variadicBoundaryXml(1, good: $root, bad: $root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$referenceMissingElement = referenceStorageBoundaryXml($root->missing->child);
try { referenceVariadicBoundaryXml($referenceMissingElement); } catch (TypeError $error) { reportBoundaryXml($error); }
try { orderedVariadicBoundaryXml($root->missing->child, $root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { orderedVariadicBoundaryXml($root->missing->child, bad: $root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { orderedVariadicBoundaryXml(bad: $root->missing->child, first: $root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { returnBoundaryXml($root); } catch (TypeError $error) { reportBoundaryXml($error); }
try { returnUnionBoundaryXml($root); } catch (TypeError $error) { reportBoundaryXml($error); }
try { returnIterableBoundaryXml($root); } catch (TypeError $error) { reportBoundaryXml($error); }
try { aliasedReturnBoundaryXml($root); } catch (TypeError $error) { reportBoundaryXml($error); }
$box = new BoundaryXmlBox();
$box->node = $root->present;
try { $box->node = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
$property = 'node';
try { $box->{$property} = $missing; } catch (TypeError $error) { reportBoundaryXml($error); }
$instanceProperty = new ReflectionProperty(BoundaryXmlBox::class, 'node');
try { $instanceProperty->setValue($box, $missing); } catch (TypeError $error) { reportBoundaryXml($error); }
// Raw reflection is exposed through eval, including writes to generated instance storage.
eval('try { $instanceProperty->setRawValue($box, $missing); } catch (TypeError $error) { reportBoundaryXml($error); }');
// Eval can replace visible locals; retain a checked root for the following typed boundaries.
if ($root instanceof BoundaryXmlElement) {
echo $box->node->getName(), "\n";
try { $box->union = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
var_dump($box->union);
$box->items = $root;
try { $box->items = $missing; } catch (TypeError $error) { reportBoundaryXml($error); }
BoundaryXmlBox::$staticItems = $root;
try { BoundaryXmlBox::$staticItems = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
$box->nullableItems = $missing;
var_dump($box->nullableItems);
BoundaryXmlBox::$staticNode = $root->present;
try { BoundaryXmlBox::$staticNode = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
echo BoundaryXmlBox::$staticNode->getName(), "\n";
BoundaryXmlStaticChild::$node = $root->present;
try { BoundaryXmlStaticChild::store($root); } catch (TypeError $error) { reportBoundaryXml($error); }
echo BoundaryXmlStaticChild::$node->getName(), "\n";
staticStateProbeBoundaryXml($root);
reflectionStateProbeBoundaryXml($root);
$lateReturn = new BoundaryXmlReturnChild('<r/>');
try { $lateReturn->missingStatic(); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $lateReturn->missingSelf(); } catch (TypeError $error) { reportBoundaryXml($error); }
try { BoundaryXmlReturnChild::missingStaticCall($lateReturn); } catch (TypeError $error) { reportBoundaryXml($error); }
try { new BoundaryXmlConstructor($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$methods = new BoundaryXmlMethods();
try { $methods->__propset_node($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $methods->__propget_node($root); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $methods->accept($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { BoundaryXmlMethods::acceptStatic($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $root->relativeVariadic($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$callback = acceptBoundaryXml(...);
try { $callback($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$namedCallback = $argc > 0 ? 'acceptBoundaryXml' : 'unusedBoundaryXml';
try { $namedCallback($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$closure = function (SimpleXMLElement $node): void { echo 'closure|'; };
try { $closure($root->missing->child); } catch (TypeError $error) { reportBoundaryXml($error); }
$hook = new BoundaryXmlHook();
try { $hook->node = $root->missing->child; } catch (TypeError $error) { reportBoundaryXml($error); }
$box->nullable = $root->missing->child;
var_dump($box->nullable, nullableBoundaryXml($root));
$returned = presentAliasedBoundaryXml($root);
echo $returned->getName(), "\n";
acceptBoundaryXml($root->missing);
$present = $root->present;
acceptRefBoundaryXml($present);
acceptPeerBoundaryXml($root->present);
acceptNullableUnionBoundaryXml($root->missing->child);
acceptIterableBoundaryXml($root);
variadicBoundaryXml(1, $root, named: $root);
$referencePresentElement = referenceStorageBoundaryXml($root);
referenceVariadicBoundaryXml($referencePresentElement);
echo "done\n";
} else { throw new Error("Expected the original boundary XML root"); }
