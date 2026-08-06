# Validation Notes — b2026.21

## Performed in this build environment

- JavaScript syntax validation completed with `node --check`
- System-tray feature configuration, tray-menu IDs and launch/restore helper call sites checked statically
- Tray minimize/restore operations checked for main-thread dispatch, native focus and launcher-restored event wiring
- Controller prompt state checked to confirm ordinary disconnects cannot leave the launcher shell inert after startup
- Controller Start/Right Bumper shortcut paths and launcher-menu element wiring checked statically
- Simulated controller sequences passed for Start tap, simultaneous Start + Right Bumper, delayed bumper chord, held Start and Right Bumper-only navigation
- The native exit path and system-tray API usage were checked against Tauri 2 desktop APIs
- Web assets rebuilt successfully
- HTML element IDs were checked against every JavaScript element reference; no missing or duplicate IDs were found
- Frontend command names were checked against Tauri's registered command list
- CSS delimiter validation completed
- Ruby Module Loader 0.5.0 and Controller Support 0.8.4 rebuilt successfully with Java 17-compatible bytecode
- A compiled controller regression harness confirmed **Movement: Classic** is first, disabled and persisted as `movementMode=classic`
- Source and compiled-module checks confirmed `startUseItem()` is invoked only on the trigger rising edge, Minecraft's Use key remains held until release, and no Ruby-owned Use repeat timer remains
- Fabric runtime mapping adapter passed a compiled mock MappingResolver test for class, field and overloaded-method descriptor translation
- Backend source received delimiter and construction-site checks for the new profile and Modrinth data structures
- ZIP archive integrity and packaged-file checks are performed before release

## Carried forward from b2026.14

- Controller Support's Java reflection/integration harnesses
- Module metadata and service registration checks
- Inventory/world-selection GUI cursor validation
- Ruby Module Loader manifest and packaged module checks

## Not performed here

- Full Cargo/Tauri compilation because this environment does not contain a Rust toolchain
- Real Microsoft authentication or Minecraft process launch
- Live Modrinth pack download and installation through the native Tauri backend
- Physical-controller testing of the launcher Start menu, Start + Right Bumper chord, profile editor and Modrinth screens
- Native Windows verification of launcher fullscreen/windowed switching, tray input recovery and WebView focus behavior
- Real Fabric Loader/Minecraft launch confirming Ruby modules initialize in Fabric

The web and Java layers are validated here. The first Windows build and real Minecraft/Modrinth sessions remain the authoritative integration tests for the new Rust paths.

## b2026.21 GitHub Actions validation

- Workflow YAML was parsed locally.
- CI version verification passed against package.json, tauri.conf.json and Cargo.toml.
- Ruby Module Loader and Controller Support rebuilt successfully with Java.
- Frontend assets rebuilt successfully.
- A native Windows Tauri build still must run on the GitHub-hosted Windows runner; this container does not provide Rust or Windows bundling tools.
