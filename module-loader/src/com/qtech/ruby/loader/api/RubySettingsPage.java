package com.qtech.ruby.loader.api;

import java.util.List;

/** A controller-friendly settings page supplied by a Ruby module. */
public record RubySettingsPage(
    String id,
    RubySettingsLocation location,
    String buttonLabel,
    String title,
    String description,
    List<RubySetting> settings
) {
    public RubySettingsPage {
        id = id == null ? "settings" : id;
        location = location == null ? RubySettingsLocation.CONTROLS : location;
        buttonLabel = buttonLabel == null ? "Module Options..." : buttonLabel;
        title = title == null ? "Module Options" : title;
        description = description == null ? "" : description;
        settings = settings == null ? List.of() : List.copyOf(settings);
    }
}
