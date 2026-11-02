<?php

// REVIEW_BODY

$xml = new SimpleXMLElement('<r xmlns:p="urn:p" at="base"/>');
$xml->{'p:q'} = 'value';
$xml['p:at'] = 'literal';
$xml->{'a:b'}->c = 'nested';
$xml->{'p:q'}[] = 'second';
echo $xml->asXML();
echo (int)isset($xml->{'p:q'}), ':', (string)$xml->{'p:q'}[0], ':', (string)$xml->{'p:q'}[1], ':', count($xml->children('urn:p')), '|';
echo (string)$xml['at'], ':', (string)$xml['p:at'], ':', $xml->{'a:b'}->getName(), '|';
$added = $xml->addChild('p:method', 'qualified', 'urn:p');
echo $added->getName(), ':', (string)$xml->children('urn:p')->method, '|';
$leading = $xml->addChild(':a', 'leading');
echo $leading->getName(), '|';
set_error_handler(function(int $level, string $message): bool { echo $message, '|'; return true; });
$xml->addAttribute(':b', '1', 'urn:q');
$xml->addAttribute(':c', '1');
$xml->addAttribute(':c', '2');
restore_error_handler();
echo $xml->asXML();
unset($added, $leading, $xml);
