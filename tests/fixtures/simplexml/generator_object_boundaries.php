<?php
function generatorBoundaryXml(SimpleXMLElement $node): Generator { echo 'generator_body|'; yield 1; }
function variadicGeneratorBoundaryXml(SimpleXMLElement ...$nodes): Generator { echo 'variadic_generator_body|'; yield 2; }
function iterableGeneratorBoundaryXml(iterable $nodes): Generator { echo 'iterable_generator_body|'; yield 3; }
function stateGeneratorBoundaryXml(SimpleXMLElement $node): Generator { yield 1; }
class BoundaryXmlGeneratorMethods {
    public function generate(SimpleXMLElement $node): Generator { yield 4; }
}
class BoundaryXmlGeneratorElement extends BoundaryXmlElement {
    public function relativeGenerator(self $node): Generator { yield 6; }
}
function stateProbeBoundaryXml(SimpleXMLElement $root): void {
    $missing = $root->missing->child;
    $state = 0;
    try { $state = 1; stateGeneratorBoundaryXml($missing); $state = 2; }
    catch (TypeError $error) { echo $state, "\n"; }
}
$generatorRoot = new BoundaryXmlGeneratorElement('<r/>');
$generatorMissing = $generatorRoot->missing->child;
$generatorMethods = new BoundaryXmlGeneratorMethods();
try { $generator = generatorBoundaryXml($generatorMissing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $generator = variadicGeneratorBoundaryXml($generatorRoot, $generatorMissing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $generator = iterableGeneratorBoundaryXml($generatorMissing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $generator = $generatorMethods->generate($generatorMissing); } catch (TypeError $error) { reportBoundaryXml($error); }
try { $generator = $generatorRoot->relativeGenerator($generatorMissing); } catch (TypeError $error) { reportBoundaryXml($error); }
$generatorClosure = function (SimpleXMLElement $node): Generator { yield 5; };
try { $generator = $generatorClosure($generatorMissing); } catch (TypeError $error) { reportBoundaryXml($error); }
stateProbeBoundaryXml($generatorRoot);
$generator = generatorBoundaryXml($generatorRoot);
echo 'generator_created|';
foreach ($generator as $number) { echo $number, '|'; }
echo "generator_done\n";
