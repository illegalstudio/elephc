<?php
// getprotobyname() and getprotobynumber() look up entries in the system
// protocols database (/etc/protocols) by name or by number.

echo "tcp  = " . getprotobyname("tcp") . "\n";
echo "udp  = " . getprotobyname("udp") . "\n";
echo "icmp = " . getprotobyname("icmp") . "\n";

echo "6  -> " . getprotobynumber(6) . "\n";
echo "17 -> " . getprotobynumber(17) . "\n";

if (getprotobyname("no_such_protocol") === false) {
    echo "unknown protocol name -> false\n";
}

if (getprotobynumber(999) === false) {
    echo "unknown protocol number -> false\n";
}
