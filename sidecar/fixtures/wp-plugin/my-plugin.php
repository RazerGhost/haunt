<?php
/**
 * Plugin Name: My Plugin
 */

require_once __DIR__ . '/includes/class-loader.php';
require_once plugin_dir_path(__FILE__) . 'includes/helpers.php';

function my_plugin_boot()
{
    $loader = new My_Loader();
    $loader->run();
}

add_action('plugins_loaded', 'my_plugin_boot');
