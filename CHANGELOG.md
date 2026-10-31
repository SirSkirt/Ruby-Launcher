# Ruby Launcher Changelog

## b2026.23 — Minecraft: Ruby Edition Home & Interface Modes

- Rebuilt Home around **Minecraft: Ruby Edition** using the Shelf design: a large Play button with a Stable/Nightly channel dropdown, install status, a build info card and a Quick Launch shelf for both channels.
- Play becomes **Install**, **Installing… n%**, **Update** or **Check again** depending on the selected channel's state. Install progress shows Download → Verify → Extract → Prepare steps, live download speed and time left.
- Added the Ruby Edition backend (`src-tauri/src/ruby_edition.rs`):
  - Finds the newest Stable (`main`) and Nightly (`nightly`) builds from the Ruby Edition GitHub releases (`ruby-edition-<branch>-<run>-<sha8>` tags) and caches them for offline use.
  - Downloads the package for Windows x64 or Linux x86-64, verifies it against the release's `SHA256SUMS`, extracts it to a staging folder and swaps it in only after preparation succeeds.
  - Prepares game resources with `horizon-prepare` from the signed-in owner's own Minecraft files (fetched through the existing vanilla installer) and a Java runtime. No Mojang files are bundled.
  - Keeps each channel in its own folder (`games/ruby-edition/<channel>/` with `bin/`, `game/`, `home/`), so updating one channel never touches the other or any worlds.
  - Launches through Ruby Edition's Launch API (`--resources`, `--game-dir`, `--home`, `--session-stdin`), handing over the Microsoft session on stdin, with tray minimise/restore, playtime tracking and cancel, uninstall (keep or delete worlds) and open-folder commands.
- Added **Controller** and **Desktop** interface modes in Settings. Desktop mode is a decorated, resizable window with a compact, mouse-friendly layout; switching modes applies immediately without a restart. The controller startup screen now offers a "Use mouse and keyboard" button that switches to Desktop mode.
- Replaced the sidebar with LB/RB top tabs (Home · Library · Settings), added contextual controller button hints, X to switch channel and Y for an Options menu on any Ruby Edition item.
- Added a Library page listing both Ruby Edition channels with their install state and playtime.
- Settings now save automatically and use toggles, sliders and left/right pickers instead of desktop form controls.
- Fixed the "Ask for a controller on startup" screen being dismissed on the first frame when no controller was connected.
- Java Edition profiles, versions and Modrinth modpacks remain in the code but are hidden behind `JAVA_EDITION_UI` in `src-web/app.js` until Java Edition selection is reintroduced.
- Added a **Release** workflow that builds Windows (and optionally Linux) installers and publishes a GitHub Release in this repository, with notes taken from this changelog. It replaces the old publish workflow that needed a separate releases repository and token.
- Updated the application version to 0.23.0 and the build ID to b2026.23.

## b2026.20 — Safe Held Use

- Removed the 50 ms manual `startUseItem()` repeat loop that could place several blocks from one left-trigger press.
- Ruby now calls `startUseItem()` exactly once when the left trigger crosses the pressed threshold.
- Minecraft's internal Use key remains held until the trigger is released, allowing eating, shields, bows and other held actions to use Minecraft's normal tick handling.
- Trigger release still clears the internal key state and calls `stopUsingItem()` when available.
- If the internal Use key binding cannot be resolved on a Minecraft version, Ruby falls back to one Use attempt per trigger press rather than rapid repetition.
- Classic movement remains unchanged and locked as the only available movement mode.
- Updated Controller Support to 0.8.4, the application version to 0.20.0 and the build ID to b2026.20.

## b2026.19 — Classic Movement Groundwork & Held Use

- Added a disabled **Movement: Classic** entry to the in-game Controller Options screen.
- Added per-profile `movementMode=classic` persistence so Modern movement can be introduced later without replacing the original behavior.
- Preserved Classic movement exactly as it currently behaves.
- Changed controller Use Item handling to hold Minecraft's own Use key state while the left trigger is held.
- Added client-tick-paced direct-use continuation as a compatibility fallback.
- Releasing the trigger, opening a screen or disconnecting the controller now clears the held Use state.
- This enables hold-to-use actions such as eating, shields, bows, potions, tridents and spyglasses.
- Updated Controller Support to 0.8.3, the application version to 0.19.0 and the build ID to b2026.19.

