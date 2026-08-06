package com.qtech.ruby.loader.api;

import java.util.List;

public interface RubyModule {
    RubyModuleMetadata metadata();
    void initialize(RubyModuleContext context) throws Exception;

    /**
     * Optional settings pages owned by this module. Values remain inside the
     * module; Ruby Module Loader only renders and edits them.
     */
    default List<RubySettingsPage> settingsPages() { return List.of(); }

    default void shutdown() throws Exception {}
}
