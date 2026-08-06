package com.qtech.ruby.loader;

import com.qtech.ruby.loader.api.RubyMappings;
import com.qtech.ruby.loader.api.RubyModule;
import com.qtech.ruby.loader.api.RubyModuleContext;
import com.qtech.ruby.loader.api.RubyModuleMetadata;
import com.qtech.ruby.loader.api.RubySetting;
import com.qtech.ruby.loader.api.RubySettingsLocation;
import com.qtech.ruby.loader.api.RubySettingsPage;
import java.io.File;
import java.lang.instrument.Instrumentation;
import java.lang.invoke.MethodHandles;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.WeakHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import java.util.ServiceLoader;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import jdk.internal.org.objectweb.asm.ClassWriter;
import jdk.internal.org.objectweb.asm.MethodVisitor;
import jdk.internal.org.objectweb.asm.Opcodes;
import jdk.internal.org.objectweb.asm.Type;

/**
 * Ruby Launcher bootstrap agent.
 *
 * The javaagent is intentionally tiny: it loads Ruby modules and exposes a
 * title-screen module panel. Game features live in modules, not in this agent.
 */
public final class RubyModuleLoaderAgent {
    private static final String LOADER_VERSION = "0.5.0";
    private static final List<RubyModule> MODULES = new CopyOnWriteArrayList<>();
    private static final List<RubySettingsPage> SETTINGS_PAGES = new CopyOnWriteArrayList<>();
    private static final List<URLClassLoader> MODULE_LOADERS = new CopyOnWriteArrayList<>();
    private static final AtomicBoolean RUNTIME_STARTED = new AtomicBoolean(false);

    private RubyModuleLoaderAgent() {}

    public static void premain(String arguments, Instrumentation instrumentation) {
        if (!Boolean.parseBoolean(System.getProperty("ruby.loader.enabled", "true"))) return;

        log("Bootstrap entered through Java instrumentation.");
        try {
            exportInternalAsm(instrumentation);
            Path mappingsPath = Path.of(requiredProperty("ruby.loader.mappings"));
            MappingIndex officialMappings = MappingIndex.load(mappingsPath);
            String minecraftVersion = System.getProperty("ruby.loader.minecraftVersion", "unknown");
            Path gameDirectory = optionalPath(System.getProperty("ruby.loader.gameDirectory", ""));
            String platform = System.getProperty("ruby.loader.platform", "none").trim().toLowerCase(Locale.ROOT);

            if (platform.equals("fabric")) {
                log("Fabric Loader detected; waiting for its runtime mapping resolver before loading Ruby modules.");
                Thread fabricBootstrap = new Thread(
                    () -> bootstrapFabricRuntime(
                        instrumentation,
                        officialMappings,
                        minecraftVersion,
                        gameDirectory
                    ),
                    "Ruby Fabric Bootstrap"
                );
                fabricBootstrap.setDaemon(true);
                fabricBootstrap.setUncaughtExceptionHandler(
                    (ignored, error) -> log("Fabric module bootstrap stopped: " + concise(error))
                );
                fabricBootstrap.start();
                return;
            }

            startRuntime(instrumentation, officialMappings, minecraftVersion, gameDirectory);
        } catch (Throwable error) {
            log("Loader disabled: " + concise(error));
        }
    }

    private static void bootstrapFabricRuntime(
        Instrumentation instrumentation,
        MappingIndex officialMappings,
        String minecraftVersion,
        Path gameDirectory
    ) {
        long deadline = System.nanoTime() + 60_000_000_000L;
        long nextWaitingLogAt = 0L;
        String lastFailure = "Fabric mapping resolver is not initialized";
        while (System.nanoTime() < deadline && !Thread.currentThread().isInterrupted()) {
            try {
                FabricRuntimeMappings mappings = FabricRuntimeMappings.tryCreate(officialMappings);
                if (mappings != null) {
                    log("Fabric runtime mappings ready (namespace: " + mappings.runtimeNamespace() + ").");
                    startRuntime(instrumentation, mappings, minecraftVersion, gameDirectory);
                    return;
                }
                lastFailure = "the official mapping namespace is not ready";
            } catch (Throwable error) {
                lastFailure = concise(error);
            }

            long now = System.nanoTime();
            if (now >= nextWaitingLogAt) {
                log("Waiting for Fabric runtime mappings (" + lastFailure + ").");
                nextWaitingLogAt = now + 3_000_000_000L;
            }
            sleep(25L);
        }
        log("Loader disabled: Fabric runtime mappings did not become available within 60 seconds ("
            + lastFailure + ").");
    }

    private static void startRuntime(
        Instrumentation instrumentation,
        RubyMappings mappings,
        String minecraftVersion,
        Path gameDirectory
    ) {
        if (!RUNTIME_STARTED.compareAndSet(false, true)) return;
        RubyModuleContext context = new RubyModuleContext(
            instrumentation,
            mappings,
            minecraftVersion,
            gameDirectory,
            RubyModuleLoaderAgent::log
        );

        List<Path> modulePaths = discoverModulePaths(System.getProperty("ruby.loader.modules", ""));
        loadModules(modulePaths, context);
        log("Loaded " + MODULES.size() + " Ruby module(s) for Minecraft " + minecraftVersion + ".");

        Thread uiThread = new Thread(
            new ModuleUiRuntime(instrumentation, mappings),
            "Ruby Module Loader UI"
        );
        uiThread.setDaemon(true);
        uiThread.setUncaughtExceptionHandler(
            (ignored, error) -> log("Minecraft menu integration stopped: " + concise(error))
        );
        uiThread.start();
    }


    private static void exportInternalAsm(Instrumentation instrumentation) {
        try {
            Module javaBase = Object.class.getModule();
            Module rubyModule = RubyModuleLoaderAgent.class.getModule();
            instrumentation.redefineModule(
                javaBase,
                Set.of(),
                Map.of("jdk.internal.org.objectweb.asm", Set.of(rubyModule)),
                Map.of(),
                Set.of(),
                Map.of()
            );
            log("Client input transformation service is available.");
        } catch (Throwable error) {
            // Module loading and the title-screen UI can still work without the
            // optional bytecode hook. Controller movement will report the exact
            // unavailable reason instead of disabling Ruby Module Loader.
            log("Client input transformation service unavailable: " + concise(error));
        }
    }

