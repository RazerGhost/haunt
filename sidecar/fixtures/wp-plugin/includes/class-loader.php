<?php

class My_Loader
{
    public function run()
    {
        add_action('init', [$this, 'register']);
        my_plugin_log('run');
    }

    public function register()
    {
        return my_plugin_log('register');
    }
}
