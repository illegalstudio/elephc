<?php
function evalWithFreshBinding(): void {
    try { eval('$value = 1; $value = "changed"; echo $value;'); echo 'W|'; }
    catch (TypeError $error) { echo 'T|'; }
}
function evalWithOpenBinding(): void {
    mixed $value = null;
    eval('$value = 1; $value = "changed"; echo $value;'); echo 'O|';
}
evalWithFreshBinding(); evalWithOpenBinding();

function evalFromOpenProducer(mixed $input): void {
    eval('$value = $input; $value = "changed"; echo $value;'); echo 'M|';
}
function evalFreshNull(): void {
    try { eval('$nil = null; $nil = 1;'); echo 'W|'; }
    catch (TypeError $error) { echo 'N|'; }
}
function evalFreshArray(): void {
    try { eval('$array = [1]; $array = ["ok"]; echo $array[0]; $array = 3;'); echo 'W|'; }
    catch (TypeError $error) { echo 'A|'; }
}
evalFromOpenProducer(1); evalFreshNull(); evalFreshArray(); evalWithFreshBinding();