    private static void loadModules(List<Path> paths, RubyModuleContext context) {
        if (paths.isEmpty()) {
            log("No bundled module paths were supplied.");
            return;
        }
        try {
            URL[] urls = paths.stream().map(path -> {
                try { return path.toUri().toURL(); }
                catch (Exception error) { throw new IllegalArgumentException(error); }
            }).toArray(URL[]::new);
            URLClassLoader loader = new URLClassLoader(urls, RubyModule.class.getClassLoader());
            MODULE_LOADERS.add(loader);
            ServiceLoader<RubyModule> serviceLoader = ServiceLoader.load(RubyModule.class, loader);
            for (RubyModule module : serviceLoader) {
                RubyModuleMetadata metadata = Objects.requireNonNull(module.metadata(), "Module metadata");
                try {
                    module.initialize(context);
                    MODULES.add(module);
                    for (RubySettingsPage page : module.settingsPages()) {
                        if (page != null && !page.settings().isEmpty()) {
                            SETTINGS_PAGES.add(page);
                            log("Module settings ready: " + page.title() + " (" + page.location() + ").");
                        }
                    }
                    log("Module ready: " + metadata.name() + " " + metadata.version() + " (" + metadata.id() + ").");
                } catch (Throwable error) {
                    log("Module failed: " + metadata.id() + " — " + concise(error));
                }
            }
        } catch (Throwable error) {
            log("Module discovery failed: " + concise(error));
        }
    }

    private static List<Path> discoverModulePaths(String configured) {
        List<Path> result = new ArrayList<>();
        if (configured == null || configured.isBlank()) return result;
        for (String raw : configured.split(java.util.regex.Pattern.quote(File.pathSeparator))) {
            if (raw == null || raw.isBlank()) continue;
            Path path = Path.of(raw).toAbsolutePath().normalize();
            if (Files.isDirectory(path)) {
                try (var stream = Files.list(path)) {
                    stream.filter(Files::isRegularFile)
                        .filter(file -> {
                            String name = file.getFileName().toString().toLowerCase(Locale.ROOT);
                            return name.endsWith(".rmod") || name.endsWith(".jar");
                        })
                        .sorted()
                        .forEach(result::add);
                } catch (Exception error) {
                    log("Could not scan module directory " + path + ": " + concise(error));
                }
            } else if (Files.isRegularFile(path)) {
                result.add(path);
            } else {
                log("Module path does not exist: " + path);
            }
        }
        return result;
    }

    private static RubySettingsPage firstSettingsPage(RubySettingsLocation location) {
        for (RubySettingsPage page : SETTINGS_PAGES) {
            if (page.location() == location && !page.settings().isEmpty()) return page;
        }
        return null;
    }

    /** Watches Minecraft's native menus and adds entries supplied by the loader or modules. */
    private static final class ModuleUiRuntime implements Runnable {
        private final Instrumentation instrumentation;
        private final RubyMappings mappings;
        private ClientBridge bridge;
        private final AtomicReference<Object> titleInjectedScreen = new AtomicReference<>();
        private final AtomicReference<Object> titleInjectedButton = new AtomicReference<>();
        private final AtomicReference<Object> controlsInjectedScreen = new AtomicReference<>();
        private final AtomicReference<Object> controlsInjectedButton = new AtomicReference<>();
        private final AtomicReference<Object> pendingScreen = new AtomicReference<>();
        private final AtomicReference<Object> failedTitleScreen = new AtomicReference<>();
        private final AtomicReference<Object> failedControlsScreen = new AtomicReference<>();
        private long nextWaitingLogAt;

        private ModuleUiRuntime(Instrumentation instrumentation, RubyMappings mappings) {
            this.instrumentation = instrumentation;
            this.mappings = mappings;
        }

        @Override
        public void run() {
            while (!Thread.currentThread().isInterrupted()) {
                try {
                    if (bridge == null) {
                        bridge = ClientBridge.tryCreate(instrumentation, mappings);
                        if (bridge == null) {
                            waiting("Waiting for Minecraft client bootstrap.");
                            sleep(150L);
                            continue;
                        }
                        log("Minecraft client bridge ready; module menu integration is armed.");
                    }

                    Object current = bridge.currentScreen();
                    maintainTitleButton(current);
                    maintainControlsButton(current, firstSettingsPage(RubySettingsLocation.CONTROLS));
                    sleep(150L);
                } catch (Throwable error) {
                    waiting("Minecraft menu bridge waiting: " + concise(error));
                    sleep(300L);
                }
            }
        }

        private void maintainTitleButton(Object current) throws Exception {
            if (current == null || !bridge.isTitleScreen(current)) {
                titleInjectedScreen.set(null);
                titleInjectedButton.set(null);
                if (current != failedTitleScreen.get()) failedTitleScreen.set(null);
                return;
            }

            Object knownButton = titleInjectedButton.get();
            if (current == titleInjectedScreen.get()
                && knownButton != null
                && !bridge.containsWidget(current, knownButton)) {
                titleInjectedScreen.set(null);
                titleInjectedButton.set(null);
                log("Minecraft rebuilt the title screen; restoring the Ruby Modules button.");
            }

            if (current == titleInjectedScreen.get()
                || current == failedTitleScreen.get()
                || !pendingScreen.compareAndSet(null, current)) return;

            Object target = current;
            bridge.execute(() -> {
                try {
                    if (bridge.currentScreen() == target) {
                        Object button = bridge.injectModulesButton(target);
                        if (button != null) {
                            titleInjectedScreen.set(target);
                            titleInjectedButton.set(button);
                            log("Added the Ruby Modules button to the Minecraft title screen.");
                        }
                    }
                } catch (Throwable error) {
                    failedTitleScreen.set(target);
                    log("Could not add title-screen Modules button to this screen: " + concise(error));
                } finally {
                    pendingScreen.compareAndSet(target, null);
                }
            });
        }

