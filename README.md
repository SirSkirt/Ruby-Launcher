# Ruby Launcher

**Build:** b2026.22  
**Application version:** 0.22.0  
**Author:** QTech Studios

Ruby Launcher is a controller-first Minecraft Java launcher built with Tauri 2, Rust and JavaScript. It handles Microsoft authentication, Vanilla installation, managed Java runtimes and launching Minecraft without requiring the user to configure Java manually.

## b2026.22 milestone

This build fixes the first GitHub Actions build failure by moving Ruby Module compilation from Java 17 to Java 21. Ruby Module Loader uses the JDK's internal ASM package and Controller Support targets Minecraft 1.21+, so CI now matches the Java 21 runtime used by the supported game versions. The private-source/public-release workflow from b2026.21 is otherwise preserved.

### GitHub Actions foundation

- Manual and push-triggered Windows x64 builds
- Rebuilds Ruby Module Loader and Controller Support before packaging
- Private workflow artifacts for test installers
- Optional compiled-only publishing to a separate public releases repository
- No GitHub token embedded in Ruby Launcher or committed to source
- Release publishing defaults to a draft prerelease
- Version consistency validation across npm, Tauri and Cargo configuration

See `docs/GITHUB_ACTIONS.md` for setup and phone-trigger instructions.

## b2026.20 milestone

This build makes held Use Item input safe for normal block placement. Ruby calls `startUseItem()` once when the left trigger is pressed, keeps Minecraft's internal Use binding held, and releases it when the trigger lifts. There is no Ruby-owned repeat timer.

### Safe held Use Item input

- One direct Use attempt occurs on the trigger's rising edge.
- Minecraft's internal Use key remains down while the trigger is held.
- Minecraft controls eating, shielding, charging and placement timing through its normal client tick.
- If the internal binding is unavailable, the fallback remains single-press rather than rapid-fire.
- Classic movement behavior is unchanged.

## b2026.19 milestone

This build preserves Ruby's original movement as **Classic** and prepares the controller settings format for a future Modern implementation. It also repairs held Use Item actions.

### Classic movement groundwork

- Controller Options now shows **Movement: Classic** as a disabled setting.
- The selected movement mode is persisted per Minecraft profile as `movementMode=classic`.
- Classic movement code and behavior are unchanged in this build.
- The disabled setting can be activated later when Modern movement is implemented without deleting the original mode.

### Held Use Item input

- Holding the left trigger now holds Minecraft's own Use key-binding state.
- b2026.19 initially included a direct-use repeat fallback; b2026.20 removes that unsafe repetition.
- Trigger release, controller loss and GUI entry clear the held state.
- This targets eating, shields, bows, potions, tridents, spyglasses and other hold-to-use actions.

## b2026.17 milestone

This build keeps Ruby Launcher alive while Minecraft is running by replacing hide-on-launch with a real system-tray workflow.

### System tray launch behavior

- The existing launch-behavior toggle now minimizes Ruby to the system tray instead of hiding its only window.
- Ruby remains running to supervise Minecraft, capture logs and preserve launcher-owned module handling.
- Left-clicking Ruby's tray icon restores and focuses the launcher.
- The tray menu contains **Open Ruby Launcher** and **Exit Ruby Launcher**.
- Ruby restores automatically after the final running Minecraft instance closes.

## b2026.16 milestone

This build adds a controller-first launcher system menu on top of the profile, launcher-settings and Modrinth foundation from b2026.15.

### Launcher controller shortcuts

- Press **Start** to open a menu containing **WIP Game Library**, **Settings** and **Exit Launcher**.
- Press **Start + Right Bumper** to open or refresh the launcher log.
- Ruby delays the Start-only action briefly while checking for the bumper chord, preventing the menu from flashing before the log opens.

## b2026.15 profile and Modrinth foundation

### Editable profiles

Every manual or Modrinth profile can now be edited with controller-friendly controls for:

- Maximum and starting memory, shown as generated `-Xms` and `-Xmx` arguments
- Java runtime preference
- Renderer preference
- Plain-language JVM tuning presets
- Per-profile in-game Controller Support
- Minecraft fullscreen or a custom window resolution

The launcher-wide Controller Support switch is a master switch. A profile must have Controller Support enabled **and** the global switch must be enabled before Ruby attaches its in-game module.

### Launcher behavior settings

- Launch Ruby Launcher fullscreen or as a decorated desktop window
- Show or skip the controller-required startup prompt
- Minimize Ruby to the system tray while Minecraft runs and restore/focus it after Minecraft closes

### Modrinth modpacks

Ruby can search Modrinth, list a pack's published versions, download its `.mrpack`, verify hashes, install client files and overrides, and create a normal editable Ruby profile. The first implementation supports vanilla, Fabric and Quilt packs. Forge and NeoForge packs are detected and blocked with a clear message until their installer processing is added.

Controller Support remains independent from the mod loader and can be enabled or disabled while installing the pack or later in the profile editor.

## Cooperative module layout

Ruby Module Loader does not replace Fabric, Quilt, Forge or NeoForge. Ruby's built-in modules are launcher enhancements that can run beside the selected Minecraft environment.

```text
ruby-module-loader.jar
├── loader API
├── Mojang mapping service
├── module discovery
├── Minecraft menu integration
└── module-owned settings renderer

modules/
└── ruby-controller.rmod
    ├── direct controller input
    ├── controller settings definitions
    └── profile-specific persistence
```

## Build commands

```bash
npm install
npm run modules:build
npm run build
npm run desktop:build
```

`modules:build` targets Java 21, matching Minecraft 1.21+ and Ruby Module Loader's JDK-internal ASM usage.

## Current limitations

- Controller gameplay and menu hooks remain experimental and require native Minecraft 1.21.1 testing.
- Button-screen navigation and a free-moving container crosshair are implemented; text entry, slot snapping, controller glyphs and advanced modded-screen handling remain pending.
- The initial settings page uses choice buttons. Full button remapping and calibration tools are later milestones.
- Built-in Modrinth installation currently supports vanilla, Fabric and Quilt packs. Forge and NeoForge installer processing is still pending.
- The Android Minecraft runtime and graphics compatibility layer are later milestones.
