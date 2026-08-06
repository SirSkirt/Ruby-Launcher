package com.qtech.ruby.modules.controller;

import com.qtech.ruby.loader.api.RubyInputHooks;
import com.qtech.ruby.loader.api.RubyMappings;
import com.qtech.ruby.loader.api.RubyModule;
import com.qtech.ruby.loader.api.RubyModuleContext;
import com.qtech.ruby.loader.api.RubyModuleMetadata;
import com.qtech.ruby.loader.api.RubySetting;
import com.qtech.ruby.loader.api.RubySettingsLocation;
import com.qtech.ruby.loader.api.RubySettingsPage;
import java.io.InputStream;
import java.io.OutputStream;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import java.util.Properties;
import java.util.function.Consumer;
import java.util.function.Supplier;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Ruby Controller Core.
 *
 * This agent reads the controller through LWJGL/GLFW and calls Minecraft's own
 * movement, camera and action APIs. It does not generate operating-system
 * keyboard or mouse events.
 */
public final class RubyControllerModule implements RubyModule {
    private static volatile RubyModuleContext moduleContext;
    private volatile ControllerSettings controllerSettings;

    @Override
    public RubyModuleMetadata metadata() {
        return new RubyModuleMetadata(
            "ruby-controller",
            "Controller Support",
            "0.8.4",
            List.of("windows", "linux", "android"),
            "Direct controller input without keyboard or mouse emulation."
        );
    }

    @Override
    public void initialize(RubyModuleContext context) {
        moduleContext = context;
        if (!Boolean.parseBoolean(System.getProperty("ruby.controller.enabled", "true"))) {
            log("Module is disabled by launcher settings.");
            return;
        }

        controllerSettings = ControllerSettings.load(context.gameDirectory());
        ControllerRuntime runtime = new ControllerRuntime(
            context.instrumentation(),
            context.mappings(),
            controllerSettings
        );
        runtime.armMovementHook();

        Thread thread = new Thread(runtime, "Ruby Controller Module");
        thread.setDaemon(true);
        thread.setUncaughtExceptionHandler((ignored, error) -> log("Controller thread stopped: " + error));
        thread.start();
    }

    @Override
    public List<RubySettingsPage> settingsPages() {
        ControllerSettings settings = controllerSettings;
        return settings == null ? List.of() : List.of(settings.page());
    }

    private static void log(String message) {
        RubyModuleContext context = moduleContext;
        if (context != null) {
            context.log("Ruby Controller", message);
        } else {
            System.out.println("[Ruby Controller] " + message);
        }
    }

    private static final class ControllerRuntime implements Runnable {
        private static final int MAX_JOYSTICKS = 16;
        private static final int BUTTON_A = 0;
        private static final int BUTTON_B = 1;
        private static final int BUTTON_X = 2;
        private static final int BUTTON_Y = 3;
        private static final int BUTTON_LB = 4;
        private static final int BUTTON_RB = 5;
        private static final int BUTTON_BACK = 6;
        private static final int BUTTON_START = 7;
        private static final int BUTTON_L3 = 9;
        private static final int BUTTON_DPAD_UP = 11;
        private static final int BUTTON_DPAD_RIGHT = 12;
        private static final int BUTTON_DPAD_DOWN = 13;
        private static final int BUTTON_DPAD_LEFT = 14;

        private final Instrumentation instrumentation;
        private final AtomicReference<ControllerFrame> latestFrame = new AtomicReference<>();
        private final AtomicBoolean taskQueued = new AtomicBoolean(false);
        private final RubyMappings mappings;
        private final ControllerSettings settings;
        private volatile MinecraftBridge bridge;
        private ClientBridge client;
        private GlfwBridge glfw;
        private Object gamepadState;
        private int joystickId = -1;
        private String controllerName = "";
        private boolean[] previousButtons = new boolean[15];
        private boolean previousForwardTap;
        private long lastForwardTapAt;
        private String lastActionError = "";
        private long nextActionErrorLogAt;
        private long nextGameplayBridgeAttemptAt;
        private boolean movementHookArmed;

        private ControllerRuntime(
            Instrumentation instrumentation,
            RubyMappings mappings,
            ControllerSettings settings
        ) {
            this.instrumentation = instrumentation;
            this.mappings = mappings;
            this.settings = Objects.requireNonNull(settings);
        }

        private void armMovementHook() {
            try {
                RubyInputHooks.setListener(this::afterKeyboardInputUpdate);
                String runtimeInputClass = mappings.className("net.minecraft.client.player.KeyboardInput");
                String mappedTick = mappings.methodName(
                    "net.minecraft.client.player.KeyboardInput", "tick", 2
                );
                RubyInputHooks.installAfterUpdateHook(instrumentation, runtimeInputClass, mappedTick);
                movementHookArmed = true;
                log("Analog movement hook armed before Minecraft loads KeyboardInput.");
            } catch (Throwable error) {
                movementHookArmed = false;
                log("Analog movement hook could not be armed: " + detailed(error));
            }
        }

        private void afterKeyboardInputUpdate(Object input) throws Exception {
            MinecraftBridge active = bridge;
            ControllerFrame frame = latestFrame.get();
            if (active != null && frame != null) {
                active.applyMovementAfterKeyboard(input, frame);
            }
        }

        @Override
        public void run() {
            log("Module initialized for Minecraft " + (moduleContext == null ? "unknown" : moduleContext.minecraftVersion()) + ".");
            log("Java runtime: " + System.getProperty("java.version")
                + " (" + System.getProperty("java.vendor", "unknown vendor") + ").");
            log("Runtime targets: Minecraft=" + mappings.className("net.minecraft.client.Minecraft")
                + ", LocalPlayer=" + mappings.className("net.minecraft.client.player.LocalPlayer")
                + ", Input=" + mappings.className("net.minecraft.client.player.Input") + ".");

            long nextWaitingLogAt = 0L;
            String clientFailure = "not attempted";
            while (client == null) {
                try {
                    client = ClientBridge.tryCreate(instrumentation, mappings);
                    clientFailure = client == null ? "Minecraft client not ready" : "ready";
                } catch (Throwable error) {
                    clientFailure = concise(error);
                }

                if (client == null) {
                    long now = System.nanoTime();
                    if (now >= nextWaitingLogAt) {
                        log("Waiting for Minecraft client bridge (" + clientFailure + ").");
                        nextWaitingLogAt = now + 3_000_000_000L;
                    }
                    sleepQuietly(150L);
                }
            }

            // A non-zero Minecraft window handle means GLFW initialization and
            // native window creation have completed. This is the safe boundary
            // that b2026.08 was missing when it touched GLFW during GLX._initGlfw.
            log("Minecraft client bridge is ready. Waiting for the title window before controller startup.");
            long windowReadySince = 0L;
            nextWaitingLogAt = 0L;
            while (!Thread.currentThread().isInterrupted()) {
                if (client.isWindowReady()) {
                    if (windowReadySince == 0L) windowReadySince = System.nanoTime();
                    if (System.nanoTime() - windowReadySince >= 500_000_000L) break;
                } else {
                    windowReadySince = 0L;
                }
                long now = System.nanoTime();
                if (now >= nextWaitingLogAt) {
                    log("Controller startup is safely paused until Minecraft's GLFW window exists.");
                    nextWaitingLogAt = now + 3_000_000_000L;
                }
                sleepQuietly(100L);
            }
            if (Thread.currentThread().isInterrupted()) return;

            String glfwFailure = "not attempted";
            nextWaitingLogAt = 0L;
            while (glfw == null) {
                try {
                    glfw = GlfwBridge.tryCreate(instrumentation, client.classLoader());
                    glfwFailure = glfw == null ? "Minecraft has not loaded GLFW yet" : "ready";
                } catch (Throwable error) {
                    glfwFailure = concise(error);
                }
                if (glfw == null) {
                    long now = System.nanoTime();
                    if (now >= nextWaitingLogAt) {
                        log("Waiting for LWJGL gamepad bridge (" + glfwFailure + ").");
                        nextWaitingLogAt = now + 3_000_000_000L;
                    }
                    sleepQuietly(150L);
                }
            }

            gamepadState = glfw.allocateState();
            log("Direct controller input layer initialized on Minecraft's live window. No keyboard/mouse event emulation is active.");
            log("Menu focus navigation is active before a world loads.");
            log("Legacy sprint enabled: click the left stick or double-tap the left stick forward.");

            long previousNanos = System.nanoTime();
            while (!Thread.currentThread().isInterrupted()) {
                try {
                    long nowNanos = System.nanoTime();
                    ensureGameplayBridge(nowNanos);
                    float deltaSeconds = Math.min(0.05f, Math.max(0.001f, (nowNanos - previousNanos) / 1_000_000_000f));
                    previousNanos = nowNanos;

                    if (!findController()) {
                        queueFrame(ControllerFrame.disconnected());
                        sleepQuietly(100L);
                        continue;
                    }

                    ControllerFrame frame = readFrame(deltaSeconds);
                    latestFrame.set(frame);
                    queueLatestOnGameThread();
                    sleepQuietly(8L);
                } catch (Throwable error) {
                    log("Input update failed: " + concise(error));
                    sleepQuietly(500L);
                }
            }

            if (gamepadState != null) {
                glfw.freeState(gamepadState);
            }
        }

        private void ensureGameplayBridge(long nowNanos) {
            if (bridge != null || nowNanos < nextGameplayBridgeAttemptAt) return;
            nextGameplayBridgeAttemptAt = nowNanos + 500_000_000L;
            try {
                MinecraftBridge candidate = MinecraftBridge.tryCreate(instrumentation, mappings);
                if (candidate == null) return;
                bridge = candidate;
                if (movementHookArmed) {
                    log("Gameplay bridge connected to the pre-armed analog input hook.");
                }
            } catch (Throwable error) {
                long now = System.nanoTime();
                String message = concise(error);
                if (!message.equals(lastActionError) || now >= nextActionErrorLogAt) {
                    log("Gameplay bridge waiting: " + message);
                    lastActionError = message;
                    nextActionErrorLogAt = now + 3_000_000_000L;
                }
            }
        }

        private boolean findController() {
            if (joystickId >= 0 && glfw.isGamepad(joystickId)) {
                return true;
            }

            joystickId = -1;
            for (int id = 0; id < MAX_JOYSTICKS; id++) {
                if (glfw.isGamepad(id)) {
                    joystickId = id;
                    String name = glfw.gamepadName(id);
                    if (name == null || name.isBlank()) {
                        name = glfw.joystickName(id);
                    }
                    controllerName = name == null ? "Unknown controller" : name;
                    previousButtons = new boolean[15];
                    log(controllerName + " connected (glyph family: " + detectFamily(controllerName) + ").");
                    return true;
                }
            }
            return false;
        }

        private ControllerFrame readFrame(float deltaSeconds) {
            if (!glfw.readState(joystickId, gamepadState)) {
                joystickId = -1;
                return ControllerFrame.disconnected();
            }

            float leftX = applyAxis(glfw.axis(gamepadState, 0), settings.deadzone());
            float leftY = applyAxis(glfw.axis(gamepadState, 1), settings.deadzone());
            float rightX = applyAxis(glfw.axis(gamepadState, 2), settings.deadzone());
            float rightY = applyAxis(glfw.axis(gamepadState, 3), settings.deadzone());
            float leftTrigger = normalizeTrigger(glfw.axis(gamepadState, 4));
            float rightTrigger = normalizeTrigger(glfw.axis(gamepadState, 5));

            boolean[] buttons = new boolean[15];
            boolean[] pressed = new boolean[15];
            boolean[] released = new boolean[15];
            for (int index = 0; index < buttons.length; index++) {
                buttons[index] = glfw.button(gamepadState, index);
                pressed[index] = buttons[index] && !previousButtons[index];
                released[index] = !buttons[index] && previousButtons[index];
            }
            previousButtons = buttons;

            float forward = -leftY;
            boolean forwardTap = forward >= 0.78f;
            boolean doubleTapSprint = false;
            long now = System.nanoTime();
            if (forwardTap && !previousForwardTap) {
                long elapsedMillis = (now - lastForwardTapAt) / 1_000_000L;
                if (lastForwardTapAt != 0L && elapsedMillis <= 320L) {
                    doubleTapSprint = true;
                    lastForwardTapAt = 0L;
                } else {
                    lastForwardTapAt = now;
                }
            }
            previousForwardTap = forwardTap;
            if (forward < 0.15f && lastForwardTapAt != 0L && (now - lastForwardTapAt) > 450_000_000L) {
                lastForwardTapAt = 0L;
            }

            return new ControllerFrame(
                true,
                controllerName,
                -leftX,
                forward,
                rightX,
                rightY,
                leftTrigger,
                rightTrigger,
                buttons,
                pressed,
                released,
                pressed[BUTTON_L3] || doubleTapSprint,
                deltaSeconds
            );
        }

        private void queueFrame(ControllerFrame frame) {
            latestFrame.set(frame);
            queueLatestOnGameThread();
        }

