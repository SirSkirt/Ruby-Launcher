# Ruby Launcher UI concepts

Mockups for a controller-first redesign. Nothing in `src-web/` has been changed yet.

**To try them:** serve this folder (`python3 -m http.server` from inside it) and open the pages at 1920×1080. Arrow keys or a gamepad D-pad move focus. Enter/A presses, Esc/B goes back, X and Y work where shown.

## Current direction: Ruby Edition Home (`ruby-edition-home.html`)

This is the chosen Shelf concept (A), redesigned around **Minecraft: Ruby Edition** per the development handoff. Java Edition profiles and modpacks are hidden from the UI for now (not deleted from the code), and the tabs are trimmed to **Home · Library · Settings**. Browse comes back with Java Edition.

Preview a state with URL parameters: `?channel=stable|nightly`, `&state=ready|install|update|installing|none`, `&menu` (dropdown open).

| Screenshot | State |
| --- | --- |
| `screenshots/ruby-edition-ready.png` | Stable installed, ready to play |
| `screenshots/ruby-edition-install.png` | Nightly selected, not installed: Play becomes **Install** |
| `screenshots/ruby-edition-menu.png` | Channel dropdown open |
| `screenshots/ruby-edition-installing.png` | Installing: progress inside the button, step-by-step status, live download line |
| `screenshots/ruby-edition-update.png` | Newer build published: Play becomes **Update** |
| `screenshots/ruby-edition-none.png` | No release published on that channel yet (true for both today) |

### How it works

- **Hero = Ruby Edition.** It shows the channel, build number, Minecraft data version (26.3) and an install status chip. The art changes per channel: daytime for Stable, night sky for Nightly, so you can tell at a glance which one you're about to launch.
- **Play + Channel dropdown, side by side.** The dropdown lists *Ruby Edition — Stable* and *Ruby Edition — Nightly*, each with its install status. The selected channel decides what Play launches.
- **The primary button follows the selected channel's state:** Play → Install → Installing… 46% → Update → "No build yet". Switching channels mid-install is allowed: the install keeps running and its progress shows on that channel's shelf tile.
- **Quick launch shelf:** one tile per channel, plus Logs and Manage installs (repair, uninstall, open folder per channel). The tiles and the dropdown control the same selection, so focusing a tile selects that channel and **A** launches it. **X** flips channels from anywhere on Home.
- **Build info card** (right): branch, latest build + commit, publish time, installed build, and the channel's own install folder. That last line makes the "these don't overwrite each other" promise visible.
- **Nightly is labelled Experimental** wherever it appears.

### What the Ruby Edition repo already gives us

I checked the `nightly` branch of `SirSkirt/ruby-edtion`. Read-only; nothing there was changed.

- **Releases:** `.github/workflows/ruby-edition-packages.yml` publishes a GitHub release on every push to `main` or `nightly`.
  - The tag format is `ruby-edition-<branch>-<run#>-<sha8>`.
  - Nightly releases are marked prerelease.
  - The assets are `ruby-edition-windows-x64.zip`, `ruby-edition-linux-x86_64.zip` and `SHA256SUMS`.
  - So the launcher can find each channel's latest build by filtering releases on the tag prefix (`ruby-edition-main-` vs `ruby-edition-nightly-`). The UI shows "build #run · sha" because the tags carry no semantic version.
- **Each zip contains:**
  - the three executables: `horizon-client`, `horizon-prepare` and `horizon-ctl`
  - `research-lock.json`
  - `ruby-edition-build.json`, which records the channel, commit and Minecraft version
- **Asset preparation:** `horizon-prepare` builds the game's resources from the player's own Minecraft Java 26.3 client and server jar, and needs JDK 25. Ruby Launcher can already install vanilla versions (with an owned account) and managed Java runtimes, so it can provide all three without redistributing anything. That's the "Prepare Minecraft 26.3 files" step in the installer.
- **Launch API** (`crates/horizon-client/src/game/launch_api.rs`): `--game-dir`, `--resources`, `--home`, `--session-stdin`, `--fullscreen`, `--quickPlaySingleplayer`. Per-channel separation needs no game changes. Stable and Nightly each get their own `game/` (saves, options), `resources/` and `home/` (logs, crash reports), and the Microsoft session goes in over stdin.
- **Caveat:** the workflow hasn't had a successful run yet, so there are no releases right now. The "No build yet" state is what the launcher would actually show today.

### Proposed folder layout

```
<launcher data>/games/ruby-edition/
  stable/   bin/  resources/  game/  home/  install.json
  nightly/  bin/  resources/  game/  home/  install.json
  shared/   minecraft-26.3/  (vanilla client + server jar, used only as prepare input)
```

### Implementation order (matches the handoff's "Immediate" list)

1. **Backend (Rust/Tauri):** add a `ruby_edition` module.
   - Commands: `list_ruby_channels` (GitHub releases → latest per channel), `install_ruby_channel` (download → SHA-256 verify → extract → prepare), `launch_ruby_channel` and `uninstall_ruby_channel`.
   - Progress goes out through the existing `install-progress` event.
2. **Prepare step:** reuse the existing vanilla installer and Java runtime manager to fetch 26.3 + Java 25, then run `horizon-prepare --prepare --server-jar … --minecraft-dir …` with output into that channel's `resources/`.
3. **Frontend:** swap the Home view's markup for this design. Hide the Java instance shelf, Versions and Modpacks behind a flag rather than deleting them.
4. **Shared UI pieces** for later screens: the Y-button context menu, the channel dropdown, and contextual button hints.

---

## Earlier exploration

