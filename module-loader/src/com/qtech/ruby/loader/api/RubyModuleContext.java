package com.qtech.ruby.loader.api;

import java.lang.instrument.Instrumentation;
import java.nio.file.Path;
import java.util.Objects;
import java.util.function.Consumer;

public final class RubyModuleContext {
    private final Instrumentation instrumentation;
    private final RubyMappings mappings;
    private final String minecraftVersion;
    private final Path gameDirectory;
    private final Consumer<String> logger;

    public RubyModuleContext(
        Instrumentation instrumentation,
        RubyMappings mappings,
        String minecraftVersion,
        Path gameDirectory,
        Consumer<String> logger
    ) {
        this.instrumentation = Objects.requireNonNull(instrumentation);
        this.mappings = Objects.requireNonNull(mappings);
        this.minecraftVersion = Objects.requireNonNullElse(minecraftVersion, "unknown");
        this.gameDirectory = gameDirectory;
        this.logger = Objects.requireNonNull(logger);
    }

    public Instrumentation instrumentation() { return instrumentation; }
    public RubyMappings mappings() { return mappings; }
    public String minecraftVersion() { return minecraftVersion; }
    public Path gameDirectory() { return gameDirectory; }
    public void log(String moduleId, String message) {
        logger.accept("[" + moduleId + "] " + message);
    }
}
