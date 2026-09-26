<?php declare(strict_types=1);
$decode = $argc > 0 ? 'mb_decode_mimeheader' : 'mb_strlen';
try { echo $decode(123); } catch (TypeError $error) { echo $error->getMessage(); }
