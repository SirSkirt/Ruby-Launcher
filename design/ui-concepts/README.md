# Ruby Launcher UI concepts

Mockups for a controller-first redesign. Nothing in `src-web/` has been changed.

**To try them:** serve this folder (`npx serve design/ui-concepts` or `python3 -m http.server` from inside it) and open the pages at 1920×1080. Arrow keys or a gamepad D-pad move focus, Enter or A presses. Add `?menu` to Concept A to see its Options menu open.

| Page | What it shows |
| --- | --- |
| `concept-a-shelf.html` | Console-style home: big hero, row of instance tiles |
| `concept-b-library.html` | List on the left, detail panel on the right |
| `concept-c-title.html` | Minimal "title screen" menu |
| `settings.html` | Settings redesign that works with any of the three |

The cover art is generated (pixel hills, one colour theme per instance). In the real launcher that slot would use the Modrinth icon or a screenshot when there is one, and the generator when there isn't.

---

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