        private void queueLatestOnGameThread() {
            if (!taskQueued.compareAndSet(false, true)) {
                return;
            }
            try {
                client.execute(() -> {
                    try {
                        ControllerFrame frame = latestFrame.get();
                        if (frame != null) {
                            Object screen = client.currentScreen();
                            MinecraftBridge active = bridge;
                            if (screen != null) {
                                if (active != null) active.pauseForScreen();
                                client.applyMenu(frame, screen, settings);
                                boolean controllerCursorActive = client.inventoryCursorActive();
                                if (controllerCursorActive) {
                                    glfw.setCursorPosition(
                                        client.windowHandle(),
                                        client.inventoryCursorWindowX(),
                                        client.inventoryCursorWindowY()
                                    );
                                    glfw.setCursorHidden(client.windowHandle(), true);
                                } else {
                                    glfw.setCursorHidden(client.windowHandle(), false);
                                }
                            } else {
                                client.leaveScreen();
                                if (frame.connected) {
                                    // Returning from an inventory or block GUI must restore
                                    // Minecraft's captured gameplay cursor, not a visible one.
                                    glfw.setCursorCaptured(client.windowHandle());
                                }
                                if (active != null) active.apply(frame, settings);
                            }
                        }
                    } catch (Throwable error) {
                        String message = concise(error);
                        long now = System.nanoTime();
                        if (!message.equals(lastActionError) || now >= nextActionErrorLogAt) {
                            log("Minecraft action update failed: " + message);
                            lastActionError = message;
                            nextActionErrorLogAt = now + 3_000_000_000L;
                        }
                    } finally {
                        taskQueued.set(false);
                    }
                });
            } catch (Throwable error) {
                taskQueued.set(false);
            }
        }

        private static String detectFamily(String name) {
            String value = name.toLowerCase(Locale.ROOT);
            if (value.contains("dualshock") || value.contains("dualsense") || value.contains("playstation") || value.contains("sony")) {
                return "PlayStation";
            }
            if (value.contains("switch") || value.contains("joy-con") || value.contains("nintendo")) {
                return "Nintendo Switch";
            }
            if (value.contains("xbox") || value.contains("xinput") || value.contains("microsoft")) {
                return "Xbox";
            }
            return "Generic";
        }
    }

    private static final class ClientBridge {
        private final Object minecraft;
        private final ClassLoader gameClassLoader;
        private final Method execute;
        private final Field screenField;
        private final Method getWindow;
        private final Method getWindowHandle;
        private final Method getWindowScreenWidth;
        private final Method getWindowScreenHeight;
        private final Class<?> titleScreenClass;
        private final Class<?> selectWorldScreenClass;
        private final Class<?> screenClass;
        private final Class<?> abstractWidgetClass;
        private final Class<?> abstractButtonClass;
        private final Class<?> abstractContainerScreenClass;
        private final Class<?> renderableClass;
        private final Method children;
        private final Method getFocused;
        private final Method setFocused;
        private final Method widgetSetFocused;
        private final Method getX;
        private final Method getY;
        private final Method getWidth;
        private final Method getHeight;
        private final Method isActive;
        private final Field visibleField;
        private final Field widgetXField;
        private final Field widgetYField;
        private final Field widgetWidthField;
        private final Field widgetHeightField;
        private final Field screenWidthField;
        private final Field screenHeightField;
        private final Method buttonOnPress;
        private final Method screenOnClose;
        private final Method addRenderableOnly;
        private final Method containerMouseMoved;
        private final Method containerMouseClicked;
        private final Method containerMouseReleased;
        private final Method containerMouseDragged;
        private final Method screenMouseMoved;
        private final Method screenMouseClicked;
        private final Method screenMouseReleased;
        private final Method screenMouseDragged;
        private final Method guiFill;
        private final Method guiOutline;
        private Object lastScreen;
        private int heldDirection;
        private long nextDirectionAt;
        private Object cursorScreen;
        private Object cursorOverlay;
        private double cursorX;
        private double cursorY;
        private double cursorCarryX;
        private double cursorCarryY;
        private double previousCursorX;
        private double previousCursorY;
        private boolean cursorPrimaryHeld;
        private boolean cursorSecondaryHeld;
        private boolean cursorLogged;

        private ClientBridge(
            Object minecraft,
            ClassLoader gameClassLoader,
            Method execute,
            Field screenField,
            Method getWindow,
            Method getWindowHandle,
            Method getWindowScreenWidth,
            Method getWindowScreenHeight,
            Class<?> titleScreenClass,
            Class<?> selectWorldScreenClass,
            Class<?> screenClass,
            Class<?> abstractWidgetClass,
            Class<?> abstractButtonClass,
            Class<?> abstractContainerScreenClass,
            Class<?> renderableClass,
            Method children,
            Method getFocused,
            Method setFocused,
            Method widgetSetFocused,
            Method getX,
            Method getY,
            Method getWidth,
            Method getHeight,
            Method isActive,
            Field visibleField,
            Field widgetXField,
            Field widgetYField,
            Field widgetWidthField,
            Field widgetHeightField,
            Field screenWidthField,
            Field screenHeightField,
            Method buttonOnPress,
            Method screenOnClose,
            Method addRenderableOnly,
            Method containerMouseMoved,
            Method containerMouseClicked,
            Method containerMouseReleased,
            Method containerMouseDragged,
            Method screenMouseMoved,
            Method screenMouseClicked,
            Method screenMouseReleased,
            Method screenMouseDragged,
            Method guiFill,
            Method guiOutline
        ) {
            this.minecraft = minecraft;
            this.gameClassLoader = gameClassLoader;
            this.execute = execute;
            this.screenField = screenField;
            this.getWindow = getWindow;
            this.getWindowHandle = getWindowHandle;
            this.getWindowScreenWidth = getWindowScreenWidth;
            this.getWindowScreenHeight = getWindowScreenHeight;
            this.titleScreenClass = titleScreenClass;
            this.selectWorldScreenClass = selectWorldScreenClass;
            this.screenClass = screenClass;
            this.abstractWidgetClass = abstractWidgetClass;
            this.abstractButtonClass = abstractButtonClass;
            this.abstractContainerScreenClass = abstractContainerScreenClass;
            this.renderableClass = renderableClass;
            this.children = children;
            this.getFocused = getFocused;
            this.setFocused = setFocused;
            this.widgetSetFocused = widgetSetFocused;
            this.getX = getX;
            this.getY = getY;
            this.getWidth = getWidth;
            this.getHeight = getHeight;
            this.isActive = isActive;
            this.visibleField = visibleField;
            this.widgetXField = widgetXField;
            this.widgetYField = widgetYField;
            this.widgetWidthField = widgetWidthField;
            this.widgetHeightField = widgetHeightField;
            this.screenWidthField = screenWidthField;
            this.screenHeightField = screenHeightField;
            this.buttonOnPress = buttonOnPress;
            this.screenOnClose = screenOnClose;
            this.addRenderableOnly = addRenderableOnly;
            this.containerMouseMoved = containerMouseMoved;
            this.containerMouseClicked = containerMouseClicked;
            this.containerMouseReleased = containerMouseReleased;
            this.containerMouseDragged = containerMouseDragged;
            this.screenMouseMoved = screenMouseMoved;
            this.screenMouseClicked = screenMouseClicked;
            this.screenMouseReleased = screenMouseReleased;
            this.screenMouseDragged = screenMouseDragged;
            this.guiFill = guiFill;
            this.guiOutline = guiOutline;
        }

