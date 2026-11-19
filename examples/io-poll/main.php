<?php
// Io\Poll is PHP 8.6 syntax: compile with --php-version=8.6.
[$reader, $writer] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);

$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$watcher = $context->add(
    new StreamPollHandle($reader),
    [Io\Poll\Event::Read, Io\Poll\Event::OneShot],
    'reader',
);

fwrite($writer, "hello from the other side\n");

foreach ($context->wait() as $event) {
    if ($event->hasTriggered(Io\Poll\Event::Read)) {
        echo $event->getData(), ': ', fread($event->getHandle()->getStream(), 1024);
    }
}
