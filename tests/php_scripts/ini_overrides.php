<?php

header('Content-Type: application/json');

echo json_encode([
    'display_errors' => ini_get('display_errors'),
], JSON_PRETTY_PRINT);