## b2026.18 — Tray Input Recovery & Fabric Module Compatibility

- Fixed the launcher becoming unresponsive after Minecraft exits and Ruby restores from the system tray.
- The controller connection prompt is now a startup-only gate; a temporary gamepad disconnect can no longer mark the entire launcher interface as inert.
- Tray minimize and restore operations are now dispatched on Tauri's main UI thread.
- Added explicit native window focus, webview focus and a `launcher-restored` recovery event after Minecraft closes.
- Added Fabric-aware runtime mapping resolution for Ruby Module Loader.
- Ruby now waits for Fabric Loader's `MappingResolver`, then translates Mojang official class, field and method names into Fabric's active runtime namespace before loading Ruby modules.
- Extended Mojang mapping parsing to retain JVM descriptors required for accurate Fabric member mapping.
- Added launcher metadata identifying the selected loader platform and the base Minecraft version to Ruby Module Loader.
- Updated Ruby Module Loader to 0.5.0, Controller Support to 0.8.2, the application version to 0.18.0 and the build ID to b2026.18.

## b2026.17 — System Tray Launch Handling

- Replaced the hide-on-Minecraft-launch path with a native system-tray minimize path.
- Ruby now remains running while Minecraft is open so process supervision, logging and launcher-owned module handling stay available.
- Added a persistent Ruby tray icon using the bundled application icon.
- Added tray actions for **Open Ruby Launcher** and **Exit Ruby Launcher**.
- Added left-click tray restoration and automatic taskbar restoration after Minecraft closes.
- The launcher restores only after the final running Minecraft instance exits.
- Updated the application version to 0.17.0 and the build ID to b2026.17.

## b2026.16 — Controller Launcher Menu

- Added a controller launcher menu opened with Start.
- Added menu entries for WIP Game Library, Settings and Exit Launcher.
- Added Start + Right Bumper as the dedicated launcher-log shortcut.
- Added chord-aware Start handling so the menu does not flash open while the log shortcut is being pressed.
- Added an explicit native exit command and a controller-friendly WIP Game Library placeholder.
- Updated the application version to 0.16.0 and the build ID to b2026.16.

## b2026.15 — Profiles, Launcher Behavior & Modrinth

- Added an Edit action for every profile, including imported Modrinth packs.
- Added controller-friendly maximum/starting memory sliders and a generated JVM argument preview.
- Added profile-level Java runtime, renderer, JVM tuning preset, in-game Controller Support, Minecraft fullscreen and custom resolution controls.
- Added a launcher-wide in-game Controller Support master switch; both the global and profile switches must be enabled for Ruby's module to attach.
- Added launcher fullscreen/windowed mode. Windowed mode restores normal operating-system decorations.
- Added an optional controller prompt at launcher startup.
- Added optional hide-on-launch behavior and automatic launcher restoration/focus after Minecraft exits.
- Added public Modrinth search, version selection and `.mrpack` installation without an API key.
- Added SHA-512/SHA-1 verification, safe archive paths, client environment filtering, overrides and client-overrides extraction.
- Added vanilla, Fabric and Quilt Modrinth profile installation and launch metadata generation.
- Forge and NeoForge Modrinth packs are identified and rejected until their installer pipelines are implemented.
- Updated the application version to 0.15.0 and the build ID to b2026.15.

## b2026.14 Cursor & World Selection Hotfix

- Fixed the Windows pointer remaining visible after backing out of an inventory or block GUI. Ruby now restores GLFW's captured/disabled gameplay cursor instead of forcing the normal visible cursor.
- Added the controller crosshair cursor to Minecraft's world-selection screen. World rows and the screen's buttons now receive direct Minecraft pointer events, allowing worlds to be selected without relying on ordinary button-focus navigation.
- Renamed the in-game option to **GUI Crosshair Cursor** because it now covers both container screens and world selection.
- Updated Controller Support to module version 0.8.1.

