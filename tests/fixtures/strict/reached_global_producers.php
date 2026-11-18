<?php
$reachedText = 'before';
function replaceTextGlobal(int $reachedText): void {
    global $reachedText;
    $reachedText = 'after'; echo gettype($reachedText), ':', $reachedText, '|';
}
replaceTextGlobal(1); echo $reachedText, '|';
$reachedReference = 'before';
function replaceReferenceGlobal(int &$reachedReference): void {
    global $reachedReference;
    $reachedReference = 'after'; echo gettype($reachedReference), ':', $reachedReference, '|';
}
$localReference = 1;
replaceReferenceGlobal($localReference); echo $localReference, ':', $reachedReference, '|';
$reachedLocal = 'before';
function replaceOrdinaryGlobal(): void {
    $reachedLocal = 1;
    global $reachedLocal;
    $reachedLocal = 'after'; echo gettype($reachedLocal), ':', $reachedLocal, '|';
}
replaceOrdinaryGlobal(); echo $reachedLocal, '|';
$reachedStatic = 'before';
function replaceStaticGlobal(): void {
    static $reachedStatic = 1;
    global $reachedStatic;
    $reachedStatic = 'after'; echo gettype($reachedStatic), ':', $reachedStatic, '|';
}
replaceStaticGlobal(); replaceStaticGlobal(); echo $reachedStatic, '|';

function reachedOpenProducer(): mixed { return 1; }
$reachedOpen = reachedOpenProducer();
function replaceOpenGlobalProducer(int $reachedOpen, mixed $next): void {
    global $reachedOpen;
    $reachedOpen = $next; echo gettype($reachedOpen), ':', $reachedOpen, '|';
}
replaceOpenGlobalProducer(1, 2.5); replaceOpenGlobalProducer(1, 'ready');
