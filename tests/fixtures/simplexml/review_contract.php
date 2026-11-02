<?php
interface ReviewGuardedXmlView {}
class ReviewGuardedXml extends SimpleXMLElement implements ReviewGuardedXmlView {
    public string $label = 'before';
}
function reviewGuardedXmlWrites(ReviewGuardedXmlView $node): void {
    if ($node instanceof ReviewGuardedXml) {
        $node->label = 'after';
        $name = 'label'; $node->{$name} = 'dynamic';
        echo $node->label, '|';
        unset($node->label); var_dump(isset($node->label));
        $node->value = 'changed';
        echo $node->value, '|';
        $name = 'value'; unset($node->{$name});
        echo count($node->value), '|';
    }
}
class ReviewBitwiseXml extends SimpleXMLElement {}
function reviewBitwiseArguments(mixed $left, mixed $right): void { var_dump($left | $right, $left & $right); }
function reviewBitwiseNullable(?ReviewBitwiseXml $value): int { return $value | 4; }
class ReviewSaveXmlAlias extends SimpleXMLElement {
    public function asXML(?string $filename = null): string|bool { echo 'wrong-alias|'; return 'wrong'; }
}
// REVIEW_BODY

$contextNamespacesXml = new SimpleXMLElement('<r xmlns:p="urn:outer" a="1"><p:c>outer</p:c><!--r--><?r data?><q xmlns:p="urn:inner" b="2"><p:c>inner</p:c><!--q--><?q data?></q></r>');
foreach ([$contextNamespacesXml['a'], $contextNamespacesXml->xpath('/r/comment()')[0], $contextNamespacesXml->xpath('/r/processing-instruction()')[0], $contextNamespacesXml->q['b'], $contextNamespacesXml->xpath('/r/q/comment()')[0], $contextNamespacesXml->xpath('/r/q/processing-instruction()')[0]] as $contextNode) {
    $contextNode->registerXPathNamespace('p', 'urn:unused');
    $contextMatches = $contextNode->xpath('../p:c');
    echo count($contextMatches), ':', (string)$contextMatches[0], '|';
}
$emptyContextXml = new SimpleXMLElement('<r xmlns:p="urn:p"><p:c>yes</p:c></r>');
$emptyChildContext = $emptyContextXml->later;
$emptyAttributeContext = $emptyContextXml->attributes();
foreach ([$emptyChildContext, $emptyAttributeContext] as $emptyContext) {
    var_dump($emptyContext->registerXPathNamespace('q', 'urn:p'));
    var_dump($emptyContext->registerXPathNamespace('', 'urn:p'));
    var_dump($emptyContext->registerXPathNamespace('q', ''));
    var_dump($emptyContext->registerXPathNamespace('q', 'urn:p'));
}
$emptyContextXml->later = '';
$newContextMatches = $emptyChildContext->xpath('../q:c');
echo count($newContextMatches), ':', (string)$newContextMatches[0], '|';
$bitwiseXml = new ReviewBitwiseXml('<r><a>3</a><b>12</b><c>15</c><d>1</d><e>31</e></r>');
$bitwiseXml->a |= 4;
$bitwiseXml->b >>= 2;
$bitwiseXml->c &= 6;
$bitwiseXml->d <<= 3;
$bitwiseXml->e ^= 7;
echo $bitwiseXml->a, ':', $bitwiseXml->b, ':', $bitwiseXml->c, ':', $bitwiseXml->d, ':', $bitwiseXml->e, '|';
var_dump($bitwiseXml->a & 6, 4 | $bitwiseXml->b, $bitwiseXml->d ^ $bitwiseXml->e, $bitwiseXml->a << 1, $bitwiseXml->d >> 2);
reviewBitwiseArguments($bitwiseXml->a, $bitwiseXml->b);
$bitwiseXml['mask'] = '3';
$bitwiseXml['mask'] |= 4;
echo $bitwiseXml['mask'], '|';
$rootBits = new ReviewBitwiseXml('<r>3</r>');
var_dump($rootBits | 4, (new SimpleXMLIterator('<r>12</r>')) >> 2, reviewBitwiseNullable($rootBits), reviewBitwiseNullable(null));
$latinByte = chr($argc > 0 ? 233 : 234);
$binaryXml = new SimpleXMLElement('<r/>');
$binaryXml->a = 'caf' . $latinByte;
$binaryKey = 'k' . chr($argc > 0 ? 255 : 254);
$binaryXml[$binaryKey] = chr($argc > 0 ? 255 : 254) . chr($argc > 0 ? 254 : 253);
$binaryChild = $binaryXml->addChild('b' . $latinByte, 'x' . $latinByte);
$binaryNsChild = $binaryXml->addChild('p:child', 'ns', 'urn:' . $latinByte);
echo bin2hex((string)$binaryXml->a), ':', bin2hex((string)$binaryXml[$binaryKey]), ':', bin2hex((string)$binaryChild), ':', bin2hex($binaryChild->getName()), '|';
$binaryNamespaces = $binaryXml->getDocNamespaces(true);
echo bin2hex($binaryNamespaces['p']), '|';
$saveXml = new SimpleXMLElement('<r/>');
try { $saveXml->asXML("simplexml-review-nul-2060.xml\0.txt"); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
try { $saveXml->saveXML("simplexml-review-nul-2060.xml\0.txt"); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
try { $saveXml->asXML(''); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
try { $saveXml->saveXML(''); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
var_dump(file_exists('simplexml-review-nul-2060.xml'));
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
var_dump($saveXml->asXML('/__simplexml_review_absent_2060__/out.xml'));
var_dump($saveXml->saveXML('/__simplexml_review_absent_2060__/out.xml'));
var_dump($saveXml->asXML('.'));
var_dump($saveXml->saveXML('.'));
var_dump($saveXml->asXML('/__simplexml_review_absent_2060__/out.xml'));
restore_error_handler();
$saveAliasXml = new ReviewSaveXmlAlias('<r/>');
echo (int)str_contains((string)$saveAliasXml->saveXML(), '<r/>'), '|';
reviewGuardedXmlWrites(new ReviewGuardedXml('<r><value>old</value></r>'));

