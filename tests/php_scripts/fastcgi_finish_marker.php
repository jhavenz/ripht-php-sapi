<?php

echo json_encode([
    'pre' => true,
]);

$finished = fastcgi_finish_request();

file_put_contents(getenv('RIPHT_FASTCGI_MARKER_PATH'), json_encode([
    'finished' => $finished,
    'marker' => 'after-finish',
]));
