package com.qtech.ruby.loader.api;

public interface RubyMappings {
    String className(String named);
    String fieldName(String owner, String named);
    String methodName(String owner, String named, int arity);
}
