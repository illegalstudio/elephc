<?php
class ReviewNumericZeroXml extends SimpleXMLElement {}
function reviewCreateUpdateNode(SimpleXMLElement $liveUpdate): int {
    echo (int)isset($liveUpdate->a->b->c), '|';
    $liveUpdate->a->b->c = 7;
    return 1;
}
class ReviewCaptureNode { public mixed $c = 1; }
class ReviewCaptureName {
    private bool $fail;
    public function __construct(bool $fail = false) { $this->fail = $fail; }
    public function __toString(): string {
        echo 'str|'; if ($this->fail) { throw new RuntimeException('capture-name'); } return 'b';
    }
    public function __destruct() { echo '~name|'; }
}
class ReviewCaptureText {
    public function __toString(): string { return 'x'; }
    public function __destruct() { echo '~text|'; }
}
class ReviewCaptureKey { public function __destruct() { echo '~key|'; } }
class ReviewCaptureArray implements ArrayAccess {
    private ReviewCaptureNode $node;
    public function __construct() { $this->node = new ReviewCaptureNode(); }
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetGet(mixed $offset): mixed { return $this->node; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
function reviewCaptureName(): ReviewCaptureName { echo 'g|'; return new ReviewCaptureName(); }
function reviewCaptureThrowName(): ReviewCaptureName { echo 'g|'; return new ReviewCaptureName(true); }
function reviewCaptureValue(): int { echo 'f|'; return 2; }
function reviewCaptureText(): ReviewCaptureText { echo 'f|'; return new ReviewCaptureText(); }
function reviewCaptureThrowRhs(): never { throw new RuntimeException('capture-rhs'); }
function reviewCapturedCoalesce(mixed $root, mixed $name): void { $root->{$name}->c ??= reviewCaptureValue(); }
function reviewEvalCapturedStatements(string $code): void { eval($code); }
class ReviewTypedPrefix { public string $s = '5'; }
function reviewTypedPrefix(mixed $object): mixed { return ++$object->s; }
class ReviewExpiryValue { public function __destruct() { echo 'D'; } }
class ReviewExpiryNode { public mixed $child = null; }
class ReviewExpiryHolder { public array $items = []; }
class ReviewOrderObject { public int $p = 0; }
function reviewOrderName(): string { echo 'g'; return 'a'; }
function reviewOrderIndex(): int { echo 'k'; return 0; }
function reviewOrderValue(): int { echo 'f'; return 1; }
function reviewOrderOrdinary(mixed $items): void {
    $items[reviewOrderIndex()]->p = reviewOrderValue();
    echo '|', $items[0]->p, '|';
}
class ReviewWriteName {
    public function __toString(): string { echo 'name|'; return 'converted'; }
}
function reviewChangeWriteName(mixed &$lateName): int {
    echo 'rhs|'; $lateName = 'new'; return 1;
}
class ReviewHookInner { public string $label = ''; }
class ReviewHookOuter {
    private ReviewHookInner $inner;
    public ReviewHookInner $child { get { echo 'get|'; return $this->inner; } }
    public int $count = 1 { get { echo 'count|'; return $this->count * 10; } }
    public function __construct() { $this->inner = new ReviewHookInner(); }
}
function reviewWriteHookLabel(mixed $node): void { $node->child->label = 'set'; }
function reviewBumpHookCount(mixed $node): int { return $node->count += 1; }
// REVIEW_BODY

set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
$updates = new SimpleXMLElement('<r/>');
$updates->a->b->c += 1;
$updates->d->e->f++;
$updates->text .= 'suffix';
echo $updates->asXML(), '|';
$existingUpdate = new SimpleXMLElement('<r><c/></r>');
$existingUpdate->c += 1; echo $existingUpdate->asXML(), '|';
$attributeUpdate = new SimpleXMLElement('<r a="5"/>');
$attributeUpdate->attributes()->a += 1;
$attributeUpdate->attributes()->b += 1;
echo $attributeUpdate->asXML(), '|';
$liveUpdate = new SimpleXMLElement('<r/>');
$liveUpdate->a->b->c += reviewCreateUpdateNode($liveUpdate);
echo (string)$liveUpdate->a->b->c, '|';
$offsetUpdate = new SimpleXMLElement('<r/>');
$offsetUpdate->a[0]->b = 1;
$offsetUpdate->a[1]->b = 2;
echo $offsetUpdate->asXML(), '|';
$hookUpdate = new ReviewHookOuter();
reviewWriteHookLabel($hookUpdate); echo $hookUpdate->child->label, '|';
var_dump(reviewBumpHookCount($hookUpdate));
$hookUpdate->child->label = 'direct'; echo $hookUpdate->child->label, '|';
$hookUpdate->count += 1; echo $hookUpdate->count, '|';
$computedOrder = new SimpleXMLElement('<r/>');
$computedOrder->{reviewOrderName()}->c = reviewOrderValue(); echo '|', $computedOrder->asXML();
$computedOrder = new SimpleXMLElement('<r/>');
$computedOrder->a[reviewOrderIndex()]->c = reviewOrderValue(); echo '|', $computedOrder->asXML();
$computedOrder = new SimpleXMLElement('<r/>');
$computedOrder->{reviewOrderName()}->list[] = reviewOrderValue(); echo '|', $computedOrder->asXML();
$computedOrder = new SimpleXMLElement('<r/>');
$computedOrder->a[reviewOrderIndex()]->c .= reviewOrderValue(); echo '|', $computedOrder->asXML();
$computedOrder = new SimpleXMLElement('<r/>');
$computedOrder->a[reviewOrderIndex()]->c++; echo '|', $computedOrder->asXML();
$computedOrder = new SimpleXMLElement('<r/>');
$computedOrder->{reviewOrderName()}->b->c += 1; echo '|', $computedOrder->asXML();
reviewOrderOrdinary([new ReviewOrderObject()]);
$expiryRows = [new ReviewExpiryNode()];
$expiryRows[reviewOrderIndex()]->child = new ReviewExpiryValue();
$expiryRows[0]->child = null; echo 'released|';
unset($expiryRows);
$expiryHolder = new ReviewExpiryHolder();
$expiryHolder->items[reviewOrderIndex()] = new ReviewExpiryValue();
unset($expiryHolder->items[0]); echo 'released|';
unset($expiryHolder);

$resourceRoot = new SimpleXMLElement('<r id="old"><a>old</a></r>');
$resourceInput = fopen('php://memory', 'r');
$resourceClosed = fopen('php://memory', 'r'); fclose($resourceClosed);
foreach ([$resourceInput, $resourceClosed] as $resourceValue) {
    try { $resourceRoot->a = $resourceValue; } catch (TypeError $resourceError) { echo $resourceError->getMessage(), '|'; }
    try { $resourceRoot['id'] = $resourceValue; } catch (TypeError $resourceError) { echo $resourceError->getMessage(), '|'; }
}
echo $resourceRoot->asXML(), '|'; fclose($resourceInput);
unset($resourceRoot, $resourceInput, $resourceClosed, $resourceValue, $resourceError);
unset($computedOrder);
$lateName = 'old'; $lateUpdate = new SimpleXMLElement('<r/>');
$lateUpdate->{$lateName} += reviewChangeWriteName($lateName);
echo $lateUpdate->asXML(), '|';
$lateName = 'old'; $lateUpdate = new SimpleXMLElement('<r/>');
$lateUpdate->{$lateName}->leaf = reviewChangeWriteName($lateName);
echo $lateUpdate->asXML(), '|';
$nameObject = new ReviewWriteName();
$lateUpdate->{$nameObject} = reviewChangeWriteName($lateName);
echo $lateUpdate->asXML(), '|';
unset($lateName, $lateUpdate, $nameObject);
reviewEvalCapturedStatements('
$root = new stdClass(); $root->b = new ReviewCaptureNode(); $root->a = [new ReviewCaptureNode()];
$root->{reviewCaptureName()}->c = reviewCaptureValue(); echo \'after-plain|\';
$root->{reviewCaptureName()}->c += 1; echo \'after-compound|\';
$root->{reviewCaptureName()}->c++; echo \'after-incdec|\';
$root->b->c = \'p\';
$root->{reviewCaptureName()}->c .= reviewCaptureText(); echo \'after-name-text|\';
$root->a[reviewOrderIndex()]->c = \'p\';
$root->a[reviewOrderIndex()]->c .= reviewCaptureText(); echo \'after-text|\';
try { $root->{reviewCaptureName()}->c .= reviewCaptureThrowRhs(); }
catch (RuntimeException $captureError) { echo $captureError->getMessage(), \'|\'; }
try { $root->{reviewCaptureThrowName()}->c = reviewCaptureValue(); }
catch (RuntimeException $captureError) { echo $captureError->getMessage(), \'|\'; }
$root->b->c = 1;
reviewCapturedCoalesce($root, reviewCaptureName()); echo \'after-keep|\';
unset($root->b->c);
reviewCapturedCoalesce($root, reviewCaptureName()); echo \'after-fallback|\';
for ($captureIteration = 0; $captureIteration < 2; $captureIteration++) {
    $root->{reviewCaptureName()}->c += 1; echo \'after-loop|\';
}
$arrayRoot = new ReviewCaptureArray();
$arrayRoot[new ReviewCaptureKey()]->c = 2; echo \'after-key|\';
unset($root, $arrayRoot, $captureError, $captureIteration);
echo \'end-captures|\';
');
$typedPrefix = new ReviewTypedPrefix();
var_dump(reviewTypedPrefix($typedPrefix), $typedPrefix->s);
unset($typedPrefix);

restore_error_handler();
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$badOffset = new SimpleXMLElement('<r/>');
try { $badOffset->a[1]->b = 1; } catch (Error $offsetError) { echo $offsetError->getMessage(), '|'; }
echo $badOffset->asXML(), '|';
$appendedOffset = new SimpleXMLElement('<r><a/></r>');
$appendedOffset->a[3]->b = 3;
echo $appendedOffset->asXML(), '|';
$subclassOffset = new ReviewNumericZeroXml('<r><a>1</a></r>');
try { $subclassOffset['zz']->b = 1; } catch (Error $subclassOffsetError) { echo $subclassOffsetError->getMessage(), '|'; }
echo $subclassOffset->asXML(), '|';
unset($subclassOffset, $subclassOffsetError);
restore_error_handler();
unset($updates, $existingUpdate, $attributeUpdate, $liveUpdate, $offsetUpdate, $hookUpdate, $badOffset, $appendedOffset, $offsetError);

