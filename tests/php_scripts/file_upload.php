<?php
/**
 * File upload test script.
 * 
 * Tests $_FILES superglobal population for multipart/form-data uploads.
 */

header('Content-Type: application/json');

$response = [
    'method' => $_SERVER['REQUEST_METHOD'] ?? 'UNKNOWN',
    'content_type' => $_SERVER['CONTENT_TYPE'] ?? null,
    'files' => $_FILES,
    'post_data' => $_POST,
    'file_count' => count($_FILES),
    'upload_tmp_dir' => ini_get('upload_tmp_dir') ?: sys_get_temp_dir(),
];

if (!empty($_FILES)) {
    foreach ($_FILES as $key => $file) {
        $response['files'][$key]['uploaded'] = is_uploaded_file($file['tmp_name']);
        
        if (isset($file['tmp_name']) && !empty($file['tmp_name'])) {
            $tmp_name = $file['tmp_name'];
            $response['files'][$key]['tmp_exists'] = file_exists($tmp_name);
            $response['files'][$key]['tmp_readable'] = is_readable($tmp_name);
            
            if ($response['files'][$key]['tmp_exists'] && $response['files'][$key]['tmp_readable']) {
                $content = @file_get_contents($tmp_name);
                if ($content !== false) {
                    $response['files'][$key]['tmp_content'] = $content;
                    $response['files'][$key]['tmp_content_length'] = strlen($content);
                } else {
                    $response['files'][$key]['tmp_content'] = null;
                    $response['files'][$key]['tmp_content_length'] = 0;
                }
            } else {
                $response['files'][$key]['tmp_content'] = null;
                $response['files'][$key]['tmp_content_length'] = 0;
            }
        } else {
            $response['files'][$key]['tmp_exists'] = false;
            $response['files'][$key]['tmp_readable'] = false;
            $response['files'][$key]['tmp_content'] = null;
            $response['files'][$key]['tmp_content_length'] = 0;
        }
    }
}

echo json_encode($response, JSON_PRETTY_PRINT);

