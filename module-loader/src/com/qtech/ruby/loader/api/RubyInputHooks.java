package com.qtech.ruby.loader.api;

import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.IllegalClassFormatException;
import java.lang.instrument.Instrumentation;
import java.security.ProtectionDomain;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import jdk.internal.org.objectweb.asm.ClassReader;
import jdk.internal.org.objectweb.asm.ClassVisitor;
import jdk.internal.org.objectweb.asm.ClassWriter;
import jdk.internal.org.objectweb.asm.MethodVisitor;
import jdk.internal.org.objectweb.asm.Opcodes;
import jdk.internal.org.objectweb.asm.Type;

/**
 * Shared post-input hook supplied by Ruby Module Loader.
 *
 * Minecraft's KeyboardInput update overwrites Input every client tick. Ruby
 * installs this hook before KeyboardInput is normally loaded, then controller
 * modules apply their analog values at the end of that same update on
 * Minecraft's render thread.
 */
public final class RubyInputHooks {
    @FunctionalInterface
    public interface Listener {
        void afterInputUpdate(Object input) throws Exception;
    }

    private static final AtomicReference<Listener> LISTENER = new AtomicReference<>();
    private static final AtomicBoolean ARMED = new AtomicBoolean(false);
    private static final AtomicBoolean TRANSFORMED = new AtomicBoolean(false);

    private RubyInputHooks() {}

    public static void setListener(Listener listener) {
        LISTENER.set(listener);
    }

    public static void clearListener(Listener listener) {
        LISTENER.compareAndSet(listener, null);
    }

    /** Called from the transformed Minecraft input method. */
    public static void afterInputUpdate(Object input) {
        Listener listener = LISTENER.get();
        if (listener == null || input == null) return;
        try {
            listener.afterInputUpdate(input);
        } catch (Throwable error) {
            String message = error.getMessage();
            System.out.println("[Ruby Module Loader] Input hook failed: "
                + error.getClass().getSimpleName()
                + (message == null ? "" : ": " + message));
        }
    }

    /**
     * Arms the post-update transformer by runtime class name. This method is
     * intentionally safe to call before Minecraft has loaded KeyboardInput.
     */
    public static synchronized boolean installAfterUpdateHook(
        Instrumentation instrumentation,
        String inputClassName,
        String mappedMethodName
    ) throws Exception {
        Objects.requireNonNull(instrumentation, "instrumentation");
        Objects.requireNonNull(inputClassName, "inputClassName");
        Objects.requireNonNull(mappedMethodName, "mappedMethodName");

        if (ARMED.get()) return true;
        if (!instrumentation.isRetransformClassesSupported()) {
            throw new IllegalStateException("Java instrumentation does not support class retransformation");
        }

        InputTransformer transformer = new InputTransformer(inputClassName, mappedMethodName);
        instrumentation.addTransformer(transformer, true);
        ARMED.set(true);

        // The controller module normally runs before KeyboardInput loads. Keep
        // this fallback for unusual launchers or future loader combinations.
        Class<?> alreadyLoaded = null;
        for (Class<?> loaded : instrumentation.getAllLoadedClasses()) {
            if (loaded.getName().equals(inputClassName)) {
                alreadyLoaded = loaded;
                break;
            }
        }
        if (alreadyLoaded != null) {
            if (!instrumentation.isModifiableClass(alreadyLoaded)) {
                throw new IllegalStateException("Minecraft input class is not modifiable: " + inputClassName);
            }
            instrumentation.retransformClasses(alreadyLoaded);
            if (!transformer.transformed()) {
                throw new NoSuchMethodException(
                    "Could not find " + inputClassName + "." + mappedMethodName
                        + "(boolean,float) for the post-input hook"
                );
            }
        }
        return true;
    }

    /** Compatibility overload for modules that already resolved the class. */
    public static synchronized boolean installAfterUpdateHook(
        Instrumentation instrumentation,
        Class<?> inputClass,
        String mappedMethodName
    ) throws Exception {
        return installAfterUpdateHook(instrumentation, inputClass.getName(), mappedMethodName);
    }

    public static boolean transformed() {
        return TRANSFORMED.get();
    }

