package com.qtech.ruby.loader;

import com.qtech.ruby.loader.api.RubyMappings;
import java.lang.reflect.Method;
import java.util.Collection;

/** Maps Mojang official names into Fabric Loader's active runtime namespace. */
final class FabricRuntimeMappings implements RubyMappings {
    private static final String SOURCE_NAMESPACE = "official";

    private final MappingIndex officialMappings;
    private final Object resolver;
    private final Method mapClassName;
    private final Method mapFieldName;
    private final Method mapMethodName;
    private final String runtimeNamespace;

    private FabricRuntimeMappings(
        MappingIndex officialMappings,
        Object resolver,
        Method mapClassName,
        Method mapFieldName,
        Method mapMethodName,
        String runtimeNamespace
    ) {
        this.officialMappings = officialMappings;
        this.resolver = resolver;
        this.mapClassName = mapClassName;
        this.mapFieldName = mapFieldName;
        this.mapMethodName = mapMethodName;
        this.runtimeNamespace = runtimeNamespace;
    }

    static FabricRuntimeMappings tryCreate(MappingIndex officialMappings) throws Exception {
        Class<?> fabricLoaderClass = Class.forName("net.fabricmc.loader.api.FabricLoader", false, ClassLoader.getSystemClassLoader());
        Object fabricLoader = fabricLoaderClass.getMethod("getInstance").invoke(null);
        Object resolver = fabricLoaderClass.getMethod("getMappingResolver").invoke(fabricLoader);
        if (resolver == null) return null;

        Class<?> resolverClass = Class.forName(
            "net.fabricmc.loader.api.MappingResolver", false, ClassLoader.getSystemClassLoader()
        );
        Method getNamespaces = resolverClass.getMethod("getNamespaces");
        Object namespaceValue = getNamespaces.invoke(resolver);
        if (!(namespaceValue instanceof Collection<?> namespaces) || !namespaces.contains(SOURCE_NAMESPACE)) {
            return null;
        }

        Method getRuntimeNamespace = resolverClass.getMethod("getCurrentRuntimeNamespace");
        String runtimeNamespace = String.valueOf(getRuntimeNamespace.invoke(resolver));
        Method mapClassName = resolverClass.getMethod("mapClassName", String.class, String.class);
        Method mapFieldName = resolverClass.getMethod(
            "mapFieldName", String.class, String.class, String.class, String.class
        );
        Method mapMethodName = resolverClass.getMethod(
            "mapMethodName", String.class, String.class, String.class, String.class
        );
        return new FabricRuntimeMappings(
            officialMappings,
            resolver,
            mapClassName,
            mapFieldName,
            mapMethodName,
            runtimeNamespace
        );
    }

    String runtimeNamespace() {
        return runtimeNamespace;
    }

    @Override
    public String className(String named) {
        String official = officialMappings.className(named);
        return invoke(mapClassName, official, SOURCE_NAMESPACE, official);
    }

    @Override
    public String fieldName(String owner, String named) {
        String officialOwner = officialMappings.className(owner);
        String officialName = officialMappings.fieldName(owner, named);
        String descriptor = officialMappings.fieldDescriptor(owner, named);
        if (descriptor == null) return named;
        return invoke(mapFieldName, officialName, SOURCE_NAMESPACE, officialOwner, officialName, descriptor);
    }

    @Override
    public String methodName(String owner, String named, int arity) {
        String officialOwner = officialMappings.className(owner);
        String officialName = officialMappings.methodName(owner, named, arity);
        String descriptor = officialMappings.methodDescriptor(owner, named, arity);
        if (descriptor == null) return named;
        return invoke(mapMethodName, officialName, SOURCE_NAMESPACE, officialOwner, officialName, descriptor);
    }

    private String invoke(Method method, String fallback, Object... arguments) {
        try {
            Object value = method.invoke(resolver, arguments);
            return value instanceof String mapped && !mapped.isBlank() ? mapped : fallback;
        } catch (Throwable ignored) {
            return fallback;
        }
    }
}
