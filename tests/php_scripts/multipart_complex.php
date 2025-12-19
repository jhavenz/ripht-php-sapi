<?php

header('Content-Type: application/json');

$result = [
    'method' => $_SERVER['REQUEST_METHOD'],
    'content_type' => $_SERVER['CONTENT_TYPE'] ?? null,
    'content_length' => $_SERVER['CONTENT_LENGTH'] ?? null,
    'files' => [],
    'post_fields' => [],
];

foreach ($_FILES as $field_name => $file_info) {
    if (is_array($file_info['name'])) {
        for ($i = 0; $i < count($file_info['name']); $i++) {
            $result['files'][] = [
                'field' => $field_name . '[' . $i . ']',
                'name' => $file_info['name'][$i],
                'type' => $file_info['type'][$i],
                'size' => $file_info['size'][$i],
                'error' => $file_info['error'][$i],
                'error_text' => match($file_info['error'][$i]) {
                    UPLOAD_ERR_OK => 'OK',
                    UPLOAD_ERR_INI_SIZE => 'INI_SIZE',
                    UPLOAD_ERR_FORM_SIZE => 'FORM_SIZE',
                    UPLOAD_ERR_PARTIAL => 'PARTIAL',
                    UPLOAD_ERR_NO_FILE => 'NO_FILE',
                    default => 'UNKNOWN'
                },
                'tmp_exists' => file_exists($file_info['tmp_name'][$i]),
            ];
        }
    } else {
        $result['files'][] = [
            'field' => $field_name,
            'name' => $file_info['name'],
            'type' => $file_info['type'],
            'size' => $file_info['size'],
            'error' => $file_info['error'],
            'error_text' => match($file_info['error']) {
                UPLOAD_ERR_OK => 'OK',
                UPLOAD_ERR_INI_SIZE => 'INI_SIZE',
                UPLOAD_ERR_FORM_SIZE => 'FORM_SIZE',
                UPLOAD_ERR_PARTIAL => 'PARTIAL',
                UPLOAD_ERR_NO_FILE => 'NO_FILE',
                default => 'UNKNOWN'
            },
            'tmp_exists' => file_exists($file_info['tmp_name']),
            'content_preview' => $file_info['error'] === UPLOAD_ERR_OK && $file_info['size'] < 1024
                ? substr(file_get_contents($file_info['tmp_name']), 0, 100)
                : null,
        ];
    }
}

foreach ($_POST as $key => $value) {
    $result['post_fields'][$key] = is_array($value) ? $value : substr($value, 0, 100);
}

echo json_encode($result, JSON_PRETTY_PRINT);