        private void maintainControlsButton(Object current, RubySettingsPage page) throws Exception {
            if (page == null || current == null || !bridge.isControlsScreen(current)) {
                controlsInjectedScreen.set(null);
                controlsInjectedButton.set(null);
                if (current != failedControlsScreen.get()) failedControlsScreen.set(null);
                return;
            }

            Object knownButton = controlsInjectedButton.get();
            if (current == controlsInjectedScreen.get()
                && knownButton != null
                && !bridge.containsWidget(current, knownButton)) {
                controlsInjectedScreen.set(null);
                controlsInjectedButton.set(null);
                log("Minecraft rebuilt the Controls screen; restoring " + page.buttonLabel() + ".");
            }

            if (current == controlsInjectedScreen.get()
                || current == failedControlsScreen.get()
                || !pendingScreen.compareAndSet(null, current)) return;

            Object target = current;
            bridge.execute(() -> {
                try {
                    if (bridge.currentScreen() == target) {
                        Object button = bridge.injectSettingsButton(target, page);
                        if (button != null) {
                            controlsInjectedScreen.set(target);
                            controlsInjectedButton.set(button);
                            log("Added " + page.buttonLabel() + " to Minecraft's Controls menu.");
                        }
                    }
                } catch (Throwable error) {
                    failedControlsScreen.set(target);
                    log("Could not add module settings to this Controls screen: " + concise(error));
                } finally {
                    pendingScreen.compareAndSet(target, null);
                }
            });
        }

        private void waiting(String message) {
            long now = System.nanoTime();
            if (now >= nextWaitingLogAt) {
                log(message);
                nextWaitingLogAt = now + 3_000_000_000L;
            }
        }
    }

    private static final class ClientBridge {
        private final Object minecraft;
        private final ClassLoader gameLoader;
        private final RubyMappings mappings;
        private final Method execute;
        private final Method setScreen;
        private final Field screenField;
        private final Class<?> titleScreenClass;
        private final Class<?> controlsScreenClass;
        private final Class<?> screenClass;
        private final Class<?> componentClass;
        private final Class<?> buttonClass;
        private final Method literal;
        private volatile Constructor<?> generatedSettingsScreenConstructor;

        private ClientBridge(
            Object minecraft,
            ClassLoader gameLoader,
            RubyMappings mappings,
            Method execute,
            Method setScreen,
            Field screenField,
            Class<?> titleScreenClass,
            Class<?> controlsScreenClass,
            Class<?> screenClass,
            Class<?> componentClass,
            Class<?> buttonClass,
            Method literal
        ) {
            this.minecraft = minecraft;
            this.gameLoader = gameLoader;
            this.mappings = mappings;
            this.execute = execute;
            this.setScreen = setScreen;
            this.screenField = screenField;
            this.titleScreenClass = titleScreenClass;
            this.controlsScreenClass = controlsScreenClass;
            this.screenClass = screenClass;
            this.componentClass = componentClass;
            this.buttonClass = buttonClass;
            this.literal = literal;
        }

        static ClientBridge tryCreate(Instrumentation instrumentation, RubyMappings mappings) throws Exception {
            Class<?> minecraftClass = findLoadedClass(instrumentation, mappings.className("net.minecraft.client.Minecraft"));
            if (minecraftClass == null) return null;
            ClassLoader gameLoader = minecraftClass.getClassLoader();
            Object minecraft = minecraftInstance(minecraftClass, mappings);
            if (minecraft == null) return null;

            Method execute = findRunnableExecutorMethod(minecraftClass);
            Class<?> screenClass = findClass(instrumentation, mappings.className("net.minecraft.client.gui.screens.Screen"), gameLoader);
            Class<?> titleScreenClass = findClass(instrumentation, mappings.className("net.minecraft.client.gui.screens.TitleScreen"), gameLoader);
            Class<?> controlsScreenClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.screens.options.controls.ControlsScreen"),
                gameLoader
            );
            Class<?> componentClass = findClass(instrumentation, mappings.className("net.minecraft.network.chat.Component"), gameLoader);
            Class<?> buttonClass = findClass(instrumentation, mappings.className("net.minecraft.client.gui.components.Button"), gameLoader);
            if (screenClass == null || titleScreenClass == null || componentClass == null || buttonClass == null) return null;

            Field screen = optionalField(minecraftClass, mappings.fieldName("net.minecraft.client.Minecraft", "screen"));
            if (screen == null) screen = findInstanceFieldAssignableTo(minecraftClass, screenClass);
            if (screen == null) throw new NoSuchFieldException("Minecraft screen field");
            String mappedSetScreen = mappings.methodName("net.minecraft.client.Minecraft", "setScreen", 1);
            Method setScreen = findNamedCompatibleSingleArgumentMethod(
                minecraftClass, mappedSetScreen, screenClass, void.class
            );
            if (setScreen == null) setScreen = findSingleArgumentMethod(minecraftClass, screenClass, void.class);
            if (setScreen == null) throw new NoSuchMethodException("Minecraft setScreen(Screen)");

            Method literal = null;
            String mappedLiteral = mappings.methodName("net.minecraft.network.chat.Component", "literal", 1);
            for (Method method : componentClass.getMethods()) {
                if (!Modifier.isStatic(method.getModifiers())
                    || method.getParameterCount() != 1
                    || method.getParameterTypes()[0] != String.class
                    || !componentClass.isAssignableFrom(method.getReturnType())) {
                    continue;
                }
                if (method.getName().equals(mappedLiteral) || method.getName().equals("literal")) {
                    literal = method;
                    break;
                }
            }
            if (literal == null) {
                for (Method method : componentClass.getMethods()) {
                    if (Modifier.isStatic(method.getModifiers())
                        && method.getParameterCount() == 1
                        && method.getParameterTypes()[0] == String.class
                        && componentClass.isAssignableFrom(method.getReturnType())) {
                        literal = method;
                        break;
                    }
                }
            }
            if (literal == null) throw new NoSuchMethodException("Component.literal(String)");
            literal.setAccessible(true);

            return new ClientBridge(
                minecraft, gameLoader, mappings, execute, setScreen, screen, titleScreenClass,
                controlsScreenClass, screenClass, componentClass, buttonClass, literal
            );
        }