    private static final class InputTransformer implements ClassFileTransformer {
        private static final String HOOK_OWNER = "com/qtech/ruby/loader/api/RubyInputHooks";
        private static final String HOOK_NAME = "afterInputUpdate";
        private static final String HOOK_DESCRIPTOR = "(Ljava/lang/Object;)V";

        private final String targetInternalName;
        private final String methodName;
        private volatile boolean transformed;

        private InputTransformer(String targetClassName, String methodName) {
            this.targetInternalName = targetClassName.replace('.', '/');
            this.methodName = methodName;
        }

        boolean transformed() {
            return transformed;
        }

        @Override
        public byte[] transform(
            Module module,
            ClassLoader loader,
            String className,
            Class<?> classBeingRedefined,
            ProtectionDomain protectionDomain,
            byte[] classfileBuffer
        ) throws IllegalClassFormatException {
            if (!targetInternalName.equals(className)) return null;

            try {
                ClassReader reader = new ClassReader(classfileBuffer);
                ClassWriter writer = new LoaderAwareClassWriter(
                    reader,
                    ClassWriter.COMPUTE_MAXS | ClassWriter.COMPUTE_FRAMES,
                    loader
                );
                AtomicBoolean matched = new AtomicBoolean(false);
                ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
                    @Override
                    public MethodVisitor visitMethod(
                        int access,
                        String name,
                        String descriptor,
                        String signature,
                        String[] exceptions
                    ) {
                        MethodVisitor delegate = super.visitMethod(access, name, descriptor, signature, exceptions);
                        Type[] arguments = Type.getArgumentTypes(descriptor);
                        boolean matches = name.equals(methodName)
                            && Type.getReturnType(descriptor).equals(Type.VOID_TYPE)
                            && arguments.length == 2
                            && arguments[0].equals(Type.BOOLEAN_TYPE)
                            && arguments[1].equals(Type.FLOAT_TYPE)
                            && (access & Opcodes.ACC_STATIC) == 0;
                        if (!matches) return delegate;

                        matched.set(true);
                        return new MethodVisitor(Opcodes.ASM9, delegate) {
                            @Override
                            public void visitInsn(int opcode) {
                                if (opcode == Opcodes.RETURN) {
                                    super.visitVarInsn(Opcodes.ALOAD, 0);
                                    super.visitMethodInsn(
                                        Opcodes.INVOKESTATIC,
                                        HOOK_OWNER,
                                        HOOK_NAME,
                                        HOOK_DESCRIPTOR,
                                        false
                                    );
                                }
                                super.visitInsn(opcode);
                            }
                        };
                    }
                };
                reader.accept(visitor, ClassReader.EXPAND_FRAMES);
                if (!matched.get()) return null;
                transformed = true;
                if (TRANSFORMED.compareAndSet(false, true)) {
                    System.out.println("[Ruby Module Loader] Post-keyboard analog input hook installed in "
                        + targetInternalName.replace('/', '.') + "." + methodName + ".");
                }
                return writer.toByteArray();
            } catch (Throwable error) {
                IllegalClassFormatException wrapped = new IllegalClassFormatException(
                    "Ruby input hook transformation failed: " + error
                );
                wrapped.initCause(error);
                throw wrapped;
            }
        }
    }

    /** Resolves frame common types through Minecraft's own class loader. */
    private static final class LoaderAwareClassWriter extends ClassWriter {
        private final ClassLoader loader;

        private LoaderAwareClassWriter(ClassReader reader, int flags, ClassLoader loader) {
            super(reader, flags);
            this.loader = loader;
        }

        @Override
        protected String getCommonSuperClass(String type1, String type2) {
            try {
                ClassLoader selected = loader == null ? ClassLoader.getSystemClassLoader() : loader;
                Class<?> first = Class.forName(type1.replace('/', '.'), false, selected);
                Class<?> second = Class.forName(type2.replace('/', '.'), false, selected);
                if (first.isAssignableFrom(second)) return type1;
                if (second.isAssignableFrom(first)) return type2;
                if (first.isInterface() || second.isInterface()) return "java/lang/Object";
                do {
                    first = first.getSuperclass();
                } while (first != null && !first.isAssignableFrom(second));
                return first == null ? "java/lang/Object" : first.getName().replace('.', '/');
            } catch (Throwable ignored) {
                return "java/lang/Object";
            }
        }
    }
}
