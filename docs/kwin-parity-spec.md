# Temporary spec: KDE Plasma test parity

This is a working spec for bringing KDE Plasma (KWin on Wayland) testing up to what GNOME has. Delete it once that lands, after folding what's still true into `testing.md`, `platforms.md`, and `development.md`.

Read `AGENTS.md`, `README.md`, and every file in `docs/` before starting. Everything there applies.

## Where things stand

The KDE Plasma adapter (`crates/kwin`, with the KWin script in `apps/kwin-script`) is in place. Its testing has two gaps that GNOME doesn't have.

1. **The sandbox needs KWin on the host.** `tools/live/kwin/kwin-session.sh` seals off the KWin installed on the machine, using Bubblewrap, and `tools/live/kwin/test.sh` runs `crates/kwin/tests/kwin.rs` inside it. There is no container image. A Mac or any machine without KWin 6 and Bubblewrap can't run KDE tests at all; only clippy and the unit tests run in the Linux build container. GNOME, by contrast, ships images (`tools/live/gnome/Dockerfile`, one per GNOME version) that run on any Docker host, the owner's arm64 Mac included.
2. **The live suite doesn't run on KWin.** `crates/live` (24 scenarios: drags, swaps and stacks, shortcuts, the nearest empty zone, workspaces, sending and staying, taking windows along with their outline, and more) runs on macOS, Windows, X11, and GNOME (Wayland and Xwayland), but not on KWin. KWin has its adapter tests and one end-to-end test only. Everything added since, take along and the marked-windows overlay layer in particular, is unverified on KWin.

## Goal

Any machine with Docker, including the owner's Mac, can run, for KDE Plasma:

- the adapter tests: `tools/live/kwin/test.sh`;
- the full live suite: a new `tools/live/kwin/live.sh [scenario prefix]`.

Both pass in full, with the same scenarios as GNOME, on the Plasma version the adapter targets (6.8) and any other version that's easy to get as an image.

## What to build

### 1. A KWin image

- A `tools/live/kwin/Dockerfile` with KWin 6, Xwayland, D-Bus, and whatever the session needs, from a distribution with current Plasma packages (Fedora's KDE packages, or KDE neon, for example). Support versions the way `tools/live/gnome/common.sh` does, if more than one is practical.
- It must work on arm64 and x86_64 hosts.
- Build the daemon, runner, and fixture the way GNOME does (`tools/live/gnome/Dockerfile.build`), reusing what can be shared rather than copying it.

### 2. The session inside the container

- Inside a container, the container is the seal, so `kwin-session.sh` (or a container counterpart like `gnome-session.sh`) starts `kwin_wayland --virtual` there with the same setup as today: its own D-Bus session, two side-by-side 1920×1080 virtual outputs, Xwayland, animations off, and no GPU.
- Keep the host Bubblewrap mode working for the owner's KDE machine, or replace it if the container covers it fully. Say which in the hand-off.
- Like `gnome-session.sh`, it must refuse to run where it could touch a real desktop.

### 3. The live suite on KWin

- `tools/live/kwin/live.sh`, analogous to `tools/live/gnome/live.sh`: build, start the session, install nothing on the host, run `snappy-live` against the daemon, and copy results, screenshots where possible, the daemon's logs, and KWin's log to `target/live/kwin/<version>/`.
- **Input:** the adapter tests already drive KWin through `org_kde_kwin_fake_input`. Give the live harness the same (the GNOME path in `crates/live/src/gnome.rs` and `crates/live/src/input.rs` shows the shape). Mind the note in `testing.md`: create the input device before any fixture window opens.
- **Fixture windows:** Wayland apps can't place or read their own frames, so place and read them through KWin, as the end-to-end test does (KWin's runner interface and `getWindowInfo`, or a throwaway script), the way GNOME's test extension does for GNOME.
- **The overlay and outlines:** without compositing there are no pixels, so check the overlay and the take-along outlines by asking KWin which on-screen surfaces Snappy has and where, as the adapter tests already do for the overlay. Scenarios that read pixels elsewhere need this equivalent.
- **Run chords:** `crates/live/src/bin/snappy-live.rs` picks chords each platform leaves free (`GNOME_CHORDS`); add KWin's if Plasma's defaults need different ones.
- **Every scenario passes.** Mark any that genuinely can't apply as skipped with the reason, as GNOME does for the grid scenario.

### 4. Shortcuts

Confirm every default chord is free on KWin with a real grab, not only the key map: Snappy + arrows, Space, 1–9, `;` `'` `.` `/`, and Enter. If any is taken by Plasma's defaults, say which; don't change the shared defaults without the owner.

## Ground rules

- Work on a branch made from the latest `main`, and rebase onto `main` before handing it over.
- Commit as the owner: `git config user.name "Syed Zaidi"` and `git config user.email "zaidi.farhad03@gmail.com"` in the clone. Never add `Co-Authored-By` or any attribution lines. Commit in logical steps. Push only when the owner asks.
- Never run a development build of the daemon, `snappy-live`, or `snappy-fixture` in the owner's logged-in session. Everything runs in the container or the sandbox.
- Don't regress the other platforms: run `tools/check.sh` and the X11 and GNOME suites before handing over.

## Docs

Move what's true into `docs/testing.md` (the KDE Plasma section, and the live suite table that lists where each suite runs), `docs/platforms.md`, and `docs/development.md`. Then delete this file.

## Hand-off report

- What was built, and whether the host Bubblewrap mode stays.
- Adapter test and live suite results per Plasma version, with scenario counts.
- Any default chord that isn't free on KWin.
- Known limitations.
