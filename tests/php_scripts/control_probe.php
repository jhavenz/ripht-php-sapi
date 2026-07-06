<?php

register_shutdown_function(static function () {
    file_put_contents(getenv('RIPHT_CONTROL_SHUTDOWN_PATH'), 'shutdown');
});

header('Content-Type: text/plain');

echo 'alpha';
flush();
echo 'omega';