## b2026.14 — Inventory Crosshair & Menu Navigation Beta

- Added a controller-driven crosshair cursor for inventories, crafting tables, furnaces, chests and other `AbstractContainerScreen` interfaces.
- The custom cursor is drawn inside Minecraft using `GuiGraphics`; Ruby now keeps Minecraft's native pointer aligned underneath it so slot hover, dragged stacks and carried items cannot remain attached to the physical mouse position.
- Either stick can move the inventory cursor. A or Right Trigger performs a primary click; X or Left Trigger performs a secondary click; B closes the container screen.
- Minecraft's normal pointer is hidden while the controller crosshair is active and restored when leaving the container screen. Hidden mode is reapplied continuously because Minecraft restores the pointer whenever it opens or replaces a container screen.
- Added readable **Inventory Crosshair Cursor** and **Inventory Cursor Speed** choices to Controller Options.
- Fixed the missing **Controller Options...** button by targeting Minecraft 1.21.1's real `options.controls.ControlsScreen` package.
- Reworked menu direction detection to use the stick's dominant axis, so small vertical drift no longer blocks left/right navigation.
- Added field-backed widget geometry and row/column-aware spatial focus scoring for paired buttons, tabs and option rows.
- Left the current experimental movement and jump behavior unchanged for wider beta feedback.
- Added a Java 17 reflection harness covering left/right focus movement, crosshair rendering, primary/secondary container clicks and B-to-close behavior.

## b2026.13 — Module-Owned Controller Options & Movement Hook Repair

- Added a generic Ruby Module Loader settings API. Modules supply their own page title, button label, setting names, explanations, current values and change handlers.
- Added **Controller Options...** to Minecraft's Controls menu only while Controller Support supplies a Controls settings page.
- Added plain-language settings for left-stick deadzone, camera speed, vertical camera direction, trigger pressure, controller menu navigation and menu repeat speed.
- Setting changes apply immediately and persist per Minecraft profile in `ruby/controller.properties`.
- Added a Minecraft-native generated settings screen with a readable title, one full-width button per setting, help text and a Done button.
- Controls-menu integration detects Minecraft widget-list rebuilds and restores the module settings button without creating duplicates.
- Reworked the post-`KeyboardInput` transformer to arm before the target class loads and recompute valid stack-map frames using Minecraft's class loader.
- Added exact hook diagnostics so the log distinguishes an armed hook from a successfully transformed method.
- Button factory selection now validates the returned builder's bounds/build chain, rejecting misleading overloads instead of failing later.
- Added Java 21 `-Xverify:all` integration tests for module-conditional settings, live value changes, persistence and full analog movement after the keyboard update.

## b2026.12 — Direct Movement, Jump & Menu Navigation

- Controller Support now starts after Minecraft has created a non-zero GLFW window handle, so gamepads can be used safely on the title screen before a world or `LocalPlayer` exists.
- Added direct controller focus navigation for Minecraft screens using the left stick or D-pad.
- A activates the currently focused Minecraft button directly; B closes submenus while leaving the main title screen open.
- Added focus-repeat timing for held directions and spatial selection between visible, active widgets.
- Left-stick movement is now written to both Minecraft's `Input` object and the player's inherited `LivingEntity.xxa` / `zza` movement state.
- A writes Minecraft's jumping state directly and calls `jumpFromGround()` once on a grounded press, while still preserving held jump for water and other movement states.
- Gameplay bridge setup is now lazy: title-screen controls initialize first, while movement/action hooks attach when a player enters a world.
- Added a one-time analog movement diagnostic showing the actual normalized forward and strafe values used by Ruby.
- Retained b2026.11 trigger pacing, camera, inventory, hotbar, pause, sprint and module-button restoration behavior.

## b2026.11 — Controller Input Timing Repair

