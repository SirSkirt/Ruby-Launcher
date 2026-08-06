package com.qtech.ruby.loader.api;

/**
 * One human-readable module setting shown by Ruby Module Loader.
 *
 * Implementations own the value, persistence and live apply behavior. The
 * loader only asks for text and requests the previous/next choice.
 */
public interface RubySetting {
    String id();
    String name();
    String description();
    String valueLabel();

    /** Whether Minecraft should allow this setting to be changed. */
    default boolean active() { return true; }

    /** direction is normally -1 for previous or +1 for next. */
    void cycle(int direction) throws Exception;
}