        static ClientBridge tryCreate(Instrumentation instrumentation, RubyMappings mappings) throws Exception {
            Class<?> minecraftClass = findLoadedClass(
                instrumentation, mappings.className("net.minecraft.client.Minecraft")
            );
            if (minecraftClass == null) return null;
            ClassLoader loader = minecraftClass.getClassLoader();

            Object minecraft = null;
            Method getInstance = findMethod(
                minecraftClass,
                mappings.methodName("net.minecraft.client.Minecraft", "getInstance", 0),
                0
            );
            if (getInstance != null && Modifier.isStatic(getInstance.getModifiers())) {
                minecraft = getInstance.invoke(null);
            }
            if (minecraft == null) {
                Field instance = optionalField(
                    minecraftClass,
                    mappings.fieldName("net.minecraft.client.Minecraft", "instance")
                );
                if (instance == null) instance = findStaticFieldAssignableTo(minecraftClass, minecraftClass);
                if (instance != null) minecraft = instance.get(null);
            }
            if (minecraft == null) return null;

            Class<?> screenClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.screens.Screen"),
                loader
            );
            Class<?> titleScreenClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.screens.TitleScreen"),
                loader
            );
            Class<?> selectWorldScreenClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.screens.worldselection.SelectWorldScreen"),
                loader
            );
            Class<?> listenerClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.components.events.GuiEventListener"),
                loader
            );
            Class<?> containerClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.components.events.AbstractContainerEventHandler"),
                loader
            );
            Class<?> widgetClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.components.AbstractWidget"),
                loader
            );
            Class<?> buttonClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.components.AbstractButton"),
                loader
            );
            Class<?> abstractContainerScreenClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.screens.inventory.AbstractContainerScreen"),
                loader
            );
            Class<?> renderableClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.components.Renderable"),
                loader
            );
            Class<?> guiGraphicsClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.GuiGraphics"),
                loader
            );
            Class<?> windowClass = findClass(
                instrumentation,
                mappings.className("com.mojang.blaze3d.platform.Window"),
                loader
            );
            if (screenClass == null || listenerClass == null || containerClass == null
                || widgetClass == null || buttonClass == null || windowClass == null
                || abstractContainerScreenClass == null || renderableClass == null || guiGraphicsClass == null) {
                return null;
            }

            Method execute = findRunnableExecutorMethod(minecraftClass);
            Field screen = optionalField(
                minecraftClass,
                mappings.fieldName("net.minecraft.client.Minecraft", "screen")
            );
            if (screen == null) screen = findInstanceFieldAssignableTo(minecraftClass, screenClass);
            if (screen == null) throw new NoSuchFieldException("Minecraft screen field");

            Method getWindow = findNamedMethodByParameters(
                minecraftClass,
                mappings.methodName("net.minecraft.client.Minecraft", "getWindow", 0),
                windowClass
            );
            if (getWindow == null) getWindow = findMethodByParameters(minecraftClass, windowClass);
            if (getWindow == null) throw new NoSuchMethodException("Minecraft.getWindow()");

            Method getHandle = findNamedMethodByParameters(
                windowClass,
                mappings.methodName("com.mojang.blaze3d.platform.Window", "getWindow", 0),
                long.class
            );
            if (getHandle == null) getHandle = findMethodByParameters(windowClass, long.class);
            if (getHandle == null) throw new NoSuchMethodException("Window.getWindow() -> long");

            Method getScreenWidth = findNamedMethodByParameters(
                windowClass,
                mappings.methodName("com.mojang.blaze3d.platform.Window", "getScreenWidth", 0),
                int.class
            );
            Method getScreenHeight = findNamedMethodByParameters(
                windowClass,
                mappings.methodName("com.mojang.blaze3d.platform.Window", "getScreenHeight", 0),
                int.class
            );

            Method children = findNamedMethodByParameters(
                screenClass,
                mappings.methodName("net.minecraft.client.gui.screens.Screen", "children", 0),
                List.class
            );
            if (children == null) {
                for (Method method : screenClass.getMethods()) {
                    if (method.getParameterCount() == 0 && List.class.isAssignableFrom(method.getReturnType())) {
                        method.setAccessible(true);
                        children = method;
                        break;
                    }
                }
            }
            if (children == null) throw new NoSuchMethodException("Screen.children()");

            Method getFocused = findNamedMethodByParameters(
                containerClass,
                mappings.methodName("net.minecraft.client.gui.components.events.AbstractContainerEventHandler", "getFocused", 0),
                listenerClass
            );
            Method setFocused = findNamedMethodByParameters(
                containerClass,
                mappings.methodName("net.minecraft.client.gui.components.events.AbstractContainerEventHandler", "setFocused", 1),
                void.class,
                listenerClass
            );
            Method widgetSetFocused = findNamedMethodByParameters(
                widgetClass,
                mappings.methodName("net.minecraft.client.gui.components.events.GuiEventListener", "setFocused", 1),
                void.class,
                boolean.class
            );
            if (widgetSetFocused == null) {
                widgetSetFocused = findNamedMethodByParameters(
                    widgetClass,
                    mappings.methodName("net.minecraft.client.gui.components.AbstractWidget", "setFocused", 1),
                    void.class,
                    boolean.class
                );
            }
            if (getFocused == null || setFocused == null || widgetSetFocused == null) {
                throw new NoSuchMethodException("Minecraft GUI focus methods");
            }

            Method getX = findNamedMethodByParameters(
                widgetClass, mappings.methodName("net.minecraft.client.gui.components.AbstractWidget", "getX", 0), int.class
            );
            Method getY = findNamedMethodByParameters(
                widgetClass, mappings.methodName("net.minecraft.client.gui.components.AbstractWidget", "getY", 0), int.class
            );
            Method getWidth = findNamedMethodByParameters(
                widgetClass, mappings.methodName("net.minecraft.client.gui.components.AbstractWidget", "getWidth", 0), int.class
            );
            Method getHeight = findNamedMethodByParameters(
                widgetClass, mappings.methodName("net.minecraft.client.gui.components.AbstractWidget", "getHeight", 0), int.class
            );
            Method isActive = findNamedMethodByParameters(
                widgetClass, mappings.methodName("net.minecraft.client.gui.components.AbstractWidget", "isActive", 0), boolean.class
            );
            Field widgetX = optionalField(
                widgetClass, mappings.fieldName("net.minecraft.client.gui.components.AbstractWidget", "x")
            );
            Field widgetY = optionalField(
                widgetClass, mappings.fieldName("net.minecraft.client.gui.components.AbstractWidget", "y")
            );
            Field widgetWidth = optionalField(
                widgetClass, mappings.fieldName("net.minecraft.client.gui.components.AbstractWidget", "width")
            );
            Field widgetHeight = optionalField(
                widgetClass, mappings.fieldName("net.minecraft.client.gui.components.AbstractWidget", "height")
            );
            if ((getX == null && widgetX == null) || (getY == null && widgetY == null)
                || (getWidth == null && widgetWidth == null) || (getHeight == null && widgetHeight == null)) {
                throw new NoSuchMethodException("AbstractWidget geometry methods or fields");
            }
            Field visible = optionalField(
                widgetClass,
                mappings.fieldName("net.minecraft.client.gui.components.AbstractWidget", "visible")
            );

            Method onPress = findNamedMethodByParameters(
                buttonClass,
                mappings.methodName("net.minecraft.client.gui.components.AbstractButton", "onPress", 0),
                void.class
            );
            if (onPress == null) throw new NoSuchMethodException("AbstractButton.onPress()");
            Method onClose = findNamedMethodByParameters(
                screenClass,
                mappings.methodName("net.minecraft.client.gui.screens.Screen", "onClose", 0),
                void.class
            );

            Field screenWidth = optionalField(
                screenClass, mappings.fieldName("net.minecraft.client.gui.screens.Screen", "width")
            );
            Field screenHeight = optionalField(
                screenClass, mappings.fieldName("net.minecraft.client.gui.screens.Screen", "height")
            );
            if (screenWidth == null || screenHeight == null) {
                throw new NoSuchFieldException("Screen width/height fields");
            }

            Method addRenderableOnly = findNamedCompatibleSingleArgumentMethod(
                screenClass,
                mappings.methodName("net.minecraft.client.gui.screens.Screen", "addRenderableOnly", 1),
                renderableClass,
                renderableClass
            );
            if (addRenderableOnly == null) {
                addRenderableOnly = findSingleArgumentCompatibleReturnMethod(
                    screenClass, renderableClass, renderableClass
                );
            }
            if (addRenderableOnly == null) {
                throw new NoSuchMethodException("Screen.addRenderableOnly(Renderable)");
            }

            Method mouseMoved = findNamedMethodByParameters(
                abstractContainerScreenClass,
                mappings.methodName(
                    "net.minecraft.client.gui.components.events.GuiEventListener", "mouseMoved", 2
                ),
                void.class, double.class, double.class
            );
            Method mouseClicked = findNamedMethodByParameters(
                abstractContainerScreenClass,
                mappings.methodName(
                    "net.minecraft.client.gui.screens.inventory.AbstractContainerScreen", "mouseClicked", 3
                ),
                boolean.class, double.class, double.class, int.class
            );
            Method mouseReleased = findNamedMethodByParameters(
                abstractContainerScreenClass,
                mappings.methodName(
                    "net.minecraft.client.gui.screens.inventory.AbstractContainerScreen", "mouseReleased", 3
                ),
                boolean.class, double.class, double.class, int.class
            );
            Method mouseDragged = findNamedMethodByParameters(
                abstractContainerScreenClass,
                mappings.methodName(
                    "net.minecraft.client.gui.screens.inventory.AbstractContainerScreen", "mouseDragged", 5
                ),
                boolean.class, double.class, double.class, int.class, double.class, double.class
            );
            if (mouseClicked == null || mouseReleased == null || mouseDragged == null) {
                throw new NoSuchMethodException("AbstractContainerScreen pointer methods");
            }

            Method screenMouseMoved = findNamedMethodByParameters(
                screenClass,
                mappings.methodName(
                    "net.minecraft.client.gui.components.events.GuiEventListener", "mouseMoved", 2
                ),
                void.class, double.class, double.class
            );
            Method screenMouseClicked = findNamedMethodByParameters(
                screenClass,
                mappings.methodName(
                    "net.minecraft.client.gui.components.events.GuiEventListener", "mouseClicked", 3
                ),
                boolean.class, double.class, double.class, int.class
            );
            Method screenMouseReleased = findNamedMethodByParameters(
                screenClass,
                mappings.methodName(
                    "net.minecraft.client.gui.components.events.GuiEventListener", "mouseReleased", 3
                ),
                boolean.class, double.class, double.class, int.class
            );
            Method screenMouseDragged = findNamedMethodByParameters(
                screenClass,
                mappings.methodName(
                    "net.minecraft.client.gui.components.events.GuiEventListener", "mouseDragged", 5
                ),
                boolean.class, double.class, double.class, int.class, double.class, double.class
            );
            if (selectWorldScreenClass != null
                && (screenMouseClicked == null || screenMouseReleased == null)) {
                log("World-selection cursor unavailable: generic Screen pointer methods were not found.");
                selectWorldScreenClass = null;
            }

            Method fill = findNamedMethodByParameters(
                guiGraphicsClass,
                mappings.methodName("net.minecraft.client.gui.GuiGraphics", "fill", 5),
                void.class, int.class, int.class, int.class, int.class, int.class
            );
            Method outline = findNamedMethodByParameters(
                guiGraphicsClass,
                mappings.methodName("net.minecraft.client.gui.GuiGraphics", "renderOutline", 5),
                void.class, int.class, int.class, int.class, int.class, int.class
            );
            if (fill == null) throw new NoSuchMethodException("GuiGraphics.fill(int,int,int,int,int)");

            return new ClientBridge(
                minecraft, loader, execute, screen, getWindow, getHandle, getScreenWidth, getScreenHeight, titleScreenClass,
                selectWorldScreenClass, screenClass, widgetClass, buttonClass, abstractContainerScreenClass, renderableClass,
                children, getFocused, setFocused, widgetSetFocused,
                getX, getY, getWidth, getHeight, isActive, visible,
                widgetX, widgetY, widgetWidth, widgetHeight, screenWidth, screenHeight,
                onPress, onClose, addRenderableOnly, mouseMoved, mouseClicked, mouseReleased,
                mouseDragged, screenMouseMoved, screenMouseClicked, screenMouseReleased,
                screenMouseDragged, fill, outline
            );
        }

        ClassLoader classLoader() { return gameClassLoader; }
        void execute(Runnable runnable) throws Exception { execute.invoke(minecraft, runnable); }
        Object currentScreen() throws Exception { return screenField.get(minecraft); }

        long windowHandle() {
            try {
                Object window = getWindow.invoke(minecraft);
                return window == null ? 0L : ((Number) getWindowHandle.invoke(window)).longValue();
            } catch (Throwable ignored) {
                return 0L;
            }
        }

        boolean isWindowReady() {
            return windowHandle() != 0L;
        }

        boolean inventoryCursorActive() {
            return cursorScreen != null;
        }

        double inventoryCursorWindowX() {
            return scaleCursorCoordinate(cursorX, screenWidth(), windowScreenWidth());
        }

        double inventoryCursorWindowY() {
            return scaleCursorCoordinate(cursorY, screenHeight(), windowScreenHeight());
        }

        void leaveScreen() {
            lastScreen = null;
            heldDirection = 0;
            nextDirectionAt = 0L;
            if (cursorScreen != null) resetCursorState();
        }

        void applyMenu(ControllerFrame frame, Object screen, ControllerSettings settings) throws Exception {
            if (screen == null) return;
            if (!frame.connected) {
                if (cursorScreen != null) resetCursorState();
                return;
            }

            boolean containerCursor = abstractContainerScreenClass.isInstance(screen);
            boolean worldSelectionCursor = selectWorldScreenClass != null
                && selectWorldScreenClass.isInstance(screen);
            if ((containerCursor || worldSelectionCursor) && settings.inventoryCursor()) {
                applyControllerCursor(frame, screen, settings, containerCursor);
                return;
            }

            if (cursorScreen != null) {
                resetCursorState();
            }
            if (!settings.menuNavigation()) return;

            if (screen != lastScreen) {
                lastScreen = screen;
                heldDirection = 0;
                nextDirectionAt = 0L;
                ensureInitialFocus(screen);
            }

            int direction = menuDirection(frame);
            long now = System.nanoTime();
            boolean navigate = false;
            if (direction == 0) {
                heldDirection = 0;
                nextDirectionAt = 0L;
            } else if (direction != heldDirection) {
                heldDirection = direction;
                nextDirectionAt = now + settings.menuInitialDelayNanos();
                navigate = true;
            } else if (now >= nextDirectionAt) {
                nextDirectionAt = now + settings.menuRepeatNanos();
                navigate = true;
            }
            if (navigate) moveFocus(screen, direction);

            if (frame.pressed(ControllerRuntime.BUTTON_A)) {
                Object focused = getFocused.invoke(screen);
                if (focused != null && abstractButtonClass.isInstance(focused)) {
                    buttonOnPress.invoke(focused);
                }
            }
            if (frame.pressed(ControllerRuntime.BUTTON_B)
                && screenOnClose != null
                && (titleScreenClass == null || !titleScreenClass.isInstance(screen))) {
                screenOnClose.invoke(screen);
            }
        }

        private void applyControllerCursor(
            ControllerFrame frame,
            Object screen,
            ControllerSettings settings,
            boolean containerCursor
        ) throws Exception {
            if (screen != cursorScreen) {
                resetCursorState();
                cursorScreen = screen;
                int width = screenWidth();
                int height = screenHeight();
                cursorX = width / 2.0;
                cursorY = height / 2.0;
                previousCursorX = cursorX;
                previousCursorY = cursorY;
                cursorOverlay = Proxy.newProxyInstance(
                    renderableClass.getClassLoader(),
                    new Class<?>[] { renderableClass },
                    (proxy, method, arguments) -> {
                        if (method.getDeclaringClass() == Object.class) {
                            return switch (method.getName()) {
                                case "toString" -> "Ruby Inventory Crosshair";
                                case "hashCode" -> System.identityHashCode(proxy);
                                case "equals" -> proxy == (arguments == null ? null : arguments[0]);
                                default -> null;
                            };
                        }
                        if (arguments != null && arguments.length == 4 && arguments[0] != null) {
                            renderCursor(arguments[0]);
                        }
                        return defaultValue(method.getReturnType());
                    }
                );
                addRenderableOnly.invoke(screen, cursorOverlay);
                if (!cursorLogged) {
                    log("Controller crosshair cursor is active for inventories, block GUIs and world selection.");
                    cursorLogged = true;
                }
            }

            int width = screenWidth();
            int height = screenHeight();
            float leftX = -frame.strafe;
            float leftY = -frame.forward;
            float cursorAxisX = Math.abs(frame.lookX) > Math.abs(leftX) ? frame.lookX : leftX;
            float cursorAxisY = Math.abs(frame.lookY) > Math.abs(leftY) ? frame.lookY : leftY;
            double speed = settings.cursorSpeed();
            double dx = cursorAxisX * speed * frame.deltaSeconds;
            double dy = cursorAxisY * speed * frame.deltaSeconds;

            previousCursorX = cursorX;
            previousCursorY = cursorY;
            cursorX = clamp(cursorX + dx, 4.0, Math.max(4.0, width - 5.0));
            cursorY = clamp(cursorY + dy, 4.0, Math.max(4.0, height - 5.0));
            double movedX = cursorX - previousCursorX;
            double movedY = cursorY - previousCursorY;

            Method mouseMoved = containerCursor ? containerMouseMoved : screenMouseMoved;
            Method mouseClicked = containerCursor ? containerMouseClicked : screenMouseClicked;
            Method mouseReleased = containerCursor ? containerMouseReleased : screenMouseReleased;
            Method mouseDragged = containerCursor ? containerMouseDragged : screenMouseDragged;

            if (mouseMoved != null) {
                mouseMoved.invoke(screen, cursorX, cursorY);
            }

            boolean primary = frame.button(ControllerRuntime.BUTTON_A)
                || frame.rightTrigger >= settings.triggerThreshold();
            boolean secondary = frame.button(ControllerRuntime.BUTTON_X)
                || frame.leftTrigger >= settings.triggerThreshold();

            if (primary && !cursorPrimaryHeld) {
                mouseClicked.invoke(screen, cursorX, cursorY, 0);
            } else if (primary && cursorPrimaryHeld && mouseDragged != null
                && (movedX != 0.0 || movedY != 0.0)) {
                mouseDragged.invoke(screen, cursorX, cursorY, 0, movedX, movedY);
            } else if (!primary && cursorPrimaryHeld) {
                mouseReleased.invoke(screen, cursorX, cursorY, 0);
            }

            if (secondary && !cursorSecondaryHeld) {
                mouseClicked.invoke(screen, cursorX, cursorY, 1);
            } else if (secondary && cursorSecondaryHeld && mouseDragged != null
                && (movedX != 0.0 || movedY != 0.0)) {
                mouseDragged.invoke(screen, cursorX, cursorY, 1, movedX, movedY);
            } else if (!secondary && cursorSecondaryHeld) {
                mouseReleased.invoke(screen, cursorX, cursorY, 1);
            }

            cursorPrimaryHeld = primary;
            cursorSecondaryHeld = secondary;

            if (frame.pressed(ControllerRuntime.BUTTON_B) && screenOnClose != null) {
                screenOnClose.invoke(screen);
            }
        }

        private void renderCursor(Object graphics) throws Exception {
            if (cursorScreen == null || graphics == null) return;
            int x = (int) Math.round(cursorX);
            int y = (int) Math.round(cursorY);
            int shadow = 0xE0000000;
            int white = 0xFFFFFFFF;

            if (guiOutline != null) {
                guiOutline.invoke(graphics, x - 6, y - 6, 13, 13, shadow);
            }
            guiFill.invoke(graphics, x - 7, y - 2, x + 8, y + 3, shadow);
            guiFill.invoke(graphics, x - 2, y - 7, x + 3, y + 8, shadow);
            guiFill.invoke(graphics, x - 6, y - 1, x + 7, y + 2, white);
            guiFill.invoke(graphics, x - 1, y - 6, x + 2, y + 7, white);
        }

        private void resetCursorState() {
            cursorScreen = null;
            cursorOverlay = null;
            cursorCarryX = 0.0;
            cursorCarryY = 0.0;
            cursorPrimaryHeld = false;
            cursorSecondaryHeld = false;
        }

        private int screenWidth() {
            try { return screenWidthField.getInt(cursorScreen); }
            catch (Throwable ignored) { return 320; }
        }

        private int screenHeight() {
            try { return screenHeightField.getInt(cursorScreen); }
            catch (Throwable ignored) { return 240; }
        }

        private int windowScreenWidth() {
            if (getWindowScreenWidth == null) return screenWidth();
            try {
                Object window = getWindow.invoke(minecraft);
                return window == null ? screenWidth() : ((Number) getWindowScreenWidth.invoke(window)).intValue();
            } catch (Throwable ignored) {
                return screenWidth();
            }
        }

        private int windowScreenHeight() {
            if (getWindowScreenHeight == null) return screenHeight();
            try {
                Object window = getWindow.invoke(minecraft);
                return window == null ? screenHeight() : ((Number) getWindowScreenHeight.invoke(window)).intValue();
            } catch (Throwable ignored) {
                return screenHeight();
            }
        }

        private static double scaleCursorCoordinate(double value, int guiSize, int windowSize) {
            if (guiSize <= 0 || windowSize <= 0) return value;
            return value * windowSize / (double) guiSize;
        }

        private int menuDirection(ControllerFrame frame) {
            if (frame.button(ControllerRuntime.BUTTON_DPAD_UP)) return 1;
            if (frame.button(ControllerRuntime.BUTTON_DPAD_DOWN)) return 2;
            if (frame.button(ControllerRuntime.BUTTON_DPAD_LEFT)) return 3;
            if (frame.button(ControllerRuntime.BUTTON_DPAD_RIGHT)) return 4;

            float horizontal = frame.strafe;
            float vertical = frame.forward;
            if (Math.abs(horizontal) > Math.abs(vertical) && Math.abs(horizontal) > 0.55f) {
                return horizontal > 0.0f ? 3 : 4;
            }
            if (Math.abs(vertical) > 0.55f) {
                return vertical > 0.0f ? 1 : 2;
            }
            return 0;
        }

        private void ensureInitialFocus(Object screen) throws Exception {
            Object focused = getFocused.invoke(screen);
            List<Object> candidates = candidates(screen);
            if (focused != null && candidates.contains(focused)) return;
            if (!candidates.isEmpty()) focus(screen, null, candidates.get(0));
        }

        private void moveFocus(Object screen, int direction) throws Exception {
            List<Object> candidates = candidates(screen);
            if (candidates.isEmpty()) return;
            Object current = getFocused.invoke(screen);
            if (current == null || !candidates.contains(current)) {
                focus(screen, current, candidates.get(0));
                return;
            }

            int cx = centerX(current);
            int cy = centerY(current);
            Object best = null;
            long bestScore = Long.MAX_VALUE;
            for (Object candidate : candidates) {
                if (candidate == current) continue;
                int tx = centerX(candidate);
                int ty = centerY(candidate);
                int dx = tx - cx;
                int dy = ty - cy;
                int primary;
                int cross;
                boolean sameLane;

                if (direction == 1) {
                    if (dy >= 0) continue;
                    primary = -dy;
                    cross = Math.abs(dx);
                    sameLane = horizontalOverlap(current, candidate);
                } else if (direction == 2) {
                    if (dy <= 0) continue;
                    primary = dy;
                    cross = Math.abs(dx);
                    sameLane = horizontalOverlap(current, candidate);
                } else if (direction == 3) {
                    if (dx >= 0) continue;
                    primary = -dx;
                    cross = Math.abs(dy);
                    sameLane = verticalOverlap(current, candidate);
                } else {
                    if (dx <= 0) continue;
                    primary = dx;
                    cross = Math.abs(dy);
                    sameLane = verticalOverlap(current, candidate);
                }

                long lanePenalty = sameLane ? 0L : 1_000_000L;
                long score = lanePenalty + (long) primary * 1_000L + (long) cross * cross;
                if (score < bestScore) {
                    bestScore = score;
                    best = candidate;
                }
            }

            if (best == null) {
                best = wrappedCandidate(candidates, current, direction);
            }
            if (best != null) focus(screen, current, best);
        }

        private Object wrappedCandidate(List<Object> candidates, Object current, int direction) {
            int cx = centerX(current);
            int cy = centerY(current);
            Object best = null;
            long bestScore = Long.MAX_VALUE;
            for (Object candidate : candidates) {
                if (candidate == current) continue;
                int tx = centerX(candidate);
                int ty = centerY(candidate);
                int cross = (direction <= 2) ? Math.abs(tx - cx) : Math.abs(ty - cy);
                int edge = switch (direction) {
                    case 1 -> -ty;
                    case 2 -> ty;
                    case 3 -> -tx;
                    default -> tx;
                };
                boolean sameLane = direction <= 2
                    ? horizontalOverlap(current, candidate)
                    : verticalOverlap(current, candidate);
                long score = (sameLane ? 0L : 1_000_000L)
                    + (long) cross * cross
                    + (long) (edge + 100_000) * 10L;
                if (score < bestScore) {
                    bestScore = score;
                    best = candidate;
                }
            }
            return best;
        }

        private List<Object> candidates(Object screen) throws Exception {
            Object value = children.invoke(screen);
            if (!(value instanceof Collection<?> collection)) return List.of();
            List<Object> result = new ArrayList<>();
            for (Object child : collection) {
                if (child == null || !abstractWidgetClass.isInstance(child)) continue;
                if (isActive != null && !(Boolean) isActive.invoke(child)) continue;
                if (visibleField != null && !visibleField.getBoolean(child)) continue;
                result.add(child);
            }
            result.sort(Comparator.comparingInt(this::centerY).thenComparingInt(this::centerX));
            return result;
        }

        private void focus(Object screen, Object old, Object next) throws Exception {
            if (old != null && abstractWidgetClass.isInstance(old)) widgetSetFocused.invoke(old, false);
            setFocused.invoke(screen, next);
            widgetSetFocused.invoke(next, true);
        }

        private boolean horizontalOverlap(Object first, Object second) {
            return left(first) < right(second) && right(first) > left(second);
        }

        private boolean verticalOverlap(Object first, Object second) {
            return top(first) < bottom(second) && bottom(first) > top(second);
        }

        private int left(Object widget) { return geometry(widget, getX, widgetXField, 0); }
        private int top(Object widget) { return geometry(widget, getY, widgetYField, 0); }
        private int width(Object widget) { return geometry(widget, getWidth, widgetWidthField, 0); }
        private int height(Object widget) { return geometry(widget, getHeight, widgetHeightField, 0); }
        private int right(Object widget) { return left(widget) + width(widget); }
        private int bottom(Object widget) { return top(widget) + height(widget); }
        private int centerX(Object widget) { return left(widget) + width(widget) / 2; }
        private int centerY(Object widget) { return top(widget) + height(widget) / 2; }

        private int geometry(Object widget, Method getter, Field field, int fallback) {
            if (getter != null) {
                try { return ((Number) getter.invoke(widget)).intValue(); }
                catch (Throwable ignored) {}
            }
            if (field != null) {
                try { return field.getInt(widget); }
                catch (Throwable ignored) {}
            }
            return fallback;
        }
    }

    private static final class MinecraftBridge {
        private final Object minecraft;
        private final ClassLoader gameClassLoader;
        private final Method execute;
        private final Field playerField;
        private final Field screenField;
        private final Field inputField;
        private final Method turn;
        private final Method setSprinting;
        private final Method startAttack;
        private final Method continueAttack;
        private final Method startUseItem;
        private final Method stopUsingItem;
        private final Object useKeyMapping;
        private final Method setUseKeyDown;
        private final Method pauseGame;
        private final Method screenOnClose;
        private final Method getInventory;
        private final Method getSelectedSlot;
        private final Method setSelectedSlot;
        private final Field selectedSlotField;
        private final Method setScreen;
        private final Constructor<?> inventoryScreenConstructor;
        private final InputWriter inputWriter;
        private final Field xxaField;
        private final Field zzaField;
        private final Field jumpingField;
        private final Method setJumping;
        private final Method jumpFromGround;
        private final Method onGround;
        private boolean attackHeld;
        private boolean useHeld;
        private long nextAttackContinueAt;
        private boolean jumpHeld;
        private boolean movementSampleLogged;

        private MinecraftBridge(
            Object minecraft,
            ClassLoader gameClassLoader,
            Method execute,
            Field playerField,
            Field screenField,
            Field inputField,
            Method turn,
            Method setSprinting,
            Method startAttack,
            Method continueAttack,
            Method startUseItem,
            Method stopUsingItem,
            Object useKeyMapping,
            Method setUseKeyDown,
            Method pauseGame,
            Method screenOnClose,
            Method getInventory,
            Method getSelectedSlot,
            Method setSelectedSlot,
            Field selectedSlotField,
            Method setScreen,
            Constructor<?> inventoryScreenConstructor,
            InputWriter inputWriter,
            Field xxaField,
            Field zzaField,
            Field jumpingField,
            Method setJumping,
            Method jumpFromGround,
            Method onGround
        ) {
            this.minecraft = minecraft;
            this.gameClassLoader = gameClassLoader;
            this.execute = execute;
            this.playerField = playerField;
            this.screenField = screenField;
            this.inputField = inputField;
            this.turn = turn;
            this.setSprinting = setSprinting;
            this.startAttack = startAttack;
            this.continueAttack = continueAttack;
            this.startUseItem = startUseItem;
            this.stopUsingItem = stopUsingItem;
            this.useKeyMapping = useKeyMapping;
            this.setUseKeyDown = setUseKeyDown;
            this.pauseGame = pauseGame;
            this.screenOnClose = screenOnClose;
            this.getInventory = getInventory;
            this.getSelectedSlot = getSelectedSlot;
            this.setSelectedSlot = setSelectedSlot;
            this.selectedSlotField = selectedSlotField;
            this.setScreen = setScreen;
            this.inventoryScreenConstructor = inventoryScreenConstructor;
            this.inputWriter = inputWriter;
            this.xxaField = xxaField;
            this.zzaField = zzaField;
            this.jumpingField = jumpingField;
            this.setJumping = setJumping;
            this.jumpFromGround = jumpFromGround;
            this.onGround = onGround;
        }

        static MinecraftBridge tryCreate(Instrumentation instrumentation, RubyMappings mappings) throws Exception {
            String minecraftRuntimeName = mappings.className("net.minecraft.client.Minecraft");
            Class<?> minecraftClass = findLoadedClass(instrumentation, minecraftRuntimeName);
            if (minecraftClass == null) {
                return null;
            }
            ClassLoader gameClassLoader = minecraftClass.getClassLoader();

            Method getInstance = findMethod(
                minecraftClass,
                mappings.methodName("net.minecraft.client.Minecraft", "getInstance", 0),
                0
            );
            Object minecraft = null;
            if (getInstance != null && Modifier.isStatic(getInstance.getModifiers())) {
                minecraft = getInstance.invoke(null);
            }
            if (minecraft == null) {
                Field instanceField = optionalField(
                    minecraftClass,
                    mappings.fieldName("net.minecraft.client.Minecraft", "instance")
                );
                if (instanceField == null) {
                    instanceField = findStaticFieldAssignableTo(minecraftClass, minecraftClass);
                }
                if (instanceField != null) {
                    minecraft = instanceField.get(null);
                }
            }
            if (minecraft == null) {
                return null;
            }

            Method execute = findRunnableExecutorMethod(minecraftClass);

            // Resolve the mapped fields first. This avoids forcing LocalPlayer,
            // Screen and Input classes through the agent thread before Minecraft
            // has naturally loaded them.
            Field player = optionalField(
                minecraftClass,
                mappings.fieldName("net.minecraft.client.Minecraft", "player")
            );
            if (player == null) {
                Class<?> mappedLocalPlayer = findClass(
                    instrumentation,
                    mappings.className("net.minecraft.client.player.LocalPlayer"),
                    gameClassLoader
                );
                if (mappedLocalPlayer != null) {
                    player = findInstanceFieldAssignableTo(minecraftClass, mappedLocalPlayer);
                }
            }
            if (player == null) {
                throw new NoSuchFieldException("Minecraft player field");
            }

            Object currentPlayer = player.get(minecraft);
            if (currentPlayer == null) {
                return null;
            }
            Class<?> localPlayerClass = currentPlayer.getClass();

            Class<?> screenClass = findClass(
                instrumentation,
                mappings.className("net.minecraft.client.gui.screens.Screen"),
                gameClassLoader
            );
            Field screen = optionalField(
                minecraftClass,
                mappings.fieldName("net.minecraft.client.Minecraft", "screen")
            );
            if (screen == null && screenClass != null) {
                screen = findInstanceFieldAssignableTo(minecraftClass, screenClass);
            }
            if (screen == null) {
                throw new NoSuchFieldException("Minecraft screen field");
            }

            Field input = optionalField(
                localPlayerClass,
                mappings.fieldName("net.minecraft.client.player.LocalPlayer", "input")
            );
            if (input == null) {
                Class<?> mappedInput = findClass(
                    instrumentation,
                    mappings.className("net.minecraft.client.player.Input"),
                    gameClassLoader
                );
                if (mappedInput != null) {
                    input = findInstanceFieldAssignableTo(localPlayerClass, mappedInput);
                }
            }
            if (input == null) {
                throw new NoSuchFieldException("LocalPlayer input field");
            }

            String mappedTurn = mappings.methodName("net.minecraft.world.entity.Entity", "turn", 2);
            Method turn = findNamedMethodByParameters(
                localPlayerClass, mappedTurn, void.class, double.class, double.class
            );
            if (turn == null) {
                turn = findNamedMethodByParameters(
                    localPlayerClass, mappedTurn, void.class, float.class, float.class
                );
            }
            if (turn == null) {
                turn = findMethodByParameters(localPlayerClass, void.class, double.class, double.class);
            }
            if (turn == null) {
                turn = findMethodByParameters(localPlayerClass, void.class, float.class, float.class);
            }
            if (turn == null) {
                throw new NoSuchMethodException("Entity turn(double,double) or turn(float,float)");
            }

            Method setSprinting = findNamedMethodByParameters(
                localPlayerClass,
                mappings.methodName("net.minecraft.world.entity.LivingEntity", "setSprinting", 1),
                void.class,
                boolean.class
            );

            Method startAttack = findNamedMethodByParameters(
                minecraftClass,
                mappings.methodName("net.minecraft.client.Minecraft", "startAttack", 0),
                boolean.class
            );
            if (startAttack == null) {
                startAttack = findNamedMethodByParameters(
                    minecraftClass,
                    mappings.methodName("net.minecraft.client.Minecraft", "startAttack", 0),
                    void.class
                );
            }
            Method continueAttack = findNamedMethodByParameters(
                minecraftClass,
                mappings.methodName("net.minecraft.client.Minecraft", "continueAttack", 1),
                void.class,
                boolean.class
            );
            Method startUseItem = findNamedMethodByParameters(
                minecraftClass,
                mappings.methodName("net.minecraft.client.Minecraft", "startUseItem", 0),
                void.class
            );
            Method pauseGame = findNamedMethodByParameters(
                minecraftClass,
                mappings.methodName("net.minecraft.client.Minecraft", "pauseGame", 1),
                void.class,
                boolean.class
            );

            Method stopUsingItem = findNamedMethodByParameters(
                localPlayerClass,
                mappings.methodName("net.minecraft.client.player.LocalPlayer", "stopUsingItem", 0),
                void.class
            );
            if (stopUsingItem == null) {
                stopUsingItem = findNamedMethodByParameters(
                    localPlayerClass,
                    mappings.methodName("net.minecraft.world.entity.LivingEntity", "stopUsingItem", 0),
                    void.class
                );
            }

            Object useKeyMapping = null;
            Method setUseKeyDown = null;
            try {
                Field optionsField = optionalField(
                    minecraftClass,
                    mappings.fieldName("net.minecraft.client.Minecraft", "options")
                );
                Object options = optionsField == null ? null : optionsField.get(minecraft);
                if (options != null) {
                    Field keyUseField = optionalField(
                        options.getClass(),
                        mappings.fieldName("net.minecraft.client.Options", "keyUse")
                    );
                    useKeyMapping = keyUseField == null ? null : keyUseField.get(options);
                    if (useKeyMapping != null) {
                        setUseKeyDown = findNamedMethodByParameters(
                            useKeyMapping.getClass(),
                            mappings.methodName("net.minecraft.client.KeyMapping", "setDown", 1),
                            void.class,
                            boolean.class
                        );
                        if (setUseKeyDown == null) {
                            setUseKeyDown = findNamedMethodByParameters(
                                useKeyMapping.getClass(),
                                "setDown",
                                void.class,
                                boolean.class
                            );
                        }
                    }
                }
            } catch (Throwable error) {
                log("Held-use key binding unavailable; single-press Use fallback will be used: " + concise(error));
                useKeyMapping = null;
                setUseKeyDown = null;
            }

            Method onClose = screenClass == null ? null : findNamedMethodByParameters(
                screenClass,
                mappings.methodName("net.minecraft.client.gui.screens.Screen", "onClose", 0),
                void.class
            );

            Method getInventory = null;
            Method getSelectedSlot = null;
            Method setSelectedSlot = null;
            Field selectedSlotField = null;
            try {
                getInventory = findNamedZeroArgumentMethod(
                    localPlayerClass,
                    mappings.methodName("net.minecraft.world.entity.player.Player", "getInventory", 0)
                );
                if (getInventory != null) {
                    Object inventory = getInventory.invoke(currentPlayer);
                    if (inventory != null) {
                        Class<?> inventoryClass = inventory.getClass();
                        getSelectedSlot = findNamedMethodByParameters(
                            inventoryClass,
                            mappings.methodName("net.minecraft.world.entity.player.Inventory", "getSelectedSlot", 0),
                            int.class
                        );
                        setSelectedSlot = findNamedMethodByParameters(
                            inventoryClass,
                            mappings.methodName("net.minecraft.world.entity.player.Inventory", "setSelectedSlot", 1),
                            void.class,
                            int.class
                        );
                        selectedSlotField = optionalField(
                            inventoryClass,
                            mappings.fieldName("net.minecraft.world.entity.player.Inventory", "selected")
                        );
                    }
                }
            } catch (Throwable error) {
                log("Hotbar integration unavailable: " + concise(error));
            }

            Method setScreen = screenClass == null ? null : findNamedCompatibleSingleArgumentMethod(
                minecraftClass,
                mappings.methodName("net.minecraft.client.Minecraft", "setScreen", 1),
                screenClass,
                void.class
            );
            Constructor<?> inventoryScreenConstructor = null;
            try {
                Class<?> inventoryScreenClass = findClass(
                    instrumentation,
                    mappings.className("net.minecraft.client.gui.screens.inventory.InventoryScreen"),
                    gameClassLoader
                );
                if (inventoryScreenClass != null) {
                    inventoryScreenConstructor = findAssignableSingleArgumentConstructor(
                        inventoryScreenClass,
                        localPlayerClass
                    );
                }
            } catch (Throwable error) {
                log("Inventory-screen integration unavailable: " + concise(error));
            }

            Field xxaField = requireFieldType(
                localPlayerClass,
                mappings.fieldName("net.minecraft.world.entity.LivingEntity", "xxa"),
                float.class
            );
            Field zzaField = requireFieldType(
                localPlayerClass,
                mappings.fieldName("net.minecraft.world.entity.LivingEntity", "zza"),
                float.class
            );
            Field jumpingField = requireFieldType(
                localPlayerClass,
                mappings.fieldName("net.minecraft.world.entity.LivingEntity", "jumping"),
                boolean.class
            );
            Method setJumping = findNamedMethodByParameters(
                localPlayerClass,
                mappings.methodName("net.minecraft.world.entity.LivingEntity", "setJumping", 1),
                void.class,
                boolean.class
            );
            Method jumpFromGround = findNamedMethodByParameters(
                localPlayerClass,
                mappings.methodName("net.minecraft.world.entity.LivingEntity", "jumpFromGround", 0),
                void.class
            );
            Method onGround = findNamedMethodByParameters(
                localPlayerClass,
                mappings.methodName("net.minecraft.world.entity.Entity", "onGround", 0),
                boolean.class
            );

            InputWriter writer = InputWriter.create(
                instrumentation,
                mappings,
                input.getType(),
                gameClassLoader
            );
            List<String> capabilities = new ArrayList<>();
            capabilities.add("analog movement");
            capabilities.add("camera");
            if (setSprinting != null) capabilities.add("sprint");
            if (startAttack != null && continueAttack != null) capabilities.add("attack/mine");
            if (startUseItem != null) capabilities.add(setUseKeyDown != null ? "held use/place" : "single-press use/place fallback");
            if (pauseGame != null) capabilities.add("pause");
            if (getInventory != null
                && (selectedSlotField != null || (getSelectedSlot != null && setSelectedSlot != null))) {
                capabilities.add("hotbar cycling");
            }
            if (setScreen != null && inventoryScreenConstructor != null) capabilities.add("inventory open");

            log("Minecraft action bridge resolved for " + minecraftClass.getName()
                + " using " + gameClassLoader + ".");
            log("Available direct-input capabilities: " + String.join(", ", capabilities) + ".");
            return new MinecraftBridge(
                minecraft, gameClassLoader, execute, player, screen, input, turn, setSprinting,
                startAttack, continueAttack, startUseItem, stopUsingItem,
                useKeyMapping, setUseKeyDown, pauseGame,
                onClose, getInventory, getSelectedSlot, setSelectedSlot, selectedSlotField,
                setScreen, inventoryScreenConstructor, writer,
                xxaField, zzaField, jumpingField, setJumping, jumpFromGround, onGround
            );
        }

        ClassLoader classLoader() {
            return gameClassLoader;
        }

        boolean isGameplayReady() {
            try {
                return playerField.get(minecraft) != null;
            } catch (Throwable ignored) {
                return false;
            }
        }

        void execute(Runnable runnable) throws Exception {
            execute.invoke(minecraft, runnable);
        }

        void applyMovementAfterKeyboard(Object input, ControllerFrame frame) throws Exception {
            Object player = playerField.get(minecraft);
            Object screen = screenField.get(minecraft);
            if (player == null || screen != null || !frame.connected) {
                if (input != null) inputWriter.clear(input);
                return;
            }
            inputWriter.write(
                input,
                frame.strafe,
                frame.forward,
                frame.button(ControllerRuntime.BUTTON_A),
                frame.button(ControllerRuntime.BUTTON_B),
                false
            );
            writeDirectMovement(player, frame);
        }

        void apply(ControllerFrame frame, ControllerSettings settings) throws Exception {
            Object player = playerField.get(minecraft);
            Object screen = screenField.get(minecraft);

            if (player == null || !frame.connected) {
                releaseActions(player);
                return;
            }

            if (screen != null) {
                pauseForScreen();
                return;
            }

            if ((frame.pressed(ControllerRuntime.BUTTON_Y)
                    || frame.pressed(ControllerRuntime.BUTTON_X))
                && setScreen != null
                && inventoryScreenConstructor != null) {
                Object inventoryScreen = inventoryScreenConstructor.newInstance(player);
                setScreen.invoke(minecraft, inventoryScreen);
                inputWriter.clear(inputField.get(player));
                releaseActions(player);
                return;
            }

            Object liveInput = inputField.get(player);
            if (liveInput != null) {
                inputWriter.write(
                    liveInput,
                    frame.strafe,
                    frame.forward,
                    frame.button(ControllerRuntime.BUTTON_A),
                    frame.button(ControllerRuntime.BUTTON_B),
                    false
                );
            }
            writeDirectMovement(player, frame);
            boolean jump = frame.button(ControllerRuntime.BUTTON_A);
            if (jump && !jumpHeld && jumpFromGround != null) {
                boolean grounded = onGround == null || (Boolean) onGround.invoke(player);
                if (grounded) jumpFromGround.invoke(player);
            }
            jumpHeld = jump;

            if (!movementSampleLogged && (Math.abs(frame.forward) > 0.50f || Math.abs(frame.strafe) > 0.50f)) {
                movementSampleLogged = true;
                log(String.format(Locale.ROOT,
                    "Analog movement active: forward=%.3f, strafe=%.3f; post-keyboard hook=%s.",
                    frame.forward, frame.strafe, RubyInputHooks.transformed() ? "installed" : "waiting"));
            }

            float lookX = responseCurve(frame.lookX) * 8.0f * settings.lookSensitivity();
            float lookY = responseCurve(frame.lookY) * 8.0f * settings.lookSensitivity();
            if (settings.invertVerticalLook()) lookY = -lookY;
            if (Math.abs(lookX) > 0.0001f || Math.abs(lookY) > 0.0001f) {
                invokeNumericPair(turn, player, lookX, lookY);
            }

            if (frame.sprintPressed && frame.forward > 0.20f && setSprinting != null) {
                setSprinting.invoke(player, true);
            }

            long actionNow = System.nanoTime();
            boolean attack = frame.rightTrigger >= settings.triggerThreshold();
            if (startAttack != null && continueAttack != null) {
                if (attack && !attackHeld) {
                    // Begin exactly once. Continuing block damage is paced at the
                    // Minecraft client-tick rate instead of the 125 Hz poll rate.
                    startAttack.invoke(minecraft);
                    continueAttack.invoke(minecraft, true);
                    nextAttackContinueAt = actionNow + 50_000_000L;
                } else if (attack && actionNow >= nextAttackContinueAt) {
                    continueAttack.invoke(minecraft, true);
                    nextAttackContinueAt = actionNow + 50_000_000L;
                } else if (!attack && attackHeld) {
                    continueAttack.invoke(minecraft, false);
                }
                attackHeld = attack;
            }

            boolean use = frame.leftTrigger >= settings.triggerThreshold();
            if (setUseKeyDown != null && useKeyMapping != null) {
                // Keep Minecraft's own Use key held for the full trigger press.
                // Minecraft's normal client tick owns eating, shielding, charging
                // and its built-in placement repeat/cooldown behavior.
                setUseKeyDown.invoke(useKeyMapping, use);
            }
            if (use && !useHeld && startUseItem != null) {
                // Begin the action once on the rising edge. Do not manually repeat:
                // repeated startUseItem calls can place several blocks per trigger pull.
                startUseItem.invoke(minecraft);
            } else if (!use && useHeld && stopUsingItem != null) {
                stopUsingItem.invoke(player);
            }
            useHeld = use;

            if (frame.pressed(ControllerRuntime.BUTTON_LB)) {
                cycleHotbar(player, -1);
            }
            if (frame.pressed(ControllerRuntime.BUTTON_RB)) {
                cycleHotbar(player, 1);
            }
            if (frame.pressed(ControllerRuntime.BUTTON_START) && pauseGame != null) {
                pauseGame.invoke(minecraft, false);
            }
        }

        void pauseForScreen() throws Exception {
            Object player = playerField.get(minecraft);
            releaseActions(player);
        }

        private void writeDirectMovement(Object player, ControllerFrame frame) throws Exception {
            if (player == null) return;
            xxaField.setFloat(player, frame.strafe);
            zzaField.setFloat(player, frame.forward);
            boolean jump = frame.button(ControllerRuntime.BUTTON_A);
            jumpingField.setBoolean(player, jump);
            if (setJumping != null) setJumping.invoke(player, jump);
        }

        private void cycleHotbar(Object player, int amount) throws Exception {
            if (getInventory == null || (selectedSlotField == null && (getSelectedSlot == null || setSelectedSlot == null))) {
                return;
            }
            Object inventory = getInventory.invoke(player);
            int selected;
            if (getSelectedSlot != null) {
                selected = ((Number) getSelectedSlot.invoke(inventory)).intValue();
            } else {
                selected = selectedSlotField.getInt(inventory);
            }

            int next = Math.floorMod(selected + amount, 9);
            if (setSelectedSlot != null) {
                setSelectedSlot.invoke(inventory, next);
            } else {
                selectedSlotField.setInt(inventory, next);
            }
        }

        private void releaseActions(Object player) throws Exception {
            if (attackHeld && continueAttack != null) {
                continueAttack.invoke(minecraft, false);
                attackHeld = false;
            }
            if (setUseKeyDown != null && useKeyMapping != null) {
                setUseKeyDown.invoke(useKeyMapping, false);
            }
            if (useHeld && player != null && stopUsingItem != null) {
                stopUsingItem.invoke(player);
            }
            useHeld = false;
            if (player != null) {
                Object input = inputField.get(player);
                if (input != null) inputWriter.clear(input);
                xxaField.setFloat(player, 0f);
                zzaField.setFloat(player, 0f);
                jumpingField.setBoolean(player, false);
                if (setJumping != null) setJumping.invoke(player, false);
            }
            jumpHeld = false;
        }
    }

    private interface InputWriter {
        void write(Object input, float strafe, float forward, boolean jump, boolean sneak, boolean sprint) throws Exception;
        void clear(Object input) throws Exception;

        static InputWriter create(
            Instrumentation instrumentation,
            RubyMappings mappings,
            Class<?> runtimeInputClass,
            ClassLoader gameClassLoader
        ) throws Exception {
            String modernName = mappings.className("net.minecraft.client.player.ClientInput");
            if (runtimeInputClass.getName().equals(modernName)) {
                return new ModernInputWriter(instrumentation, mappings, runtimeInputClass, gameClassLoader);
            }
            return new ClassicInputWriter(mappings, runtimeInputClass);
        }
    }

    private static final class ClassicInputWriter implements InputWriter {
        private final Field leftImpulse;
        private final Field forwardImpulse;
        private final Field up;
        private final Field down;
        private final Field left;
        private final Field right;
        private final Field jumping;
        private final Field shiftKeyDown;

        private ClassicInputWriter(RubyMappings mappings, Class<?> inputClass) throws Exception {
            String owner = "net.minecraft.client.player.Input";
            leftImpulse = requireFieldType(inputClass, mappings.fieldName(owner, "leftImpulse"), float.class);
            forwardImpulse = requireFieldType(inputClass, mappings.fieldName(owner, "forwardImpulse"), float.class);
            up = requireFieldType(inputClass, mappings.fieldName(owner, "up"), boolean.class);
            down = requireFieldType(inputClass, mappings.fieldName(owner, "down"), boolean.class);
            left = requireFieldType(inputClass, mappings.fieldName(owner, "left"), boolean.class);
            right = requireFieldType(inputClass, mappings.fieldName(owner, "right"), boolean.class);
            jumping = requireFieldType(inputClass, mappings.fieldName(owner, "jumping"), boolean.class);
            shiftKeyDown = requireFieldType(inputClass, mappings.fieldName(owner, "shiftKeyDown"), boolean.class);
            log("Using classic analog Input fields.");
        }

        @Override
        public void write(Object input, float strafe, float forward, boolean jump, boolean sneak, boolean sprint) throws Exception {
            leftImpulse.setFloat(input, strafe);
            forwardImpulse.setFloat(input, forward);
            up.setBoolean(input, forward > 0.25f);
            down.setBoolean(input, forward < -0.25f);
            left.setBoolean(input, strafe > 0.25f);
            right.setBoolean(input, strafe < -0.25f);
            jumping.setBoolean(input, jump);
            shiftKeyDown.setBoolean(input, sneak);
        }

        @Override
        public void clear(Object input) throws Exception {
            write(input, 0f, 0f, false, false, false);
        }
    }

    private static final class ModernInputWriter implements InputWriter {
        private final Field keyPresses;
        private final Field moveVector;
        private final Constructor<?> inputConstructor;
        private final Constructor<?> vectorConstructor;

        private ModernInputWriter(
            Instrumentation instrumentation,
            RubyMappings mappings,
            Class<?> clientInputClass,
            ClassLoader gameClassLoader
        ) throws Exception {
            String owner = "net.minecraft.client.player.ClientInput";
            keyPresses = requireField(clientInputClass, mappings.fieldName(owner, "keyPresses"));
            moveVector = requireField(clientInputClass, mappings.fieldName(owner, "moveVector"));

            Class<?> inputRecord = requireClass(
                instrumentation,
                mappings.className("net.minecraft.world.entity.player.Input"),
                gameClassLoader
            );
            inputConstructor = findBooleanConstructor(inputRecord);
            Class<?> vec2 = requireClass(
                instrumentation,
                mappings.className("net.minecraft.world.phys.Vec2"),
                gameClassLoader
            );
            vectorConstructor = findNumericPairConstructor(vec2);
            log("Using modern ClientInput records with true analog move vectors.");
        }

        @Override
        public void write(Object input, float strafe, float forward, boolean jump, boolean sneak, boolean sprint) throws Exception {
            boolean forwardPressed = forward > 0.25f;
            boolean backwardPressed = forward < -0.25f;
            boolean leftPressed = strafe > 0.25f;
            boolean rightPressed = strafe < -0.25f;

            Object keyState;
            int count = inputConstructor.getParameterCount();
            if (count >= 7) {
                keyState = inputConstructor.newInstance(
                    forwardPressed, backwardPressed, leftPressed, rightPressed,
                    jump, sneak, sprint
                );
            } else {
                keyState = inputConstructor.newInstance(
                    forwardPressed, backwardPressed, leftPressed, rightPressed,
                    jump, sneak
                );
            }
            Class<?> vectorType = vectorConstructor.getParameterTypes()[0];
            Object vector = vectorType == double.class
                ? vectorConstructor.newInstance((double) strafe, (double) forward)
                : vectorConstructor.newInstance(strafe, forward);
            keyPresses.set(input, keyState);
            moveVector.set(input, vector);
        }

        @Override
        public void clear(Object input) throws Exception {
            write(input, 0f, 0f, false, false, false);
        }
    }

    /**
     * Module-owned settings. Ruby Module Loader renders these values, while the
     * controller module remains responsible for validation, persistence and
     * applying changes immediately.
     */
    private static final class ControllerSettings {
        private static final List<Choice<String>> MOVEMENT_MODES = List.of(
            new Choice<>("classic", "Classic")
        );
        private static final List<Choice<Float>> DEADZONES = List.of(
            new Choice<>(0.08f, "8% — Very small"),
            new Choice<>(0.12f, "12% — Small"),
            new Choice<>(0.18f, "18% — Recommended"),
            new Choice<>(0.24f, "24% — Large"),
            new Choice<>(0.30f, "30% — Very large")
        );
        private static final List<Choice<Float>> LOOK_SPEEDS = List.of(
            new Choice<>(0.60f, "Slow"),
            new Choice<>(0.80f, "A little slow"),
            new Choice<>(1.00f, "Normal"),
            new Choice<>(1.25f, "Fast"),
            new Choice<>(1.50f, "Very fast"),
            new Choice<>(2.00f, "Maximum")
        );
        private static final List<Choice<Float>> TRIGGER_LEVELS = List.of(
            new Choice<>(0.15f, "Light"),
            new Choice<>(0.25f, "Easy"),
            new Choice<>(0.35f, "Normal"),
            new Choice<>(0.50f, "Firm")
        );
        private static final List<Choice<Boolean>> ON_OFF = List.of(
            new Choice<>(Boolean.TRUE, "On"),
            new Choice<>(Boolean.FALSE, "Off")
        );
        private static final List<Choice<Boolean>> LOOK_DIRECTION = List.of(
            new Choice<>(Boolean.FALSE, "Standard"),
            new Choice<>(Boolean.TRUE, "Inverted")
        );
        private static final List<Choice<String>> MENU_SPEEDS = List.of(
            new Choice<>("slow", "Slow"),
            new Choice<>("normal", "Normal"),
            new Choice<>("fast", "Fast")
        );
        private static final List<Choice<Float>> CURSOR_SPEEDS = List.of(
            new Choice<>(120.0f, "Slow"),
            new Choice<>(180.0f, "A little slow"),
            new Choice<>(240.0f, "Normal"),
            new Choice<>(320.0f, "Fast"),
            new Choice<>(420.0f, "Very fast")
        );

        private final Path settingsFile;
        private final RubySettingsPage page;
        private volatile String movementMode;
        private volatile float deadzone;
        private volatile float lookSensitivity;
        private volatile float triggerThreshold;
        private volatile boolean invertVerticalLook;
        private volatile boolean menuNavigation;
        private volatile String menuRepeatSpeed;
        private volatile boolean inventoryCursor;
        private volatile float cursorSpeed;

        private ControllerSettings(Path gameDirectory) {
            Path base = gameDirectory == null
                ? Path.of(".").toAbsolutePath().normalize()
                : gameDirectory.toAbsolutePath().normalize();
            settingsFile = base.resolve("ruby").resolve("controller.properties");

            movementMode = normalizeMovementMode(
                System.getProperty("ruby.controller.movementMode", "classic")
            );
            deadzone = closestFloat(
                parseStoredFloat("ruby.controller.deadzone", 0.18f), DEADZONES
            );
            lookSensitivity = closestFloat(
                parseStoredFloat("ruby.controller.lookSensitivity", 1.00f), LOOK_SPEEDS
            );
            triggerThreshold = closestFloat(
                parseStoredFloat("ruby.controller.triggerThreshold", 0.35f), TRIGGER_LEVELS
            );
            invertVerticalLook = Boolean.parseBoolean(
                System.getProperty("ruby.controller.invertVerticalLook", "false")
            );
            menuNavigation = Boolean.parseBoolean(
                System.getProperty("ruby.controller.menuNavigation", "true")
            );
            menuRepeatSpeed = normalizeMenuSpeed(
                System.getProperty("ruby.controller.menuRepeatSpeed", "normal")
            );
            inventoryCursor = Boolean.parseBoolean(
                System.getProperty("ruby.controller.inventoryCursor", "true")
            );
            cursorSpeed = closestFloat(
                parseStoredFloat("ruby.controller.cursorSpeed", 240.0f), CURSOR_SPEEDS
            );

            loadFile();
            page = new RubySettingsPage(
                "ruby-controller-options",
                RubySettingsLocation.CONTROLS,
                "Controller Options...",
                "Controller Options",
                "Press A to change a value. Changes apply immediately and are saved for this Minecraft profile.",
                List.of(
                    lockedChoiceSetting(
                        "movement-mode",
                        "Movement",
                        "Classic preserves Ruby's original direct analog movement. Modern movement is planned but not available yet.",
                        MOVEMENT_MODES,
                        this::movementMode
                    ),
                    choiceSetting(
                        "left-stick-deadzone",
                        "Left Stick Deadzone",
                        "How far the left stick must move before walking starts. Raise this if your character drifts.",
                        DEADZONES,
                        this::deadzone,
                        value -> deadzone = value
                    ),
                    choiceSetting(
                        "camera-speed",
                        "Camera Speed",
                        "How quickly the right stick turns the camera.",
                        LOOK_SPEEDS,
                        this::lookSensitivity,
                        value -> lookSensitivity = value
                    ),
                    choiceSetting(
                        "vertical-camera",
                        "Vertical Camera Direction",
                        "Standard looks up when you push up. Inverted does the opposite.",
                        LOOK_DIRECTION,
                        this::invertVerticalLook,
                        value -> invertVerticalLook = value
                    ),
                    choiceSetting(
                        "trigger-pressure",
                        "Trigger Pressure",
                        "How far a trigger must be pulled before mining, attacking, placing or using begins.",
                        TRIGGER_LEVELS,
                        this::triggerThreshold,
                        value -> triggerThreshold = value
                    ),
                    choiceSetting(
                        "menu-navigation",
                        "Controller Menu Navigation",
                        "Lets the left stick and D-pad move between Minecraft menu buttons.",
                        ON_OFF,
                        this::menuNavigation,
                        value -> menuNavigation = value
                    ),
                    choiceSetting(
                        "menu-repeat-speed",
                        "Menu Repeat Speed",
                        "How quickly menu focus keeps moving while a direction is held.",
                        MENU_SPEEDS,
                        this::menuRepeatSpeed,
                        value -> menuRepeatSpeed = value
                    ),
                    choiceSetting(
                        "inventory-cursor",
                        "GUI Crosshair Cursor",
                        "Shows a controller cursor in inventories, block screens and the world-selection screen.",
                        ON_OFF,
                        this::inventoryCursor,
                        value -> inventoryCursor = value
                    ),
                    choiceSetting(
                        "inventory-cursor-speed",
                        "GUI Cursor Speed",
                        "How quickly the controller cursor moves across supported Minecraft screens.",
                        CURSOR_SPEEDS,
                        this::cursorSpeed,
                        value -> cursorSpeed = value
                    )
                )
            );
        }

        static ControllerSettings load(Path gameDirectory) {
            ControllerSettings settings = new ControllerSettings(gameDirectory);
            log("Controller settings loaded from " + settings.settingsFile + ".");
            return settings;
        }

        RubySettingsPage page() { return page; }
        String movementMode() { return movementMode; }
        float deadzone() { return deadzone; }
        float lookSensitivity() { return lookSensitivity; }
        float triggerThreshold() { return triggerThreshold; }
        boolean invertVerticalLook() { return invertVerticalLook; }
        boolean menuNavigation() { return menuNavigation; }
        String menuRepeatSpeed() { return menuRepeatSpeed; }
        boolean inventoryCursor() { return inventoryCursor; }
        float cursorSpeed() { return cursorSpeed; }

        long menuInitialDelayNanos() {
            return switch (menuRepeatSpeed) {
                case "slow" -> 500_000_000L;
                case "fast" -> 250_000_000L;
                default -> 350_000_000L;
            };
        }

        long menuRepeatNanos() {
            return switch (menuRepeatSpeed) {
                case "slow" -> 180_000_000L;
                case "fast" -> 85_000_000L;
                default -> 125_000_000L;
            };
        }

        private <T> RubySetting lockedChoiceSetting(
            String id,
            String name,
            String description,
            List<Choice<T>> choices,
            Supplier<T> getter
        ) {
            return new RubySetting() {
                @Override public String id() { return id; }
                @Override public String name() { return name; }
                @Override public String description() { return description; }
                @Override public boolean active() { return false; }

                @Override
                public String valueLabel() {
                    T value = getter.get();
                    for (Choice<T> choice : choices) {
                        if (Objects.equals(choice.value(), value)) return choice.label();
                    }
                    return String.valueOf(value);
                }

                @Override
                public void cycle(int direction) {
                    // Groundwork only: Modern movement is intentionally locked.
                }
            };
        }

        private <T> RubySetting choiceSetting(
            String id,
            String name,
            String description,
            List<Choice<T>> choices,
            Supplier<T> getter,
            Consumer<T> setter
        ) {
            return new RubySetting() {
                @Override public String id() { return id; }
                @Override public String name() { return name; }
                @Override public String description() { return description; }

                @Override
                public String valueLabel() {
                    T value = getter.get();
                    for (Choice<T> choice : choices) {
                        if (Objects.equals(choice.value(), value)) return choice.label();
                    }
                    return String.valueOf(value);
                }

                @Override
                public void cycle(int direction) {
                    T current = getter.get();
                    int index = 0;
                    for (int candidate = 0; candidate < choices.size(); candidate++) {
                        if (Objects.equals(choices.get(candidate).value(), current)) {
                            index = candidate;
                            break;
                        }
                    }
                    int next = Math.floorMod(index + (direction < 0 ? -1 : 1), choices.size());
                    setter.accept(choices.get(next).value());
                    save();
                    log(name + " changed to " + choices.get(next).label() + ".");
                }
            };
        }

        private void loadFile() {
            if (!Files.isRegularFile(settingsFile)) return;
            Properties properties = new Properties();
            try (InputStream input = Files.newInputStream(settingsFile)) {
                properties.load(input);
                movementMode = normalizeMovementMode(
                    properties.getProperty("movementMode", movementMode)
                );
                deadzone = closestFloat(
                    parsePropertyFloat(properties, "leftStickDeadzone", deadzone), DEADZONES
                );
                lookSensitivity = closestFloat(
                    parsePropertyFloat(properties, "lookSensitivity", lookSensitivity), LOOK_SPEEDS
                );
                triggerThreshold = closestFloat(
                    parsePropertyFloat(properties, "triggerThreshold", triggerThreshold), TRIGGER_LEVELS
                );
                invertVerticalLook = Boolean.parseBoolean(
                    properties.getProperty("invertVerticalLook", Boolean.toString(invertVerticalLook))
                );
                menuNavigation = Boolean.parseBoolean(
                    properties.getProperty("menuNavigation", Boolean.toString(menuNavigation))
                );
                menuRepeatSpeed = normalizeMenuSpeed(
                    properties.getProperty("menuRepeatSpeed", menuRepeatSpeed)
                );
                inventoryCursor = Boolean.parseBoolean(
                    properties.getProperty("inventoryCursor", Boolean.toString(inventoryCursor))
                );
                cursorSpeed = closestFloat(
                    parsePropertyFloat(properties, "cursorSpeed", cursorSpeed), CURSOR_SPEEDS
                );
            } catch (Throwable error) {
                log("Could not read controller settings; recommended values will be used: " + concise(error));
            }
        }

        private synchronized void save() {
            Properties properties = new Properties();
            properties.setProperty("movementMode", movementMode);
            properties.setProperty("leftStickDeadzone", Float.toString(deadzone));
            properties.setProperty("lookSensitivity", Float.toString(lookSensitivity));
            properties.setProperty("triggerThreshold", Float.toString(triggerThreshold));
            properties.setProperty("invertVerticalLook", Boolean.toString(invertVerticalLook));
            properties.setProperty("menuNavigation", Boolean.toString(menuNavigation));
            properties.setProperty("menuRepeatSpeed", menuRepeatSpeed);
            properties.setProperty("inventoryCursor", Boolean.toString(inventoryCursor));
            properties.setProperty("cursorSpeed", Float.toString(cursorSpeed));
            try {
                Files.createDirectories(settingsFile.getParent());
                try (OutputStream output = Files.newOutputStream(settingsFile)) {
                    properties.store(output, "Ruby Controller Support settings");
                }
            } catch (Throwable error) {
                log("Could not save controller settings: " + concise(error));
            }
        }

        private float parseStoredFloat(String systemProperty, float fallback) {
            return parseFloat(systemProperty, fallback);
        }

        private static float parsePropertyFloat(Properties properties, String name, float fallback) {
            try {
                return Float.parseFloat(properties.getProperty(name, Float.toString(fallback)));
            } catch (NumberFormatException ignored) {
                return fallback;
            }
        }

        private static float closestFloat(float requested, List<Choice<Float>> choices) {
            Choice<Float> best = choices.get(0);
            float distance = Math.abs(requested - best.value());
            for (Choice<Float> choice : choices) {
                float candidate = Math.abs(requested - choice.value());
                if (candidate < distance) {
                    best = choice;
                    distance = candidate;
                }
            }
            return best.value();
        }

        private static String normalizeMovementMode(String value) {
            // Modern is reserved in the settings format but deliberately locked
            // until its movement implementation exists.
            return "classic";
        }

        private static String normalizeMenuSpeed(String value) {
            if (value == null) return "normal";
            return switch (value.toLowerCase(Locale.ROOT)) {
                case "slow" -> "slow";
                case "fast" -> "fast";
                default -> "normal";
            };
        }

        private record Choice<T>(T value, String label) {}
    }

    private static final class GlfwBridge {
        private final Method joystickPresent;
        private final Method joystickIsGamepad;
        private final Method getGamepadName;
        private final Method getJoystickName;
        private final Method getGamepadState;
        private final Method calloc;
        private final Method axes;
        private final Method buttons;
        private final Method free;
        private final Method setInputMode;
        private final Method setCursorPos;
        private int cursorMode = -1;

        private GlfwBridge(
            Method joystickPresent,
            Method joystickIsGamepad,
            Method getGamepadName,
            Method getJoystickName,
            Method getGamepadState,
            Method calloc,
            Method axes,
            Method buttons,
            Method free,
            Method setInputMode,
            Method setCursorPos
        ) {
            this.joystickPresent = joystickPresent;
            this.joystickIsGamepad = joystickIsGamepad;
            this.getGamepadName = getGamepadName;
            this.getJoystickName = getJoystickName;
            this.getGamepadState = getGamepadState;
            this.calloc = calloc;
            this.axes = axes;
            this.buttons = buttons;
            this.free = free;
            this.setInputMode = setInputMode;
            this.setCursorPos = setCursorPos;
        }

        static GlfwBridge tryCreate(Instrumentation instrumentation, ClassLoader preferredLoader) throws Exception {
            Class<?> glfw = findLoadedClass(instrumentation, "org.lwjgl.glfw.GLFW");
            if (glfw == null) {
                return null;
            }

            // Vanilla Minecraft loads GLFW itself, but it has no reason to load
            // GLFWGamepadState because Java Edition has no native controller path.
            // Waiting for that class to appear therefore deadlocks controller startup.
            Class<?> state = findLoadedClass(instrumentation, "org.lwjgl.glfw.GLFWGamepadState");
            if (state == null) {
                ClassLoader loader = glfw.getClassLoader() != null ? glfw.getClassLoader() : preferredLoader;
                state = Class.forName("org.lwjgl.glfw.GLFWGamepadState", false, loader);
            }
            GlfwBridge bridge = new GlfwBridge(
                glfw.getMethod("glfwJoystickPresent", int.class),
                glfw.getMethod("glfwJoystickIsGamepad", int.class),
                glfw.getMethod("glfwGetGamepadName", int.class),
                glfw.getMethod("glfwGetJoystickName", int.class),
                glfw.getMethod("glfwGetGamepadState", int.class, state),
                state.getMethod("calloc"),
                state.getMethod("axes", int.class),
                state.getMethod("buttons", int.class),
                state.getMethod("free"),
                glfw.getMethod("glfwSetInputMode", long.class, int.class, int.class),
                glfw.getMethod("glfwSetCursorPos", long.class, double.class, double.class)
            );
            log("LWJGL gamepad bridge resolved through " + glfw.getClassLoader() + ".");
            return bridge;
        }

        Object allocateState() {
            try {
                return calloc.invoke(null);
            } catch (Exception error) {
                throw new IllegalStateException("Could not allocate GLFW gamepad state.", error);
            }
        }

        void freeState(Object state) {
            try {
                free.invoke(state);
            } catch (Exception ignored) {
                // Process shutdown will reclaim it.
            }
        }

        boolean isGamepad(int id) {
            try {
                return (Boolean) joystickPresent.invoke(null, id)
                    && (Boolean) joystickIsGamepad.invoke(null, id);
            } catch (Exception ignored) {
                return false;
            }
        }

        String gamepadName(int id) {
            try { return (String) getGamepadName.invoke(null, id); }
            catch (Exception ignored) { return null; }
        }

        String joystickName(int id) {
            try { return (String) getJoystickName.invoke(null, id); }
            catch (Exception ignored) { return null; }
        }

        boolean readState(int id, Object state) {
            try { return (Boolean) getGamepadState.invoke(null, id, state); }
            catch (Exception ignored) { return false; }
        }

        float axis(Object state, int index) {
            try { return ((Number) axes.invoke(state, index)).floatValue(); }
            catch (Exception ignored) { return 0f; }
        }

        boolean button(Object state, int index) {
            try { return ((Number) buttons.invoke(state, index)).byteValue() != 0; }
            catch (Exception ignored) { return false; }
        }

        void setCursorPosition(long window, double x, double y) {
            if (window == 0L) return;
            try {
                setCursorPos.invoke(null, window, x, y);
            } catch (Throwable error) {
                // Cursor locking is best-effort; controller input must remain usable.
            }
        }

        void setCursorHidden(long window, boolean hidden) {
            setCursorMode(window, hidden ? 0x34002 : 0x34001, hidden);
        }

        void setCursorCaptured(long window) {
            // GLFW_CURSOR_DISABLED is Minecraft's normal in-world mode. Reapply it
            // every controller frame after a GUI closes because the GUI may have
            // restored GLFW_CURSOR_NORMAL after Ruby's screen-state check.
            setCursorMode(window, 0x34003, true);
        }

        private void setCursorMode(long window, int mode, boolean force) {
            if (window == 0L || (!force && cursorMode == mode)) return;
            try {
                // GLFW_CURSOR = 0x33001; NORMAL = 0x34001;
                // HIDDEN = 0x34002; DISABLED/captured = 0x34003.
                setInputMode.invoke(null, window, 0x33001, mode);
                cursorMode = mode;
            } catch (Throwable error) {
                // Cursor mode is best-effort; never break controller input over it.
            }
        }
    }


    private static final class ControllerFrame {
        final boolean connected;
        final String controllerName;
        final float strafe;
        final float forward;
        final float lookX;
        final float lookY;
        final float leftTrigger;
        final float rightTrigger;
        final boolean[] buttons;
        final boolean[] pressed;
        final boolean[] released;
        final boolean sprintPressed;
        final float deltaSeconds;

        private ControllerFrame(
            boolean connected,
            String controllerName,
            float strafe,
            float forward,
            float lookX,
            float lookY,
            float leftTrigger,
            float rightTrigger,
            boolean[] buttons,
            boolean[] pressed,
            boolean[] released,
            boolean sprintPressed,
            float deltaSeconds
        ) {
            this.connected = connected;
            this.controllerName = controllerName;
            this.strafe = strafe;
            this.forward = forward;
            this.lookX = lookX;
            this.lookY = lookY;
            this.leftTrigger = leftTrigger;
            this.rightTrigger = rightTrigger;
            this.buttons = buttons;
            this.pressed = pressed;
            this.released = released;
            this.sprintPressed = sprintPressed;
            this.deltaSeconds = deltaSeconds;
        }

        static ControllerFrame disconnected() {
            return new ControllerFrame(false, "", 0f, 0f, 0f, 0f, 0f, 0f, new boolean[15], new boolean[15], new boolean[15], false, 0f);
        }

        boolean button(int index) { return index >= 0 && index < buttons.length && buttons[index]; }
        boolean pressed(int index) { return index >= 0 && index < pressed.length && pressed[index]; }
    }

    private static Class<?> findLoadedClass(Instrumentation instrumentation, String name) {
        for (Class<?> loaded : instrumentation.getAllLoadedClasses()) {
            if (loaded.getName().equals(name)) {
                return loaded;
            }
        }
        return null;
    }

    private static Class<?> findClass(Instrumentation instrumentation, String name) {
        return findClass(instrumentation, name, Thread.currentThread().getContextClassLoader());
    }

    private static Class<?> findClass(
        Instrumentation instrumentation,
        String name,
        ClassLoader preferredLoader
    ) {
        Class<?> loaded = findLoadedClass(instrumentation, name);
        if (loaded != null) {
            return loaded;
        }

        ClassLoader[] candidates = new ClassLoader[] {
            preferredLoader,
            Thread.currentThread().getContextClassLoader(),
            ClassLoader.getSystemClassLoader()
        };
        for (ClassLoader loader : candidates) {
            if (loader == null) {
                continue;
            }
            try {
                return Class.forName(name, false, loader);
            } catch (Throwable ignored) {
                // Try the next loader.
            }
        }
        return null;
    }

    private static Class<?> requireClass(Instrumentation instrumentation, String name) {
        return requireClass(instrumentation, name, Thread.currentThread().getContextClassLoader());
    }

    private static Class<?> requireClass(
        Instrumentation instrumentation,
        String name,
        ClassLoader preferredLoader
    ) {
        Class<?> type = findClass(instrumentation, name, preferredLoader);
        if (type == null) {
            throw new IllegalStateException("Minecraft class is unavailable: " + name);
        }
        return type;
    }

    private static Field requireField(Class<?> type, String name) throws NoSuchFieldException {
        Field field = optionalField(type, name);
        if (field == null) {
            throw new NoSuchFieldException(type.getName() + "." + name);
        }
        return field;
    }

    private static Field optionalField(Class<?> type, String name) {
        Class<?> cursor = type;
        while (cursor != null) {
            try {
                Field field = cursor.getDeclaredField(name);
                field.setAccessible(true);
                return field;
            } catch (NoSuchFieldException ignored) {
                cursor = cursor.getSuperclass();
            }
        }
        return null;
    }

    private static Field findStaticFieldAssignableTo(Class<?> type, Class<?> valueType) {
        Class<?> cursor = type;
        while (cursor != null) {
            for (Field field : cursor.getDeclaredFields()) {
                if (Modifier.isStatic(field.getModifiers()) && valueType.isAssignableFrom(field.getType())) {
                    field.setAccessible(true);
                    return field;
                }
            }
            cursor = cursor.getSuperclass();
        }
        return null;
    }

    private static Field findInstanceFieldAssignableTo(Class<?> type, Class<?> valueType) {
        Class<?> cursor = type;
        while (cursor != null) {
            for (Field field : cursor.getDeclaredFields()) {
                if (!Modifier.isStatic(field.getModifiers()) && valueType.isAssignableFrom(field.getType())) {
                    field.setAccessible(true);
                    return field;
                }
            }
            cursor = cursor.getSuperclass();
        }
        return null;
    }

    private static Method findNamedMethodByParameters(
        Class<?> type,
        String name,
        Class<?> returnType,
        Class<?>... parameters
    ) {
        if (name == null || name.isBlank()) return null;
        Class<?> cursor = type;
        while (cursor != null) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getName().equals(name)
                    && method.getReturnType() == returnType
                    && java.util.Arrays.equals(method.getParameterTypes(), parameters)) {
                    method.setAccessible(true);
                    return method;
                }
            }
            cursor = cursor.getSuperclass();
        }
        for (Method method : type.getMethods()) {
            if (method.getName().equals(name)
                && method.getReturnType() == returnType
                && java.util.Arrays.equals(method.getParameterTypes(), parameters)) {
                method.setAccessible(true);
                return method;
            }
        }
        return null;
    }

    private static Method findNamedZeroArgumentMethod(Class<?> type, String name) {
        if (name == null || name.isBlank()) return null;
        Class<?> cursor = type;
        while (cursor != null) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getName().equals(name) && method.getParameterCount() == 0) {
                    method.setAccessible(true);
                    return method;
                }
            }
            cursor = cursor.getSuperclass();
        }
        return null;
    }

    private static Method findNamedCompatibleSingleArgumentMethod(
        Class<?> type,
        String name,
        Class<?> argumentType,
        Class<?> returnType
    ) {
        if (name == null || name.isBlank()) return null;
        Class<?> cursor = type;
        while (cursor != null) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (!method.getName().equals(name)
                    || method.getParameterCount() != 1
                    || method.getReturnType() != returnType) {
                    continue;
                }
                if (method.getParameterTypes()[0].isAssignableFrom(argumentType)) {
                    method.setAccessible(true);
                    return method;
                }
            }
            cursor = cursor.getSuperclass();
        }
        return null;
    }

    private static Method findSingleArgumentCompatibleReturnMethod(
        Class<?> type,
        Class<?> argumentType,
        Class<?> returnType
    ) {
        Class<?> cursor = type;
        while (cursor != null) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getParameterCount() != 1) continue;
                if (!method.getParameterTypes()[0].isAssignableFrom(argumentType)) continue;
                if (!returnType.isAssignableFrom(method.getReturnType())) continue;
                method.setAccessible(true);
                return method;
            }
            cursor = cursor.getSuperclass();
        }
        return null;
    }

    private static Field requireFieldType(Class<?> type, String name, Class<?> expectedType) throws NoSuchFieldException {
        Field field = requireField(type, name);
        if (field.getType() != expectedType) {
            throw new NoSuchFieldException(
                type.getName() + "." + name + " expected " + expectedType.getName()
                    + " but mapped to " + field.getType().getName()
            );
        }
        return field;
    }

    private static void invokeNumericPair(Method method, Object target, float first, float second) throws Exception {
        Class<?>[] parameters = method.getParameterTypes();
        if (parameters.length != 2) {
            throw new IllegalArgumentException("Camera method does not accept two numeric values: " + method);
        }
        if (parameters[0] == double.class && parameters[1] == double.class) {
            method.invoke(target, (double) first, (double) second);
            return;
        }
        if (parameters[0] == float.class && parameters[1] == float.class) {
            method.invoke(target, first, second);
            return;
        }
        throw new IllegalArgumentException("Unsupported camera method parameters: " + java.util.Arrays.toString(parameters));
    }

    private static Method findMethodByParameters(Class<?> type, Class<?> returnType, Class<?>... parameters) {
        Class<?> cursor = type;
        while (cursor != null) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getReturnType() == returnType && java.util.Arrays.equals(method.getParameterTypes(), parameters)) {
                    method.setAccessible(true);
                    return method;
                }
            }
            cursor = cursor.getSuperclass();
        }
        return null;
    }

    private static Method requireMethod(Class<?> type, String name, int arity) throws NoSuchMethodException {
        Method method = optionalMethod(type, name, arity);
        if (method == null) {
            throw new NoSuchMethodException(type.getName() + "." + name + "/" + arity);
        }
        return method;
    }

    private static Method optionalMethod(Class<?> type, String name, int arity) {
        Class<?> cursor = type;
        while (cursor != null) {
            for (Method method : cursor.getDeclaredMethods()) {
                if (method.getName().equals(name) && method.getParameterCount() == arity) {
                    method.setAccessible(true);
                    return method;
                }
            }
            cursor = cursor.getSuperclass();
        }
        return null;
    }

    private static Method findMethod(Class<?> type, String name, int arity) {
        return optionalMethod(type, name, arity);
    }

    private static Method findRunnableExecutorMethod(Class<?> type) throws NoSuchMethodException {
        Method fallback = null;
        Class<?> cursor = type;
        while (cursor != null) {
            for (Method method : cursor.getDeclaredMethods()) {
                Class<?>[] parameters = method.getParameterTypes();
                if (parameters.length == 1
                    && parameters[0] == Runnable.class
                    && method.getReturnType() == void.class) {
                    method.setAccessible(true);
                    if (method.getName().equals("execute")) {
                        return method;
                    }
                    fallback = method;
                }
            }
            cursor = cursor.getSuperclass();
        }
        if (fallback != null) {
            return fallback;
        }
        throw new NoSuchMethodException(type.getName() + " Runnable executor method");
    }

    private static Method findMethodBySignature(Class<?> type, String name, Class<?> parameter) throws NoSuchMethodException {
        Class<?> cursor = type;
        while (cursor != null) {
            try {
                Method method = cursor.getDeclaredMethod(name, parameter);
                method.setAccessible(true);
                return method;
            } catch (NoSuchMethodException ignored) {
                cursor = cursor.getSuperclass();
            }
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }


    private static Constructor<?> findAssignableSingleArgumentConstructor(Class<?> type, Class<?> argumentType) {
        for (Constructor<?> constructor : type.getDeclaredConstructors()) {
            Class<?>[] parameters = constructor.getParameterTypes();
            if (parameters.length == 1 && parameters[0].isAssignableFrom(argumentType)) {
                constructor.setAccessible(true);
                return constructor;
            }
        }
        return null;
    }

    private static Constructor<?> findBooleanConstructor(Class<?> type) {
        for (Constructor<?> constructor : type.getDeclaredConstructors()) {
            Class<?>[] parameters = constructor.getParameterTypes();
            if ((parameters.length == 6 || parameters.length == 7) && allBoolean(parameters)) {
                constructor.setAccessible(true);
                return constructor;
            }
        }
        throw new IllegalStateException("No compatible player-input constructor was found in " + type.getName());
    }

    private static boolean allBoolean(Class<?>[] parameters) {
        for (Class<?> parameter : parameters) {
            if (parameter != boolean.class) {
                return false;
            }
        }
        return true;
    }

    private static Constructor<?> findNumericPairConstructor(Class<?> type) {
        for (Constructor<?> constructor : type.getDeclaredConstructors()) {
            Class<?>[] parameters = constructor.getParameterTypes();
            if (parameters.length == 2
                && (parameters[0] == float.class || parameters[0] == double.class)
                && parameters[0] == parameters[1]) {
                constructor.setAccessible(true);
                return constructor;
            }
        }
        throw new IllegalStateException("No compatible two-axis vector constructor was found in " + type.getName());
    }

    private static float parseFloat(String name, float fallback) {
        try {
            return Float.parseFloat(System.getProperty(name, Float.toString(fallback)));
        } catch (NumberFormatException ignored) {
            return fallback;
        }
    }

    private static float applyAxis(float value, float deadzone) {
        float magnitude = Math.abs(value);
        if (magnitude <= deadzone) {
            return 0f;
        }
        float normalized = (magnitude - deadzone) / (1f - deadzone);
        return Math.copySign(Math.min(1f, normalized), value);
    }

    private static float responseCurve(float value) {
        return value * Math.abs(value);
    }

    private static float normalizeTrigger(float value) {
        return clamp((value + 1f) * 0.5f, 0f, 1f);
    }

    private static float clamp(float value, float minimum, float maximum) {
        return Math.max(minimum, Math.min(maximum, value));
    }
 
    private static double clamp(double value, double minimum, double maximum) {
        return Math.max(minimum, Math.min(maximum, value));
    }

    private static Object defaultValue(Class<?> type) {
        if (type == null || type == void.class || !type.isPrimitive()) return null;
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

    private static void sleepQuietly(long millis) {
        try {
            Thread.sleep(millis);
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
        }
    }

    private static String detailed(Throwable error) {
        Throwable cause = error;
        while (cause.getCause() != null && cause.getCause() != cause) {
            cause = cause.getCause();
        }
        StringBuilder result = new StringBuilder(cause.getClass().getSimpleName());
        if (cause.getMessage() != null && !cause.getMessage().isBlank()) {
            result.append(": ").append(cause.getMessage());
        }
        StackTraceElement[] trace = cause.getStackTrace();
        if (trace.length > 0) result.append(" at ").append(trace[0]);
        return result.toString();
    }

    private static String concise(Throwable error) {
        Throwable cause = error;
        while (cause.getCause() != null && cause.getCause() != cause) {
            cause = cause.getCause();
        }
        String message = cause.getMessage();
        return cause.getClass().getSimpleName() + (message == null ? "" : ": " + message);
    }
}
