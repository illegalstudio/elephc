<?php
function structuralCallback(int $n): int {return $n+10;}
function structuralOwnedString(string $text): mixed {echo 'O';return $text.'!';}
function structuralSequence(): iterable {yield 4;}
function structuralAliasWrite(mixed &$value,mixed $next): void {$value=$next;}
function structuralLocalCallable(mixed $next,bool $inspect): void {
 $value=static fn(int $n): int=>$n+1;
 try {$value=$next; echo 'W';} catch(TypeError $error) {echo 'T',$value(1);}
 if($inspect) {echo $value(1);} echo '|';
}
function structuralLocalCallableOwned(): void {
 $value=static fn(int $n): int=>$n+1;
 try {$value=structuralOwnedString('missing');echo 'W';} catch(TypeError $error) {echo 'T',$value(1);}
 echo '|';
}
function structuralLocalIterable(iterable $value,mixed $next,bool $inspect): void {
 try {$value=$next;echo 'W';} catch(TypeError $error) {echo 'T';foreach($value as $item){echo $item;}}
 if($inspect) {foreach($value as $item){echo $item;}} echo '|';
}
function structuralCallableAlias(mixed $next,bool $inspect): void {
 $value=static fn(int $n): int=>$n+1;
 try {structuralAliasWrite($value,$next);echo 'W';} catch(TypeError $error) {echo 'T',$value(1);}
 if($inspect) {echo $value(1);} echo '|';
}
function structuralIterableAlias(iterable $value,mixed $next,bool $inspect): void {
 try {structuralAliasWrite($value,$next);echo 'W';} catch(TypeError $error) {echo 'T';foreach($value as $item){echo $item;}}
 if($inspect) {foreach($value as $item){echo $item;}} echo '|';
}
structuralLocalCallable(1,false);structuralLocalCallable('missing_callback',false);
structuralLocalCallable('structuralCallback',true);structuralLocalCallableOwned();
structuralLocalIterable([1],1,false);structuralLocalIterable([1],[2,3],true);
structuralLocalIterable([1],structuralSequence(),true);
structuralCallableAlias(1,false);structuralCallableAlias('missing_callback',false);
structuralCallableAlias('structuralCallback',true);
structuralIterableAlias([1],1,false);structuralIterableAlias([1],[2],true);
class StructuralHandler {
 public static function apply(int $n): int {return $n+30;}
 public static function check(mixed $next): void {
  $value=static fn(int $n): int=>$n+1;
  try {$value=$next;echo 'W';}catch(TypeError $error){echo 'T',$value(1);}echo '|';
 }
}
class StructuralInvoker {public function __invoke(int $n): int {return $n+40;}}
trait StructuralGuardTrait {
 public function checkTrait(mixed $next): void {
  $value=static fn(int $n): int=>$n+1;
  try {$value=$next;echo 'W';}catch(TypeError $error){echo 'T',$value(1);}echo '|';
 }
}
class StructuralTraitUser {use StructuralGuardTrait;}
function structuralCallableExpression(mixed $next,bool $inspect): void {
 $value=static fn(int $n): int=>$n+1;
 try {$observed=($value=$next);echo 'W';if($inspect){echo $observed(1);}}
 catch(TypeError $error){echo 'T',$value(1);}echo '|';
}
structuralLocalCallable('STRUCTURALCALLBACK',true);
structuralLocalCallable(['StructuralHandler','apply'],true);
structuralLocalCallable(['StructuralHandler','missing'],false);
structuralLocalCallable(new StructuralInvoker(),true);
structuralLocalCallable(new stdClass(),false);
structuralCallableExpression(1,false);structuralCallableExpression('structuralCallback',true);
structuralIterableAlias([1],structuralSequence(),true);
StructuralHandler::check(1);$traitUser=new StructuralTraitUser();$traitUser->checkTrait(1);
structuralCallableAlias(new StructuralInvoker(),true);
gc_collect_cycles();
