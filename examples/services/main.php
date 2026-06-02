<?php
// getservbyname() and getservbyport() look up entries in the system
// services database (/etc/services) by name or by port number.

echo "http   (tcp) = " . getservbyname("http", "tcp") . "\n";
echo "https  (tcp) = " . getservbyname("https", "tcp") . "\n";
echo "ssh    (tcp) = " . getservbyname("ssh", "tcp") . "\n";
echo "domain (udp) = " . getservbyname("domain", "udp") . "\n";

echo "80  (tcp) -> " . getservbyport(80, "tcp") . "\n";
echo "443 (tcp) -> " . getservbyport(443, "tcp") . "\n";

if (getservbyname("no_such_service", "tcp") === false) {
    echo "unknown service name -> false\n";
}

if (getservbyport(80, "no_such_proto") === false) {
    echo "unknown service protocol -> false\n";
}
