# Android Setup

Ruby Launcher uses Tauri 2's Android target. The current project configuration sets `minSdkVersion` to 28, which means Android 9 or newer.

## Required tools

- Node.js LTS and npm
- Rust through rustup
- Android Studio
- Android SDK Platform
- Android SDK Platform-Tools
- Android SDK Build-Tools
- Android SDK Command-line Tools
- Android NDK (Side by side)
- Android Studio's bundled JDK or another compatible JDK

## Windows environment variables

The exact NDK version directory depends on what Android Studio installed.

```powershell
[System.Environment]::SetEnvironmentVariable(
  "JAVA_HOME",
  "C:\Program Files\Android\Android Studio\jbr",
  "User"
)

[System.Environment]::SetEnvironmentVariable(
  "ANDROID_HOME",
  "$env:LocalAppData\Android\Sdk",
  "User"
)

$VERSION = Get-ChildItem -Name "$env:LocalAppData\Android\Sdk\ndk" | Select-Object -Last 1
[System.Environment]::SetEnvironmentVariable(
  "NDK_HOME",
  "$env:LocalAppData\Android\Sdk\ndk\$VERSION",
  "User"
)
```

Restart PowerShell and the IDE after setting environment variables.

## Rust Android targets

```powershell
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

## Initialize and run

```powershell
npm install
npm run android:init
npm run android:dev
```

Use a connected Android phone with USB debugging enabled or an Android emulator.

## APK build

```powershell
npm run android:build
```

The generated output location is reported by the Tauri/Gradle build. A release intended for distribution will eventually need signing configuration.

## b2026.07 note

This build retains the Android target, but its vanilla installer intentionally targets desktop files. Android TV manifest entries, device detection, mobile OpenJDK, Android-compatible LWJGL/native libraries and the game host remain later milestones.
