<?php
function globalBoundarySourceOpen(): mixed { return 1; } $globalBoundaryOpen = globalBoundarySourceOpen(); $globalBoundaryOpen = 'text';
function changeGlobalBoundaryOpen(mixed $next): void {
 global $globalBoundaryOpen;
 try { $globalBoundaryOpen = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
 echo gettype($globalBoundaryOpen), ':', $globalBoundaryOpen, '|';
}
changeGlobalBoundaryOpen(2.5);changeGlobalBoundaryOpen(3);changeGlobalBoundaryOpen('final');
function globalBoundarySourceUnion(): int|string { return 1; } $globalBoundaryUnion = globalBoundarySourceUnion(); $globalBoundaryUnion = 2;
function changeGlobalBoundaryUnion(mixed $next): void {
 global $globalBoundaryUnion;
 try { $globalBoundaryUnion = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
 echo gettype($globalBoundaryUnion), ':', $globalBoundaryUnion, '|';
}
changeGlobalBoundaryUnion('next');changeGlobalBoundaryUnion(3);changeGlobalBoundaryUnion(1.5);
function globalBoundarySourceNullable(): ?int { return 1; } $globalBoundaryNullable = globalBoundarySourceNullable(); $globalBoundaryNullable = 2;
function changeGlobalBoundaryNullable(mixed $next): void {
 global $globalBoundaryNullable;
 try { $globalBoundaryNullable = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
 echo gettype($globalBoundaryNullable), ':', $globalBoundaryNullable, '|';
}
changeGlobalBoundaryNullable(null);changeGlobalBoundaryNullable(1.5);changeGlobalBoundaryNullable(4);
$globalBoundaryInteger = 1;
function changeGlobalBoundaryInteger(mixed $next): void {
 global $globalBoundaryInteger;
 try { $globalBoundaryInteger = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
 echo gettype($globalBoundaryInteger), ':', $globalBoundaryInteger, '|';
}
changeGlobalBoundaryInteger(2.5);changeGlobalBoundaryInteger('bad');changeGlobalBoundaryInteger(2);