        Object currentScreen() throws Exception { return screenField.get(minecraft); }
        boolean isTitleScreen(Object screen) { return titleScreenClass.isInstance(screen); }
        boolean isControlsScreen(Object screen) {
            return controlsScreenClass != null && controlsScreenClass.isInstance(screen);
        }
        void execute(Runnable runnable) throws Exception { execute.invoke(minecraft, runnable); }

        Object injectModulesButton(Object titleScreen) throws Exception {
            int width = screenWidth(titleScreen);
            return createAndAddButton(
                titleScreen,
                "Ruby Modules",
                Math.max(5, width - 115),
                5,
                110,
                20,
                () -> showModulesScreen(titleScreen),
                "Ruby Modules"
            );
        }

        Object injectSettingsButton(Object controlsScreen, RubySettingsPage page) throws Exception {
            int width = screenWidth(controlsScreen);
            return createAndAddButton(
                controlsScreen,
                page.buttonLabel(),
                Math.max(5, width - 165),
                5,
                160,
                20,
                () -> showSettingsScreen(controlsScreen, page),
                page.buttonLabel()
            );
        }

        private int screenWidth(Object screen) throws Exception {
            return intField(
                screen,
                screenClass,
                mappings.fieldName("net.minecraft.client.gui.screens.Screen", "width"),
                "width"
            );
        }

        private int screenHeight(Object screen) throws Exception {
            return intField(
                screen,
                screenClass,
                mappings.fieldName("net.minecraft.client.gui.screens.Screen", "height"),
                "height"
            );
        }

        private Object createAndAddButton(
            Object screen,
            String text,
            int x,
            int y,
            int width,
            int height,
            Runnable action,
            String purpose
        ) throws Exception {
            Object label = component(text);
            Object builder = createButtonBuilder(label, action, purpose);
            Method bounds = findBuilderBounds(builder.getClass());
            if (bounds == null) {
                throw new NoSuchMethodException("Button builder bounds(int,int,int,int)");
            }
            try {
                bounds.invoke(builder, x, y, width, height);
            } catch (IllegalArgumentException error) {
                throw new IllegalArgumentException(
                    "Button bounds invocation rejected arguments for " + methodDescription(bounds), error
                );
            }

            Method build = findZeroArgumentReturning(builder.getClass(), buttonClass);
            if (build == null) {
                throw new NoSuchMethodException("Button builder build() -> Button");
            }
            Object button;
            try {
                button = build.invoke(builder);
            } catch (IllegalArgumentException error) {
                throw new IllegalArgumentException(
                    "Button build invocation rejected arguments for " + methodDescription(build), error
                );
            }
            if (button == null || !buttonClass.isInstance(button)) {
                throw new IllegalStateException(
                    "Button builder returned " + typeName(button) + " instead of " + buttonClass.getName()
                );
            }
            addWidget(screen, button);
            return button;
        }

        private void addWidget(Object screen, Object widget) throws Exception {
            String mappedAdd = mappings.methodName(
                "net.minecraft.client.gui.screens.Screen", "addRenderableWidget", 1
            );
            Method add = findNamedCompatibleSingleArgumentMethod(
                screenClass, mappedAdd, widget.getClass(), null
            );
            if (add == null) add = findCompatibleSingleArgumentMethod(screenClass, widget.getClass());
            if (add == null) throw new NoSuchMethodException("Screen.addRenderableWidget(" + widget.getClass().getSimpleName() + ")");
            try {
                add.invoke(screen, widget);
            } catch (IllegalArgumentException error) {
                throw new IllegalArgumentException(
                    "Screen widget insertion rejected " + widget.getClass().getName()
                        + " for " + methodDescription(add), error
                );
            }
        }

        private void setButtonMessage(Object button, String text) throws Exception {
            Object message = component(text);
            String mappedSetMessage = mappings.methodName(
                "net.minecraft.client.gui.components.AbstractWidget", "setMessage", 1
            );
            Method setter = findNamedCompatibleSingleArgumentMethod(
                button.getClass(), mappedSetMessage, message.getClass(), void.class
            );
            if (setter == null) {
                setter = findNamedCompatibleSingleArgumentMethod(
                    button.getClass(), "setMessage", message.getClass(), void.class
                );
            }
            if (setter == null) {
                setter = findCompatibleSingleArgumentMethodReturning(
                    button.getClass(), message.getClass(), void.class
                );
            }
            if (setter == null) throw new NoSuchMethodException("AbstractWidget.setMessage(Component)");
            setter.invoke(button, message);
        }

        private void setWidgetActive(Object widget, boolean active) {
            String mappedActive = mappings.fieldName(
                "net.minecraft.client.gui.components.AbstractWidget", "active"
            );
            Field field = optionalField(widget.getClass(), mappedActive);
            if (field == null) field = optionalField(widget.getClass(), "active");
            if (field == null || field.getType() != boolean.class) return;
            try { field.setBoolean(widget, active); }
            catch (Throwable ignored) {}
        }

        boolean containsWidget(Object screen, Object widget) {
            if (screen == null || widget == null) return false;
            for (Class<?> type = screen.getClass(); type != null; type = type.getSuperclass()) {
                for (Field field : type.getDeclaredFields()) {
                    if (Modifier.isStatic(field.getModifiers())) continue;
                    try {
                        field.setAccessible(true);
                        Object value = field.get(screen);
                        if (value instanceof Collection<?> collection) {
                            for (Object entry : collection) {
                                if (entry == widget) return true;
                            }
                        } else if (value instanceof Iterable<?> iterable) {
                            for (Object entry : iterable) {
                                if (entry == widget) return true;
                            }
                        } else if (value != null && value.getClass().isArray()) {
                            int length = java.lang.reflect.Array.getLength(value);
                            for (int index = 0; index < length; index++) {
                                if (java.lang.reflect.Array.get(value, index) == widget) return true;
                            }
                        }
                    } catch (Throwable ignored) {
                        // Some implementation fields may be inaccessible. Continue with
                        // the remaining widget collections rather than failing the UI.
                    }
                }
            }
            return false;
        }