The original critique and the three starting concepts are kept below for reference. Pages: `concept-a-shelf.html` (add `?menu` for its Options menu), `concept-b-library.html`, `concept-c-title.html` and `settings.html`.


## What's wrong with the current layout

Screenshots: `screenshots/current-*.png`

1. **It's laid out like a desktop web dashboard.** A sidebar, a top bar and a grid of cards is how admin panels look. On a TV it means small text, lots of boxes and nowhere obvious for your eye to land.
2. **Play isn't the star of the home screen.** Most of the screen goes to a marketing banner ("A launcher built for the couch…") and a dev-only "Build status" grid. Play is a mid-sized button in the right column.
3. **Every page has three headings.** Top bar heading + an all-caps pink label + the page title + a paragraph. For example, Instances says "Choose your world." / "SEPARATE WORLDS & MODS" / "Instances" / "Each profile keeps…".
4. **LB/RB doesn't match the layout.** The bumpers cycle sections, but the sections are a vertical sidebar. Bumpers read as "left/right tabs", so the sections belong in a horizontal tab bar.
5. **Keyboard hints in a controller UI.** The `1`–`5` badges on the nav buttons mean nothing on a gamepad.
6. **Instance cards have three focus stops each, and one of them is Remove.** Select / Edit / Remove sit side by side, so a slightly-off D-pad press next to Select lands on a destructive action. You also have to "Select" an instance and then go back to Home to play it.
7. **Settings use desktop form controls.** Native `<select>` dropdowns, checkboxes and a manual **Save settings** button. Focus jumps around a two-column grid unpredictably, and it's easy to leave without saving.
8. **Text is too small for 10 feet.** Descriptions are ~13–14 px at 1080p. TV guidelines usually want ≥ 20 px for secondary text.
9. **Versions and Modpacks are two top-level sections that do the same job**: both are ways to create a new instance.
10. *(Small bug)* The sidebar controller card always says "Controller connected" with a green dot, even when it says "No controller connected" underneath. `unlockLauncherWithoutController()` only updates `#controllerName`, not `#controllerCardTitle`, and the dot is static.

## Principles all three concepts follow

- **Four tabs on LB/RB:** Home · Library · Browse · Settings. Browse merges Versions + Modpacks ("get something new to play").
- **One focus style everywhere:** a thick ruby ring plus a slight scale-up. Lists use a solid white highlight instead.
- **Context menus instead of button rows.** An instance tile is one focus stop. **A** plays it. **Y** opens Options (Edit, Repair, Open folder, Mods, Remove…). Remove sits at the bottom behind a divider and needs a confirm.
- **Contextual button hints, bottom-right**, that change with what's focused (like Xbox/PlayStation) instead of a fixed footer.
- **No Save button.** Settings apply as you change them.
- **Bigger type:** 26–32 px for primary labels, 19–22 px minimum for secondary text.
- **Dev info moves out of the way.** The build number goes to Settings → About, build status goes to Diagnostics, logs stay on Start+RB.

## The concepts

### A · Shelf (console dashboard)
`screenshots/concept-a-shelf.png`, `screenshots/concept-a-options-menu.png`

The hero fills the screen with the focused instance's art, title, loader/version chips and a big Play button. Below it is a row of instance tiles, and the hero updates as you scroll the row.

- **Good:** the "couch-iest" option, fewest decisions, looks most like a real console. Play is always one press away.
- **Bad:** it gets awkward past about 10 instances, which is why it pairs with a Library tab. It also depends on good cover art to look its best.

### B · Library (list + detail)
`screenshots/concept-b-library.png`

A big list on the left with filter chips (All / Vanilla / Modded / Recent, cycled with **X**). The right side shows details for the focused instance: banner, stats (version, memory, controller on/off, playtime) and actions.

- **Good:** scales to dozens of profiles and modpacks, and shows the most information without opening dialogs. Up/down in one list is the most predictable controller movement there is.
- **Bad:** less "wow" than A, and closer to a PC launcher in feel.

### C · Title screen (minimal)
`screenshots/concept-c-title.png`

A game-style main menu: logo, the current instance (swap with LB/RB), a giant Play, and four other buttons.

- **Good:** the simplest possible flow (launch → A → playing). Works great for one or two instances, and maps naturally to Android TV.
- **Bad:** hides information. Managing lots of instances means going into a sub-screen, and LB/RB is used for instance switching instead of tabs.

### Settings redesign
`screenshots/settings-redesign.png`

Category list on the left, full-width rows on the right. Every control works with the D-pad alone:
- **Toggles** flip with A.
- **Sliders** move with left/right.
- **Cyclers** (`‹ Classic ›`) replace dropdowns, also left/right. No popup lists.

It also shows two settings worth considering: **Movement style** (the Classic mode from b2026.19, now visible) and **Button layout** (Xbox / PlayStation / Nintendo glyphs and A/B swap).

## Recommendation

Use **A for Home and B for the Library tab**. They share the same top bar, tiles, focus style and hints. Home stays fast and console-like, and Library handles the "I have 30 modpacks" case. C is the alternative if you want the launcher to feel like a game menu rather than a console dashboard, but it scales worst.

## Rough implementation notes

- The existing spatial `moveFocus()` in `src-web/app.js` already works for all of these layouts. What changes is mostly markup and CSS.
- Main new piece: a reusable **context menu dialog** (bound to Y) to replace the per-card Select/Edit/Remove buttons.
- `<select>` → cycler component: the left/right handling already in `adjustFocusedControl()` can be reused; it's mostly a visual change.
- Auto-save: call `save_settings` (debounced) on each `change` event instead of on form submit.
- Covers: use the Modrinth project icon when present, otherwise generate one from the instance id (see `shared.js` → `coverSVG`).
