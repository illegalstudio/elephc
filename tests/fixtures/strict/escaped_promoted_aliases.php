<?php
class EscapedPromotedAlias { public function __construct(public mixed &$value) {} }
function escapedScalarPair(): array {
    $source = 3;
    return [new EscapedPromotedAlias($source), new EscapedPromotedAlias($source)];
}
function escapedArrayPair(): array {
    $source = ['k' => 4];
    return [new EscapedPromotedAlias($source['k']), new EscapedPromotedAlias($source['k'])];
}
function escapedNestedPair(): array {
    $source = [['k' => 5]];
    return [new EscapedPromotedAlias($source[0]['k']), new EscapedPromotedAlias($source[0]['k'])];
}
function observeEscapedPair(array $pair, int $next): void {
    echo $pair[0]->value, '/'; $pair[0]->value = $next;
    echo $pair[1]->value, '/'; $pair[1]->value = $next + 4;
    echo $pair[0]->value, '|';
}
function escapedPromotedObservations(): void {
    observeEscapedPair(escapedScalarPair(), 7);
    observeEscapedPair(escapedArrayPair(), 8);
    observeEscapedPair(escapedNestedPair(), 9);
}
