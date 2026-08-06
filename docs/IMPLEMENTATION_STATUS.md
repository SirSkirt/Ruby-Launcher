# Implementation Status — b2026.22

## Working launcher systems

- Start launcher menu with WIP Game Library, Settings and native Exit Launcher actions
- Start + Right Bumper launcher-log shortcut with chord-aware input handling

- Microsoft/Xbox/Minecraft authentication and remembered account restoration
- Vanilla installation and repair
- Managed Java runtime selection and download
- Minecraft process launch and log capture
- Editable per-profile memory, Java, renderer, JVM preset, Controller Support, fullscreen and resolution settings
- Global in-game Controller Support master switch
- Launcher fullscreen/windowed behavior and optional controller startup prompt
- Optional minimize-to-system-tray while Minecraft runs, with tray restore/exit actions, main-thread input recovery and automatic restore/focus after the final game exits
- Modrinth modpack search, version selection and `.mrpack` installation
- Modrinth vanilla, Fabric and Quilt loader profile generation
- Ruby Module Loader bootstrap and module discovery
- Fabric runtime namespace mapping through Fabric Loader MappingResolver
- Ruby Modules title-screen button and module-owned settings pages

## Controller Support under active testing

- Direct gameplay movement, camera, jump, attack/use, inventory and hotbar controls
- Menu focus navigation
- In-game Controller Options with per-profile persistence
- Custom GUI crosshair for container screens and world selection
- Per-profile and global enable/disable controls in Ruby Launcher
- Vanilla compatibility is active; Fabric now has a loader-aware mapping path that still needs real-pack validation; Quilt remains experimental

## Current Modrinth boundaries

- Client-required and client-optional files are installed; server-only files are skipped
- File hashes and pack paths are validated before installation
- Pack overrides and client-overrides are applied
- Forge and NeoForge packs are blocked until their installer-specific processing is implemented
- Modpack update, dependency repair and optional-file selection are later milestones

## Next milestones

- Native Windows testing of profile launch settings and tray minimize/restore behavior
- Real Fabric and Quilt Modrinth pack launch testing
- Modpack update/reinstall and repair actions
- Better controller text entry and long-list navigation
- Forge and NeoForge installation support
- Android Java runtime and graphics compatibility work
