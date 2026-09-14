<?php
$split = $argc > 0 ? "mb_split" : "mb_ereg_match";
try { $split(); } catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
try { $split(","); } catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
try { $split(",", "a,b", 2, 3); } catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
try { $split([], "a,b"); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
try { $split(",", []); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
try { $split(",", "a,b", []); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
var_dump($split("[", chr(255)));
var_dump($split("[", "ab", 1));
var_dump($split("$", "ab"));
var_dump($split(",", "a,b,c", 2.5));
var_dump($split(",", "a,b,c", null));