- The **Ruby Modules** button is restored when Minecraft rebuilds or reuses the title screen after closing the module-status submenu.
- Added a reusable Ruby Module Loader post-input transformation service.
- Analog movement is now applied immediately after Minecraft's own `KeyboardInput` update on the game thread, preventing keyboard processing from overwriting the left stick.
- The transformation hook is installed through Java instrumentation and does not generate keyboard or mouse events.
- Left Trigger now starts use/place once per pull and lets Minecraft hold charge/use state until release.
- Right Trigger starts attack once and continues block damage at Minecraft's 20 Hz client-tick cadence instead of Ruby's 125 Hz controller-poll cadence.
- X now opens the player's crafting/inventory screen as the first legacy crafting shortcut.
- Kept camera polling responsive and preserved working Y, B, bumpers and pause controls.
- Added exact validation harnesses for post-keyboard analog movement, trigger edge handling, X-button screen opening and title-screen button restoration.

## b2026.09 — Ruby Module Loader Foundation

- Replaced the one-piece Controller Core agent with `ruby-module-loader.jar`.
- Added a reusable Ruby module API and shared Mojang mapping service.
- Packaged Controller Support separately as `ruby-controller.rmod`.
- Added a **Ruby Modules** button to Minecraft's title screen.
- Added an in-game module status panel using normal Minecraft UI classes.
- Preserved b2026.07's world-gated controller startup to avoid b2026.08's GLFW initialization crash.
- Prepared the same loader path for future Mobile Enhancements and other built-in modules.
- The loader is cooperative and does not replace Fabric, Quilt, Forge or NeoForge.

## b2026.07 — Safe Controller Bridge Revision

### Based on the stable b2026.07 launcher path

- Restored b2026.07 as the base instead of carrying forward b2026.08's early GLFW initialization.
- The controller agent no longer resolves or polls GLFW while Minecraft is inside native window initialization.
- Ruby waits until the Minecraft singleton, a real `LocalPlayer`, and the mapped input object are available.
- After the player appears, Ruby waits for a short stable period before creating the GLFW gamepad bridge.
- GLFW itself is never force-loaded by Ruby; Minecraft must load it first on its render thread.

### Lessons retained from b2026.08

- Added exact Java, mapping target, class-loader and bridge-failure diagnostics.
- Minecraft and GLFW failures are reported independently instead of one exception hiding the other.
- Minecraft classes are resolved through the actual game class loader.
- The game-thread executor is found by method signature rather than one fragile declaration.
- Player, input and inventory integration uses live runtime objects where possible instead of preloading optional classes.

### Validation

- Compiled with `javac --release 17` on Java 21.
- Passed a safe-start harness proving that no GLFW native call occurs before a world/player exists.
- Passed a direct-input harness for analog movement, camera and attack after the safe startup gate opens.

## b2026.07 — Controller Hook Initialization Fix

### Root cause fixed

- The Java agent and Mojang mappings were loading correctly, but the controller runtime then waited forever for `org.lwjgl.glfw.GLFWGamepadState` to be loaded by Minecraft.
- Vanilla Minecraft loads GLFW but never uses the GLFW gamepad-state class, so that class would never appear on its own.
- Ruby Controller Core now loads `GLFWGamepadState` through Minecraft's existing LWJGL class loader and initializes the gamepad bridge itself.

### More resilient Minecraft integration

- Reduced the minimum bridge requirements to analog movement and camera hooks.
- Attack, use, sprint, pause, inventory and hotbar capabilities are resolved independently, so one optional mapping mismatch no longer disables all controller input.
- Added type-based field fallbacks for the Minecraft instance, player, screen and input objects.
- Added explicit bootstrap, bridge, capability, controller-family and retry logging.
- Preserved direct GLFW input and direct Minecraft actions; no keyboard or mouse event translation was added.

### Legacy-style controls retained

- Left-stick click activates sprint while moving forward.
- A rapid forward-stick double tap activates sprint.
- Analog movement, camera, jump, sneak, triggers and bumpers remain part of the first controller test.

### Still pending

- Controller-driven menus
- Inventory grid navigation
- Xbox, PlayStation and Switch glyph rendering
- Remapping and rumble
- Broader Minecraft-version and third-party loader compatibility testing

## b2026.05 — Play Pipeline & Managed Java

- Play performs the complete desktop vanilla install, runtime and launch flow.
- Repair performs deeper SHA-1 validation.
- Added managed Eclipse Temurin runtimes on Windows.
- Added Minecraft process startup and game-log capture.