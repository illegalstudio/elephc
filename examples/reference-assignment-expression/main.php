<?php

class ReferenceExpressionCache
{
    private static array $cache = [];
    private static int $level = 0;

    public static function read(string $key): void
    {
        if (null !== $value = &self::$cache[$key]) {
            echo $value;

            return;
        }

        $value = 7;
    }

    public static function enter(): void
    {
        if (!self::$level++) {
            echo 'P';
        }
    }
}

ReferenceExpressionCache::read('key');
ReferenceExpressionCache::read('key');
ReferenceExpressionCache::enter();
ReferenceExpressionCache::enter();

class DynamicReferencePasses
{
    private array $beforePasses = [[1]];
    private array $afterPasses = [[3]];

    public function append(string $type, int $value): void
    {
        $property = $type.'Passes';
        $passes = &$this->$property;
        $passes[0][] = $value;
    }

    public function printBefore(): void
    {
        echo $this->beforePasses[0][0], $this->beforePasses[0][1];
    }
}

$passes = new DynamicReferencePasses();
$passes->append('before', 2);
$passes->printBefore();

function appendEscapingReference(array &$loops): void
{
    $path = [1];
    $loops[0][] = &$path;
    $path[] = 2;
}

function printEscapingReference(mixed $loops): void
{
    echo $loops[0][0][0], $loops[0][0][1];
}

$loops = [[]];
appendEscapingReference($loops);
printEscapingReference($loops);

$referencedValues = [1];
$referencedStub = 5;
$referencedValues[0] = &$referencedStub;
$referencedStub = 7;
echo $referencedValues[0];

// Later loop iterations mutate the same nested path shared by both result entries.
function appendMutablePaths(array &$paths): void
{
    $path = ['root', []];
    foreach (['left', 'right'] as $key) {
        $path[1][$key] = true;
        $paths[$key][] = &$path;
    }
}

$sharedPaths = [];
appendMutablePaths($sharedPaths);
echo "\n", json_encode($sharedPaths), "\n";
class SharedPropertyReferences
{
    public array $refs = [];

    public function bind(SharedPropertyReferences $source): void
    {
        $this->refs = &$source->refs;
    }
}

$sourceReferences = new SharedPropertyReferences();
$targetReferences = new SharedPropertyReferences();
$targetReferences->bind($sourceReferences);
$sourceReferences->refs[] = 'S';
$targetReferences->refs[] = 'T';
echo $targetReferences->refs[0], $sourceReferences->refs[1];
$referencedValues[0] = 8;
echo $referencedStub;
unset($referencedStub);
$referencedValues[0] = 9;
echo $referencedValues[0];
