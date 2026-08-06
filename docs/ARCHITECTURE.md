# Ruby Launcher Architecture — b2026.21

## Profile and launch chain

```text
Ruby profile
├── Minecraft/base version
├── optional Fabric or Quilt launch version
├── memory + JVM preset
├── Java and renderer preference
├── game fullscreen/resolution
└── per-profile Controller Support switch
        ↓
Launcher-wide Controller Support master switch
        ↓
Managed Java runtime selected from Minecraft metadata/profile preference
        ↓
Optional -javaagent:ruby-module-loader.jar
        ↓
Vanilla, Fabric or Quilt main class
```

Ruby Module Loader is a launcher-owned enhancement layer. It does not replace Fabric or Quilt. Controller Support is attached only when both the global and profile switches are enabled and compatible official client mappings are present.

## Modrinth installation

```text
Modrinth search API
        ↓
selected project version + primary .mrpack
        ↓
SHA-512/SHA-1 verification
        ↓
modrinth.index.json
├── Minecraft version
├── loader dependency
├── client file list
└── overrides
        ↓
base Minecraft install
        ↓
Fabric/Quilt launcher profile merge when required
        ↓
separate editable Ruby profile
```

Pack paths are restricted to normal relative components before anything is written. Client-unsupported files are skipped. Forge and NeoForge are detected but intentionally blocked because they require installer-specific processing.

## Launcher behavior

Launcher settings are persisted independently from profile settings. Fullscreen changes are applied to the Tauri window immediately. Windowed mode restores operating-system decorations. When minimize-on-launch is enabled, the main window is minimized and removed from the taskbar only after the Minecraft process starts. A persistent Tauri system-tray icon keeps Ruby accessible, and the process-exit watcher restores/focuses the launcher after the final running Minecraft instance closes.

## Module layout

```text
src-tauri/resources/
├── ruby-module-loader.jar
└── modules/
    └── ruby-controller.rmod
```

Modules are ordinary JAR-format files using the `.rmod` extension and Java `ServiceLoader` metadata.
