package com.qtech.ruby.loader.api;

import java.util.List;

public record RubyModuleMetadata(
    String id,
    String name,
    String version,
    List<String> platforms,
    String description
) {
    public RubyModuleMetadata {
        platforms = platforms == null ? List.of() : List.copyOf(platforms);
        description = description == null ? "" : description;
    }
}
