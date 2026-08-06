# GitHub Actions Builds

Ruby Launcher can be built from a private GitHub repository without exposing its source code or embedding a GitHub token in the application.

## Workflows

### Build Windows

`.github/workflows/build-windows.yml` runs automatically on pushes to `main` and can also be started manually from the **Actions** tab. It builds Windows x64 NSIS and MSI installers and stores them as private workflow artifacts.

This is the workflow to use for ordinary test builds. On a phone:

1. Open the private Ruby Launcher repository on GitHub.
2. Open **Actions**.
3. Select **Build Windows**.
4. Choose **Run workflow**.
5. After it succeeds, open the run and download the artifact.

### Publish Windows Release

`.github/workflows/publish-windows.yml` is manual-only. It builds Ruby Launcher from the private repository and publishes only compiled installers to a separate public releases repository.

The workflow defaults to a draft prerelease so the assets can be reviewed before becoming public.

## Private repository configuration

Create these repository variables in the private source repository under **Settings → Secrets and variables → Actions → Variables**:

| Variable | Value |
| --- | --- |
| `RUBY_RELEASES_OWNER` | GitHub account or organization that owns the public releases repository |
| `RUBY_RELEASES_REPO` | Name of the public releases repository |
| `RUBY_RELEASES_BRANCH` | Default branch in the public releases repository, normally `main` |

Create this repository secret under **Actions → Secrets**:

| Secret | Purpose |
| --- | --- |
| `RUBY_RELEASES_TOKEN` | Fine-grained GitHub token restricted to the public releases repository with **Contents: Read and write** permission |

Do not place this token in source code, Tauri configuration, frontend JavaScript, compiled resources, or launcher settings. It exists only inside GitHub Actions.

## Toolchain used by CI

- Windows GitHub-hosted runner
- Node.js 22
- Eclipse Temurin Java 17 for Ruby Module Loader and Controller Support
- Stable Rust through rustup
- Tauri CLI 2.11.4
- Tauri Action v1

The Java modules are rebuilt from source before the native Tauri build, so the bundled `.jar` and `.rmod` always match the checked-in source.

## Update signing

The workflows intentionally do not generate `latest.json` or updater bundles yet because the in-launcher Tauri updater has not been wired into this build. When OTA updates are added, keep the updater private key in GitHub Actions Secrets, keep an external backup, and commit only the public verification key.

Android remains a separate release path. It should use Android signing or Play Store signing rather than the desktop updater workflow.
