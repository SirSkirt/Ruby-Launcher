package com.qtech.ruby.loader;

import com.qtech.ruby.loader.api.RubyMappings;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Parses Mojang's client ProGuard mappings and exposes the official runtime
 * names used by an unmodified Minecraft client. Descriptors are retained so a
 * mod-loader mapping service can translate the same members into its runtime
 * namespace (Fabric's intermediary namespace, for example).
 */
final class MappingIndex implements RubyMappings {
    private final Map<String, String> classes = new HashMap<>();
    private final Map<String, Map<String, FieldName>> fields = new HashMap<>();
    private final Map<String, List<MethodName>> methods = new HashMap<>();

    static MappingIndex load(Path path) throws IOException {
        MappingIndex index = new MappingIndex();
        List<String> lines = Files.readAllLines(path);

        // Build the complete class table first. Member descriptors can mention
        // classes that appear later in the mappings file.
        for (String line : lines) {
            if (line.isBlank() || line.startsWith("#") || Character.isWhitespace(line.charAt(0))) continue;
            int arrow = line.lastIndexOf(" -> ");
            if (arrow < 0 || !line.endsWith(":")) continue;
            String named = line.substring(0, arrow).trim();
            String official = line.substring(arrow + 4, line.length() - 1).trim();
            index.classes.put(named, official);
        }

        String currentClass = null;
        for (String line : lines) {
            if (line.isBlank() || line.startsWith("#")) continue;
            if (!Character.isWhitespace(line.charAt(0))) {
                int arrow = line.lastIndexOf(" -> ");
                currentClass = arrow >= 0 && line.endsWith(":")
                    ? line.substring(0, arrow).trim()
                    : null;
                continue;
            }
            if (currentClass == null) continue;

            String value = line.trim();
            int arrow = value.lastIndexOf(" -> ");
            if (arrow < 0) continue;
            String left = stripLineNumbers(value.substring(0, arrow).trim());
            String officialName = value.substring(arrow + 4).trim();
            int open = left.indexOf('(');
            if (open >= 0) {
                int close = left.indexOf(')', open);
                if (close < 0) continue;
                String before = left.substring(0, open).trim();
                int separator = before.lastIndexOf(' ');
                if (separator < 0) continue;
                String returnType = before.substring(0, separator).trim();
                String named = before.substring(separator + 1).trim();
                String arguments = left.substring(open + 1, close).trim();
                List<String> argumentTypes = splitArguments(arguments);
                String descriptor = index.methodDescriptor(returnType, argumentTypes);
                index.methods.computeIfAbsent(currentClass, ignored -> new ArrayList<>())
                    .add(new MethodName(named, officialName, argumentTypes.size(), descriptor));
            } else {
                int separator = left.lastIndexOf(' ');
                if (separator < 0) continue;
                String type = left.substring(0, separator).trim();
                String named = left.substring(separator + 1).trim();
                index.fields.computeIfAbsent(currentClass, ignored -> new HashMap<>())
                    .put(named, new FieldName(officialName, index.typeDescriptor(type)));
            }
        }

        if (index.classes.isEmpty()) {
            throw new IOException("The mappings file contained no class mappings.");
        }
        return index;
    }

    private static String stripLineNumbers(String value) {
        String stripped = value.replaceFirst("^\\d+:\\d+:", "");
        return stripped.replaceFirst(":\\d+:\\d+$", "");
    }

    private static List<String> splitArguments(String arguments) {
        if (arguments.isBlank()) return List.of();
        List<String> result = new ArrayList<>();
        for (String value : arguments.split(",", -1)) {
            String type = value.trim();
            if (!type.isEmpty()) result.add(type);
        }
        return result;
    }

    private String methodDescriptor(String returnType, List<String> arguments) {
        StringBuilder descriptor = new StringBuilder("(");
        for (String argument : arguments) descriptor.append(typeDescriptor(argument));
        return descriptor.append(')').append(typeDescriptor(returnType)).toString();
    }

    private String typeDescriptor(String rawType) {
        String type = rawType.trim();
        int dimensions = 0;
        while (type.endsWith("[]")) {
            dimensions++;
            type = type.substring(0, type.length() - 2);
        }

        String base = switch (type) {
            case "void" -> "V";
            case "boolean" -> "Z";
            case "byte" -> "B";
            case "char" -> "C";
            case "short" -> "S";
            case "int" -> "I";
            case "long" -> "J";
            case "float" -> "F";
            case "double" -> "D";
            default -> "L" + classes.getOrDefault(type, type).replace('.', '/') + ";";
        };
        return "[".repeat(dimensions) + base;
    }

    @Override
    public String className(String named) {
        return classes.getOrDefault(named, named);
    }

    @Override
    public String fieldName(String owner, String named) {
        FieldName field = fields.getOrDefault(owner, Map.of()).get(named);
        return field == null ? named : field.official();
    }

    @Override
    public String methodName(String owner, String named, int arity) {
        MethodName method = method(owner, named, arity);
        return method == null ? named : method.official();
    }

    String fieldDescriptor(String owner, String named) {
        FieldName field = fields.getOrDefault(owner, Map.of()).get(named);
        return field == null ? null : field.descriptor();
    }

    String methodDescriptor(String owner, String named, int arity) {
        MethodName method = method(owner, named, arity);
        return method == null ? null : method.descriptor();
    }

    private MethodName method(String owner, String named, int arity) {
        for (MethodName method : methods.getOrDefault(owner, List.of())) {
            if (method.named().equals(named) && method.arity() == arity) return method;
        }
        return null;
    }

    private record FieldName(String official, String descriptor) {}
    private record MethodName(String named, String official, int arity, String descriptor) {}
}