        private Object createButtonBuilder(Object label, Runnable action, String purpose) throws Exception {
            String mappedBuilder = mappings.methodName(
                "net.minecraft.client.gui.components.Button", "builder", 2
            );
            List<Method> candidates = new ArrayList<>();
            for (Method method : buttonClass.getDeclaredMethods()) {
                if (!Modifier.isStatic(method.getModifiers()) || method.getParameterCount() != 2) continue;
                Class<?>[] parameters = method.getParameterTypes();
                if (!parameters[0].isInstance(label) || !isFunctionalInterface(parameters[1])) continue;
                if (method.getReturnType() == void.class) continue;
                method.setAccessible(true);
                if (method.getName().equals(mappedBuilder) || method.getName().equals("builder")) {
                    candidates.add(0, method);
                } else {
                    candidates.add(method);
                }
            }
            if (candidates.isEmpty()) {
                throw new NoSuchMethodException(
                    "Button.builder compatible with label type " + label.getClass().getName()
                );
            }

            List<String> failures = new ArrayList<>();
            for (Method candidate : candidates) {
                Class<?> callbackType = candidate.getParameterTypes()[1];
                Object callback = interfaceProxy(callbackType, ignored -> action.run());
                if (!callbackType.isInstance(callback)) {
                    failures.add(methodDescription(candidate) + " callback proxy mismatch");
                    continue;
                }
                try {
                    Object builder = candidate.invoke(null, label, callback);
                    if (builder == null) {
                        failures.add(methodDescription(candidate) + " returned null");
                        continue;
                    }
                    Method bounds = findBuilderBounds(builder.getClass());
                    Method build = findZeroArgumentReturning(builder.getClass(), buttonClass);
                    if (bounds == null || build == null) {
                        failures.add(methodDescription(candidate)
                            + " returned " + builder.getClass().getName()
                            + " without a compatible bounds/build chain");
                        continue;
                    }
                    log("Resolved Button builder for " + purpose + ": " + methodDescription(candidate));
                    return builder;
                } catch (IllegalArgumentException error) {
                    failures.add(methodDescription(candidate) + " rejected runtime arguments");
                }
            }
            throw new IllegalArgumentException("No compatible Button builder succeeded: " + String.join("; ", failures));
        }

        private Method findBuilderBounds(Class<?> builderClass) {
            String mappedBounds = mappings.methodName(
                "net.minecraft.client.gui.components.Button$Builder", "bounds", 4
            );
            Method fallback = null;
            for (Class<?> cursor = builderClass; cursor != null; cursor = cursor.getSuperclass()) {
                for (Method method : cursor.getDeclaredMethods()) {
                    Class<?>[] parameters = method.getParameterTypes();
                    if (parameters.length != 4
                        || parameters[0] != int.class
                        || parameters[1] != int.class
                        || parameters[2] != int.class
                        || parameters[3] != int.class) {
                        continue;
                    }
                    if (method.getReturnType() != void.class
                        && !method.getReturnType().isAssignableFrom(builderClass)
                        && !builderClass.isAssignableFrom(method.getReturnType())) {
                        continue;
                    }
                    method.setAccessible(true);
                    if (method.getName().equals(mappedBounds) || method.getName().equals("bounds")) {
                        return method;
                    }
                    if (fallback == null) fallback = method;
                }
            }
            return fallback;
        }

        private void showSettingsScreen(Object previousScreen, RubySettingsPage page) {
            try {
                Constructor<?> constructor = settingsScreenConstructor();
                Object screen = constructor.newInstance(component(page.title()));
                SettingsScreenSupport.register(
                    screen,
                    new SettingsScreenContext(this, previousScreen, page)
                );
                setScreen.invoke(minecraft, screen);
            } catch (Throwable error) {
                log(page.title() + " could not open: " + concise(error));
            }
        }

