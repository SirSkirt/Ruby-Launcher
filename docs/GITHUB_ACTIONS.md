# GitHub Actions Builds

Ruby Launcher is built and released entirely by GitHub Actions. Publishing uses the repository's built-in `GITHUB_TOKEN`; no GitHub token is ever embedded in the application.

## Repository secret

| Secret | Purpose |
| --- | --- |
| `RUBY_SIGNIN_KEY` | The Microsoft sign-in application (client) ID. It's compiled into the launcher at build time, so it isn't stored in the public source. |

The Release workflow refuses to run without it. Build Windows still builds, with a warning, but that test build can't sign in.

Keeping the ID in a secret keeps it out of the source code, but it is not hidden from people who have the app. Microsoft's sign-in flow sends the ID in plain text and it's inside the compiled launcher, so treat it as private-ish, not secret.

For local builds, set `RUBY_SIGNIN_KEY` in your environment before `npm run desktop:build` or `cargo build`. Without it the launcher runs, but sign-in shows a message instead.

## Workflows

### Build Windows (test builds)

`.github/workflows/build-windows.yml` runs automatically on pushes to `main` and can also be started manually. It builds Windows x64 NSIS and MSI installers and keeps them as workflow artifacts (not a public release).

### Release

`.github/workflows/release.yml` is manual-only. It builds Ruby Launcher and publishes a GitHub Release in this repository:

| Platform | Files |
| --- | --- |
| Windows x64 | `-setup.exe` (NSIS installer) and `.msi` |
| Linux x86-64 (optional) | `.AppImage` and `.deb` |

How it runs:

1. Reads the version from `package.json` and the build ID from `src-tauri/src/lib.rs`.
2. Refuses to run if a release for `v<version>` already exists, so a forgotten version bump can't overwrite a published release.
3. Creates a draft release whose notes are the newest section of `CHANGELOG.md`.
4. Builds each platform and uploads its installers to that draft.
5. Publishes the release, but only if every build succeeded. If one fails, the release stays a draft with whatever finished, and nothing goes public.

Options when starting it:

| Option | Default | Meaning |
| --- | --- | --- |
| Mark as a prerelease | on | Labels it as a test build |
| Keep as a draft | off | Leave it unpublished so you can review it first |
| Also build Linux packages | on | Adds the AppImage and `.deb` |

### Releasing from a phone

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` (CI checks they match) and add a section to the top of `CHANGELOG.md`.
2. Open the repository on GitHub → **Actions** → **Release** → **Run workflow**.
3. Pick the options and run it. A full release takes roughly 15–30 minutes.
4. The finished release appears under **Releases**.

## Toolchain used by CI

- GitHub-hosted Windows and Ubuntu 22.04 runners
- Node.js 22
- Eclipse Temurin Java 21 for Ruby Module Loader and Controller Support
- Stable Rust through rustup
- Tauri CLI 2.11.4
- Tauri Action v1

The Java modules are rebuilt from source before the native Tauri build, so the bundled `.jar` and `.rmod` always match the checked-in source.

## Update signing

The workflows don't generate `latest.json` or updater bundles yet because the in-launcher Tauri updater has not been wired into this build. When OTA updates are added, keep the updater private key in GitHub Actions Secrets, keep an external backup, and commit only the public verification key.

Android remains a separate release path. It should use Android signing or Play Store signing rather than the desktop updater workflow.
