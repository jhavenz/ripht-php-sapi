<?php

header('X-Ripht-Before-Finish: yes');

echo json_encode([
    'before' => true,
]);

$finished = fastcgi_finish_request();

header('X-Ripht-After-Finish: no');
echo json_encode([
    'after' => true,
]);

file_put_contents(getenv('RIPHT_FASTCGI_LATE_OUTPUT_PATH'), json_encode([
    'finished' => $finished,
    'marker' => 'late-output-complete',
]));