        private Constructor<?> settingsScreenConstructor() throws Exception {
            Constructor<?> cached = generatedSettingsScreenConstructor;
            if (cached != null) return cached;
            synchronized (this) {
                cached = generatedSettingsScreenConstructor;
                if (cached != null) return cached;

                String packageName = screenClass.getPackageName();
                String simpleName = "RubyGeneratedModuleSettingsScreen";
                String binaryName = packageName.isEmpty() ? simpleName : packageName + "." + simpleName;
                String internalName = binaryName.replace('.', '/');
                String superName = Type.getInternalName(screenClass);
                String componentDescriptor = Type.getDescriptor(componentClass);
                String constructorDescriptor = "(" + componentDescriptor + ")V";
                String supportName = Type.getInternalName(SettingsScreenSupport.class);
                String initName = mappings.methodName(
                    "net.minecraft.client.gui.screens.Screen", "init", 0
                );
                String onCloseName = mappings.methodName(
                    "net.minecraft.client.gui.screens.Screen", "onClose", 0
                );

                ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_MAXS | ClassWriter.COMPUTE_FRAMES);
                writer.visit(
                    Opcodes.V17,
                    Opcodes.ACC_PUBLIC | Opcodes.ACC_FINAL | Opcodes.ACC_SUPER,
                    internalName,
                    null,
                    superName,
                    null
                );

                MethodVisitor constructor = writer.visitMethod(
                    Opcodes.ACC_PUBLIC,
                    "<init>",
                    constructorDescriptor,
                    null,
                    null
                );
                constructor.visitCode();
                constructor.visitVarInsn(Opcodes.ALOAD, 0);
                constructor.visitVarInsn(Opcodes.ALOAD, 1);
                constructor.visitMethodInsn(
                    Opcodes.INVOKESPECIAL,
                    superName,
                    "<init>",
                    constructorDescriptor,
                    false
                );
                constructor.visitInsn(Opcodes.RETURN);
                constructor.visitMaxs(0, 0);
                constructor.visitEnd();

                MethodVisitor init = writer.visitMethod(
                    Opcodes.ACC_PROTECTED,
                    initName,
                    "()V",
                    null,
                    null
                );
                init.visitCode();
                init.visitVarInsn(Opcodes.ALOAD, 0);
                init.visitMethodInsn(Opcodes.INVOKESPECIAL, superName, initName, "()V", false);
                init.visitVarInsn(Opcodes.ALOAD, 0);
                init.visitMethodInsn(
                    Opcodes.INVOKESTATIC,
                    supportName,
                    "initialize",
                    "(Ljava/lang/Object;)V",
                    false
                );
                init.visitInsn(Opcodes.RETURN);
                init.visitMaxs(0, 0);
                init.visitEnd();

                MethodVisitor close = writer.visitMethod(
                    Opcodes.ACC_PUBLIC,
                    onCloseName,
                    "()V",
                    null,
                    null
                );
                close.visitCode();
                close.visitVarInsn(Opcodes.ALOAD, 0);
                close.visitMethodInsn(
                    Opcodes.INVOKESTATIC,
                    supportName,
                    "close",
                    "(Ljava/lang/Object;)V",
                    false
                );
                close.visitInsn(Opcodes.RETURN);
                close.visitMaxs(0, 0);
                close.visitEnd();
                writer.visitEnd();

                MethodHandles.Lookup lookup = MethodHandles.privateLookupIn(
                    screenClass,
                    MethodHandles.lookup()
                );
                Class<?> generated;
                try {
                    generated = lookup.defineClass(writer.toByteArray());
                } catch (LinkageError alreadyDefined) {
                    generated = Class.forName(binaryName, false, gameLoader);
                }
                cached = generated.getDeclaredConstructor(componentClass);
                cached.setAccessible(true);
                generatedSettingsScreenConstructor = cached;
                log("Generated Minecraft-native module settings screen: " + generated.getName() + ".");
                return cached;
            }
        }

        private void populateSettingsScreen(Object screen, SettingsScreenContext context) throws Exception {
            if (!context.initialized.compareAndSet(false, true)) return;

            int width = screenWidth(screen);
            int height = screenHeight(screen);
            List<RubySetting> settings = context.page.settings();
            int settingsTop = 38;
            int reservedBottom = 56;
            int availableHeight = Math.max(60, height - reservedBottom - settingsTop);
            boolean useTwoColumns = settings.size() > 1
                && settings.size() * 15 > availableHeight
                && width >= 360;
            int columns = useTwoColumns ? 2 : 1;
            int rows = Math.max(1, (settings.size() + columns - 1) / columns);
            int rowSpacing = useTwoColumns
                ? 24
                : Math.max(15, Math.min(24, availableHeight / rows));
            int buttonHeight = Math.max(14, Math.min(20, rowSpacing - 2));
            int contentWidth = Math.max(220, Math.min(useTwoColumns ? 620 : 420, width - 20));
            int x = Math.max(10, (width - contentWidth) / 2);
            int columnGap = useTwoColumns ? 8 : 0;
            int buttonWidth = (contentWidth - columnGap * (columns - 1)) / columns;

            Object title = createAndAddButton(
                screen,
                context.page.title(),
                x,
                10,
                contentWidth,
                20,
                () -> {},
                context.page.title() + " heading"
            );
            setWidgetActive(title, false);

            for (int index = 0; index < settings.size(); index++) {
                RubySetting setting = settings.get(index);
                int row = index / columns;
                int column = index % columns;
                int buttonX = x + column * (buttonWidth + columnGap);
                int buttonY = settingsTop + row * rowSpacing;
                AtomicReference<Object> buttonReference = new AtomicReference<>();
                Object button = createAndAddButton(
                    screen,
                    setting.name() + ": " + setting.valueLabel(),
                    buttonX,
                    buttonY,
                    buttonWidth,
                    buttonHeight,
                    () -> {
                        try {
                            setting.cycle(1);
                            Object currentButton = buttonReference.get();
                            if (currentButton != null) {
                                setButtonMessage(
                                    currentButton,
                                    setting.name() + ": " + setting.valueLabel()
                                );
                            }
                            Object description = context.descriptionButton.get();
                            if (description != null) {
                                setButtonMessage(description, setting.description());
                            }
                        } catch (Throwable error) {
                            log("Could not change " + setting.name() + ": " + concise(error));
                        }
                    },
                    setting.name()
                );
                buttonReference.set(button);
                setWidgetActive(button, setting.active());
                context.settingButtons.put(button, setting);
            }

            int settingsBottom = settingsTop + rows * rowSpacing;
            int descriptionY = Math.max(settingsBottom + 2, height - 52);
            Object description = createAndAddButton(
                screen,
                context.page.description(),
                Math.max(5, (width - Math.min(420, width - 10)) / 2),
                descriptionY,
                Math.min(420, width - 10),
                20,
                () -> {},
                context.page.title() + " help text"
            );
            setWidgetActive(description, false);
            context.descriptionButton.set(description);

            createAndAddButton(
                screen,
                "Done",
                Math.max(5, (width - 150) / 2),
                Math.max(descriptionY + 24, height - 28),
                150,
                20,
                () -> returnTo(context.previousScreen),
                context.page.title() + " Done"
            );
        }

        private void returnTo(Object previousScreen) {
            try {
                setScreen.invoke(minecraft, previousScreen);
            } catch (Throwable error) {
                log("Could not return from module settings: " + concise(error));
            }
        }

        private void showModulesScreen(Object previousScreen) {
            try {
                Object title = component("Ruby Modules");
                StringBuilder details = new StringBuilder("Ruby Module Loader ").append(LOADER_VERSION);
                if (MODULES.isEmpty()) {
                    details.append("\nNo Ruby modules loaded.");
                } else {
                    for (RubyModule module : MODULES) {
                        RubyModuleMetadata metadata = module.metadata();
                        details.append("\n").append(metadata.name()).append(" ").append(metadata.version()).append(" — Enabled");
                    }
                }
                Object message = component(details.toString());
                Object done = component("Done");
                Object back = component("Back");

                Class<?> confirmClass = findClass(null, mappings.className("net.minecraft.client.gui.screens.ConfirmScreen"), gameLoader);
                if (confirmClass == null) {
                    log(details.toString().replace('\n', ' '));
                    return;
                }
                Object confirm = constructConfirmScreen(confirmClass, previousScreen, title, message, done, back);
                if (confirm == null) throw new NoSuchMethodException("Compatible ConfirmScreen constructor");
                setScreen.invoke(minecraft, confirm);
            } catch (Throwable error) {
                log("Modules screen could not open: " + concise(error));
            }
        }

        private Object constructConfirmScreen(
            Class<?> confirmClass,
            Object previousScreen,
            Object title,
            Object message,
            Object done,
            Object back
        ) throws Exception {
            Constructor<?>[] constructors = confirmClass.getDeclaredConstructors();
            Arrays.sort(constructors, (a, b) -> Integer.compare(b.getParameterCount(), a.getParameterCount()));
            List<String> failures = new ArrayList<>();
            for (Constructor<?> constructor : constructors) {
                Class<?>[] types = constructor.getParameterTypes();
                if (types.length < 3 || !isFunctionalInterface(types[0])) continue;
                Object[] componentsToUse = new Object[] { title, message, done, back };
                Object[] arguments = new Object[types.length];
                Object callback = interfaceProxy(types[0], ignored -> {
                    try { setScreen.invoke(minecraft, previousScreen); }
                    catch (Throwable error) { log("Could not return from Modules screen: " + concise(error)); }
                });
                arguments[0] = callback;
                boolean compatible = types[0].isInstance(callback);
                for (int i = 1; compatible && i < types.length; i++) {
                    Object value = componentsToUse[Math.min(i - 1, componentsToUse.length - 1)];
                    if (!types[i].isInstance(value)) {
                        compatible = false;
                        break;
                    }
                    arguments[i] = value;
                }
                if (!compatible) continue;
                constructor.setAccessible(true);
                try {
                    return constructor.newInstance(arguments);
                } catch (IllegalArgumentException error) {
                    failures.add(constructorDescription(constructor) + " rejected runtime arguments");
                }
            }
            if (!failures.isEmpty()) {
                throw new IllegalArgumentException("ConfirmScreen constructors failed: " + String.join("; ", failures));
            }
            return null;
        }

        private Object component(String text) throws Exception { return literal.invoke(null, text); }

        private Object interfaceProxy(Class<?> type, java.util.function.Consumer<Object[]> action) {
            InvocationHandler handler = (proxy, method, args) -> {
                if (method.getDeclaringClass() == Object.class) {
                    return switch (method.getName()) {
                        case "toString" -> "RubyModuleCallback";
                        case "hashCode" -> System.identityHashCode(proxy);
                        case "equals" -> proxy == (args == null ? null : args[0]);
                        default -> null;
                    };
                }
                action.accept(args == null ? new Object[0] : args);
                return defaultValue(method.getReturnType());
            };
            return Proxy.newProxyInstance(gameLoader, new Class<?>[] { type }, handler);
        }
    }

    private static final class SettingsScreenContext {
        private final ClientBridge bridge;
        private final Object previousScreen;
        private final RubySettingsPage page;
        private final AtomicBoolean initialized = new AtomicBoolean(false);
        private final AtomicReference<Object> descriptionButton = new AtomicReference<>();
        private final Map<Object, RubySetting> settingButtons = Collections.synchronizedMap(
            new IdentityHashMap<>()
        );

        private SettingsScreenContext(
            ClientBridge bridge,
            Object previousScreen,
            RubySettingsPage page
        ) {
            this.bridge = bridge;
            this.previousScreen = previousScreen;
            this.page = page;
        }
    }

    /** Entry points called by the runtime-generated Minecraft Screen subclass. */
    public static final class SettingsScreenSupport {
        private static final Map<Object, SettingsScreenContext> CONTEXTS = Collections.synchronizedMap(
            new WeakHashMap<>()
        );

        private SettingsScreenSupport() {}

        private static void register(Object screen, SettingsScreenContext context) {
            CONTEXTS.put(screen, context);
        }

        public static void initialize(Object screen) {
            SettingsScreenContext context = CONTEXTS.get(screen);
            if (context == null) {
                log("Generated module settings screen opened without a registered settings page.");
                return;
            }
            try {
                context.bridge.populateSettingsScreen(screen, context);
            } catch (Throwable error) {
                log("Could not initialize " + context.page.title() + ": " + concise(error));
            }
        }

        public static void close(Object screen) {
            SettingsScreenContext context = CONTEXTS.get(screen);
            if (context != null) context.bridge.returnTo(context.previousScreen);
        }
    }

    private static Object minecraftInstance(Class<?> minecraftClass, RubyMappings mappings) throws Exception {
        Method getInstance = optionalMethod(minecraftClass, mappings.methodName("net.minecraft.client.Minecraft", "getInstance", 0), 0);
        if (getInstance != null && Modifier.isStatic(getInstance.getModifiers())) {
            Object value = getInstance.invoke(null);
            if (value != null) return value;
        }
        Field instance = optionalField(minecraftClass, mappings.fieldName("net.minecraft.client.Minecraft", "instance"));
        if (instance == null) instance = findStaticFieldAssignableTo(minecraftClass, minecraftClass);
        return instance == null ? null : instance.get(null);
    }

    private static Class<?> findLoadedClass(Instrumentation instrumentation, String name) {
        if (instrumentation == null) return null;
        for (Class<?> loaded : instrumentation.getAllLoadedClasses()) {
            if (loaded.getName().equals(name)) return loaded;
        }
        return null;
    }

    private static Class<?> findClass(Instrumentation instrumentation, String name, ClassLoader preferred) {
        Class<?> loaded = findLoadedClass(instrumentation, name);
        if (loaded != null) return loaded;
        for (ClassLoader loader : new ClassLoader[] { preferred, Thread.currentThread().getContextClassLoader(), ClassLoader.getSystemClassLoader() }) {
            if (loader == null) continue;
            try { return Class.forName(name, false, loader); }
            catch (Throwable ignored) {}
        }
        return null;
    }

    private static Field optionalField(Class<?> type, String name) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            try {
                Field field = cursor.getDeclaredField(name);
                field.setAccessible(true);
                return field;
            } catch (NoSuchFieldException ignored) {}
        }
        return null;
    }

    private static Field findStaticFieldAssignableTo(Class<?> type, Class<?> valueType) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Field field : cursor.getDeclaredFields()) {
                if (Modifier.isStatic(field.getModifiers()) && valueType.isAssignableFrom(field.getType())) {
                    field.setAccessible(true);
                    return field;
                }
            }
        }
        return null;
    }

    private static Field findInstanceFieldAssignableTo(Class<?> type, Class<?> valueType) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Field field : cursor.getDeclaredFields()) {
                if (!Modifier.isStatic(field.getModifiers()) && valueType.isAssignableFrom(field.getType())) {
                    field.setAccessible(true);
                    return field;
                }
            }
        }
        return null;
    }

    private static Method optionalMethod(Class<?> type, String name, int arity) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getName().equals(name) && method.getParameterCount() == arity) {
                    method.setAccessible(true);
                    return method;
                }
            }
        }
        return null;
    }

    private static Method findRunnableExecutorMethod(Class<?> type) throws NoSuchMethodException {
        Method fallback = null;
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Method method : cursor.getDeclaredMethods()) {
                Class<?>[] parameters = method.getParameterTypes();
                if (parameters.length == 1 && parameters[0] == Runnable.class && method.getReturnType() == void.class) {
                    method.setAccessible(true);
                    if (method.getName().equals("execute")) return method;
                    fallback = method;
                }
            }
        }
        if (fallback != null) return fallback;
        throw new NoSuchMethodException("Minecraft Runnable executor");
    }

    private static Method findNamedCompatibleSingleArgumentMethod(
        Class<?> type,
        String name,
        Class<?> argumentType,
        Class<?> returnType
    ) {
        if (name == null || name.isBlank()) return null;
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (!method.getName().equals(name) || method.getParameterCount() != 1) continue;
                if (!method.getParameterTypes()[0].isAssignableFrom(argumentType)) continue;
                if (returnType != null && method.getReturnType() != returnType) continue;
                method.setAccessible(true);
                return method;
            }
        }
        return null;
    }

    private static boolean isFunctionalInterface(Class<?> type) {
        if (type == null || !type.isInterface()) return false;
        int abstractMethods = 0;
        for (Method method : type.getMethods()) {
            if (method.getDeclaringClass() == Object.class) continue;
            if (Modifier.isAbstract(method.getModifiers())) abstractMethods++;
        }
        return abstractMethods == 1;
    }

    private static String methodDescription(Method method) {
        return method.getDeclaringClass().getName() + "." + method.getName()
            + Arrays.toString(method.getParameterTypes()) + " -> " + method.getReturnType().getName();
    }

    private static String constructorDescription(Constructor<?> constructor) {
        return constructor.getDeclaringClass().getName() + Arrays.toString(constructor.getParameterTypes());
    }

    private static String typeName(Object value) {
        return value == null ? "null" : value.getClass().getName();
    }

    private static Method findSingleArgumentMethod(Class<?> type, Class<?> parameter, Class<?> returnType) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getParameterCount() == 1
                    && method.getParameterTypes()[0].isAssignableFrom(parameter)
                    && method.getReturnType() == returnType) {
                    method.setAccessible(true);
                    return method;
                }
            }
        }
        return null;
    }

    private static Method findCompatibleSingleArgumentMethodReturning(
        Class<?> type,
        Class<?> argumentType,
        Class<?> returnType
    ) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getParameterCount() == 1
                    && method.getParameterTypes()[0].isAssignableFrom(argumentType)
                    && method.getReturnType() == returnType) {
                    method.setAccessible(true);
                    return method;
                }
            }
        }
        return null;
    }

    private static Method findCompatibleSingleArgumentMethod(Class<?> type, Class<?> argumentType) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getParameterCount() == 1 && method.getParameterTypes()[0].isAssignableFrom(argumentType)) {
                    method.setAccessible(true);
                    return method;
                }
            }
        }
        return null;
    }

    private static Method findMethodBySignature(Class<?> type, String name, Class<?>... parameters) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            try {
                Method method = cursor.getDeclaredMethod(name, parameters);
                method.setAccessible(true);
                return method;
            } catch (NoSuchMethodException ignored) {}
        }
        return null;
    }


    private static Method findZeroArgumentReturning(Class<?> type, Class<?> returnType) {
        for (Class<?> cursor = type; cursor != null; cursor = cursor.getSuperclass()) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getParameterCount() == 0 && returnType.isAssignableFrom(method.getReturnType())) {
                    method.setAccessible(true);
                    return method;
                }
            }
        }
        return null;
    }

    private static Method findIntMethod(Class<?> type, int count) {
        for (Method method : type.getDeclaredMethods()) {
            if (method.getParameterCount() != count) continue;
            boolean allInt = true;
            for (Class<?> parameter : method.getParameterTypes()) allInt &= parameter == int.class;
            if (allInt) {
                method.setAccessible(true);
                return method;
            }
        }
        return null;
    }

    private static int intField(Object object, Class<?> owner, String mappedName, String fallbackName) throws Exception {
        Field field = optionalField(owner, mappedName);
        if (field == null) field = optionalField(owner, fallbackName);
        if (field == null) throw new NoSuchFieldException(owner.getName() + "." + fallbackName);
        return field.getInt(object);
    }

    private static Object defaultValue(Class<?> type) {
        if (!type.isPrimitive() || type == void.class) return null;
        if (type == boolean.class) return false;
        if (type == byte.class) return (byte) 0;
        if (type == short.class) return (short) 0;
        if (type == int.class) return 0;
        if (type == long.class) return 0L;
        if (type == float.class) return 0f;
        if (type == double.class) return 0d;
        if (type == char.class) return '\0';
        return null;
    }

    private static String requiredProperty(String name) {
        String value = System.getProperty(name);
        if (value == null || value.isBlank()) throw new IllegalStateException("Missing system property: " + name);
        return value;
    }

    private static Path optionalPath(String value) {
        return value == null || value.isBlank() ? null : Path.of(value).toAbsolutePath().normalize();
    }

    private static String concise(Throwable error) {
        Throwable root = error;
        while (root.getCause() != null && root.getCause() != root) root = root.getCause();
        String message = root.getMessage();
        return root.getClass().getSimpleName() + (message == null || message.isBlank() ? "" : ": " + message);
    }

    private static void sleep(long millis) {
        try { Thread.sleep(millis); }
        catch (InterruptedException interrupted) { Thread.currentThread().interrupt(); }
    }

    private static void log(String message) {
        System.out.println("[Ruby Module Loader] " + message);
    }
}
