# The Mac App Store

Every app on the Mac App Store runs in the App Sandbox. Snappy's store build is the daemon built with the `mac-app-store` feature (`crates/macos`, forwarded by `apps/snappy`), which leaves out what the sandbox doesn't allow. `tools/macos/app-store.sh` builds, signs, and packages it, and `.github/workflows/mac-app-store.yml` does that for each release and uploads the package to App Store Connect.

The store listing is called **Snappy - Window Zones**, because "Snappy" is taken on the App Store, and the same name is used on the Microsoft Store. The name under the icon, in the menu bar, and in System Settings stays **Snappy**: it comes from the bundles' `CFBundleDisplayName`, not from the listing.

## What's different in the store build

### In the sandbox, feature by feature

| What Snappy does | In the sandbox | Why |
|---|---|---|
| Moving, resizing, raising, and focusing other apps' windows (`AXUIElement…`) | Works once the user grants Accessibility | Public API. Sandboxed apps may use it once the user turns Snappy on under Privacy & Security → Accessibility. Magnet, BetterSnapTool, and Moom ship on the App Store this way. |
| Asking for and checking Accessibility (`AXIsProcessTrustedWithOptions`) | Works | The system prompt and the System Settings list work for sandboxed apps. |
| Watching windows open, close, move, and change focus (`AXObserver`) | Works with Accessibility | Same API family as above. |
| The window under the pointer (`AXUIElementCopyElementAtPosition`) | Works with Accessibility | Same. |
| Chrome's enhanced accessibility, turned off for one change (`AXEnhancedUserInterface`) | Works with Accessibility | It's an Accessibility attribute of Chrome's app element. |
| The window server's on-screen list (`CGWindowListCopyWindowInfo`) | Works | Owner, layer, and bounds need no permission. Only window names need Screen Recording, and Snappy never reads them. |
| Watching the pointer and modifiers during a drag (NSEvent global monitors) | Works with Accessibility | Monitors are public AppKit. Snappy uses no event tap for drags, so it never needs Input Monitoring. |
| Global shortcuts (Carbon `RegisterEventHotKey`) | Works | Public, needs no permission or entitlement, and is how App Store window managers register shortcuts. |
| Reading macOS's own shortcuts (`com.apple.symbolichotkeys`) | Probably not | The sandbox keeps an app's preferences in its container, and reading another domain takes a temporary-exception entitlement that App Review grants reluctantly. Snappy then falls back to macOS's defaults (Mission Control's Control-arrows and the rest), so only shortcuts the user has changed go unnoticed. Unverified. |
| Overlays, the menu bar item, and the desktop guard's 1×1 windows | Works | Snappy's own windows. |
| NSWorkspace: app launches, activation, Space changes, sleep, running apps, VoiceOver and Switch Control state | Works | Public notifications and properties. |
| The `com.apple.accessibility.api` distributed notification | Works | Sandboxed apps may observe distributed notifications. Snappy uses only that it arrived. |
| Opening System Settings at Accessibility (`x-apple.systempreferences:` URL) | Works | |
| Display UUIDs (`CGDisplayCreateUUIDFromDisplayID`) | Works | Public ColorSync API. |
| Caps Lock as the Snappy key: the HID user key mapping (`IOHIDEventSystemClientSetProperty`, `UserKeyMapping`) | Doesn't work | The sandbox doesn't let apps change the HID event system's properties. It would take IOKit temporary exceptions that App Review doesn't grant, and no App Store app can remap keys system-wide. Left out of the build. |
| Caps Lock: the active event tap that adds the Snappy key's modifiers, and the posted modifier event | Left out | Useless without the mapping. An active tap would also need Accessibility in the sandbox. |
| Caps Lock: turning Caps Lock on and off (`IOHIDSetModifierLockState` on `IOHIDSystem`) | Doesn't work | Opening the `IOHIDSystem` user client is blocked by the sandbox. Left out. |
| Caps Lock: reading System Settings' Modifier Keys (`ByHost` global preferences) | Left out | Only Caps Lock needs it. |
| Open at login through the bundled launch agent (`SMAppService.agent`) | Not allowed | App Store apps may not install launchd jobs (App Review Guidelines 2.4.5). The store build registers the app itself, `SMAppService.mainApp` (macOS 13 and later; Snappy needs 14), and the bundle has no `Library/LaunchAgents`. |
| Starting the app's window (the daemon runs `Snappy App.app`'s binary with pipes) | Works | A sandboxed app may run executables inside its own bundle. The helper is signed with `com.apple.security.inherit`, so it runs in the daemon's sandbox. Pipes need no entitlement, so there's no socket and no App Group. |
| The app's Import and Export file pickers | Works in the app | `com.apple.security.files.user-selected.read-write`, on the daemon. A helper that inherits the sandbox may hold no entitlement but `app-sandbox` and `inherit`, and gets the daemon's. |
| Reading and writing the file the user picked | Works in the app | The picker grants the file to the process that showed it, the app, and the app reads or writes it. The daemon gets and sends only the text, over the pipe. |
| Settings, state, and logs under `~/Library` | Works, in the container | A sandboxed process's home is its container, so the same paths land in `~/Library/Containers/io.github.farhadzaidi.Snappy/Data/Library/Application Support/Snappy/` and `…/Data/Library/Logs/Snappy/`. The app inherits the same container. |
| The single-instance lock and signals (`SIGTERM`, `SIGUSR2`) | Works | The lock lives in the container. Signals need nothing. |
| Private API | None in Snappy's code; a few in Tauri's | See [Private API](#private-api). |

### What users of the store build lose

- **Caps Lock as the Snappy key.** Turning it on is refused with "Apps from the App Store can't remap Caps Lock, so it can't be the Snappy key." The Snappy key itself works as everywhere else.
- **Changed system shortcuts**, if the sandbox hides them: Snappy may offer a chord the user has moved one of macOS's own shortcuts onto. macOS's default shortcuts are still known.
- **Settings shared with a GitHub build.** The store build keeps its files in its container, so the two builds don't see each other's settings. Export from one and import into the other to move them.

### Private API

Apple rejects apps that use private API. Snappy's own code uses none. Its hand-written declarations in `crates/macos/src/ffi/` are all in the public SDK headers (`AXUIElement.h`, `CarbonEvents.h`, ColorSync, and `IOHIDEventSystemClient.h` and `IOHIDLib.h` for Caps Lock, which the store build doesn't compile). Snappy doesn't use `_AXUIElementGetWindow` or any `CGS`/`SLS` call.

The app's dependencies do. In a release build of the Tauri versions in `Cargo.lock`:

- wry sets the WebKit preferences `drawsBackground` (run because `tauri.conf.json` sets `backgroundColor`), `allowsPictureInPictureMediaPlayback` (always), and `fullScreenEnabled` (only before macOS 12.3) through key-value coding;
- tao names private `NSCursor` selectors (`_helpCursor`, `_zoomInCursor`, `_zoomOutCursor`, `_windowResize…Cursor`, `busyButClickableCursor`), called only for those CSS cursors.

The developer tools (`_inspector`, `developerExtrasEnabled`) are compiled out of release builds. Tauri apps ship on the App Store with these, but if App Review cites one, the choices are an upstream fix or a patched dependency.

## One-time setup

All of this is in the Apple Developer account of the team that publishes Snappy.

1. **App ID.** Certificates, Identifiers & Profiles → Identifiers → App IDs: an explicit App ID for `io.github.farhadzaidi.Snappy`, with no extra capabilities. The app's helper, `io.github.farhadzaidi.Snappy.App`, inherits the daemon's sandbox, so it needs no App ID or profile of its own, and no App Group is needed because the two talk over pipes.
2. **Certificates.** An **Apple Distribution** certificate and a **Mac Installer Distribution** certificate (it shows in Keychain Access as "3rd Party Mac Developer Installer"). Export each with its private key as a `.p12`, with the same password.
3. **Provisioning profile.** Profiles → Distribution → **Mac App Store Connect**, for the App ID above and the Apple Distribution certificate. Download the `.provisionprofile`.
4. **The app record.** App Store Connect → Apps → New App: platform macOS, name **Snappy - Window Zones**, bundle ID `io.github.farhadzaidi.Snappy`, a SKU such as `snappy`. Category Productivity, free, and App Privacy "Data Not Collected".
5. **API key.** App Store Connect → Users and Access → Integrations → App Store Connect API → Team Keys: a key with the **App Manager** role. Download the `.p8`, which can be downloaded only once, and note its Key ID and the Issuer ID.
6. **Secrets.** In the GitHub repository, Settings → Secrets and variables → Actions. Base64 files with `base64 -i <file> | pbcopy`.

| Secret | What it holds |
|---|---|
| `MAS_CERTIFICATE_P12` | The Apple Distribution `.p12`, base64 |
| `MAS_INSTALLER_CERTIFICATE_P12` | The Mac Installer Distribution `.p12`, base64 |
| `MAS_CERTIFICATE_PASSWORD` | The password of both `.p12` files |
| `MAS_PROVISION_PROFILE` | The Mac App Store Connect profile, base64 |
| `APPLE_TEAM_ID` | The team ID, ten characters |
| `APP_STORE_CONNECT_KEY_ID` | The API key's Key ID |
| `APP_STORE_CONNECT_ISSUER_ID` | The Issuer ID |
| `APP_STORE_CONNECT_KEY_P8` | The API key's `.p8`, base64 |

No `MAS_APP_PROVISION_PROFILE` is needed: the helper has no profile.

## How a release reaches the store

1. Publishing a GitHub release runs the workflow for its tag. It checks that the tag is `v` and Snappy's version, builds both architectures, signs, packages, and uploads. The package is also kept as an artifact.
2. The build appears in App Store Connect after Apple processes it, usually within the hour. Add it to a new macOS version with the same version number, write What's New, and submit it for review with the notes below.
3. Each upload's build number, `CFBundleVersion`, is the workflow's run number, which only grows, as App Store Connect requires of every upload.

GitHub doesn't start workflows for events caused by a workflow's own `GITHUB_TOKEN`. If a release is published by a workflow, that workflow has to start this one itself: `gh workflow run mac-app-store.yml -f tag=<tag> -f dry-run=false`, with `actions: write`.

From the Actions tab, **Run workflow** builds any tag. It's a dry run unless unticked: built and signed, never uploaded. Without the secrets, the workflow builds the store variant signed ad hoc and keeps the package as an artifact, so the build stays checked.

By hand on a Mac, with the identities in the login keychain:

```sh
APPLE_TEAM_ID=<team> MAS_PROVISION_PROFILE=<profile> BUILD_NUMBER=<above the last> tools/macos/app-store.sh signed
APP_STORE_CONNECT_KEY_ID=<key> APP_STORE_CONNECT_ISSUER_ID=<issuer> tools/macos/app-store.sh upload dist/app-store/Snappy-<version>.pkg
```

The upload expects the key at `~/.appstoreconnect/private_keys/AuthKey_<key id>.p8`. A hand-made build needs a build number above the workflow's last run number.

## Notes for App Review

Paste this into App Review Information → Notes:

> Snappy arranges windows into zones. It lives in the menu bar and has no Dock icon; choose "Open Snappy…" from its menu bar icon to open its window.
>
> Snappy uses the macOS Accessibility API to move and resize other apps' windows when the user drags a window with Shift held onto a zone, or presses one of Snappy's keyboard shortcuts. On first launch it asks for Accessibility permission (System Settings → Privacy & Security → Accessibility). Without it Snappy can't move windows and says so in its window. Snappy reads only window positions, sizes, and which app a window belongs to, never window titles or contents. It doesn't use Input Monitoring or Screen Recording, and it makes no network connections.
>
> To try it: grant Accessibility, then hold Shift while dragging a window by its title bar. Zones appear, and the window snaps into the one it's dropped on. Control-Option-Command with an arrow key moves the focused window to the neighboring zone.
>
> Snappy's window is a helper app inside the bundle that the menu bar app starts. It runs in the menu bar app's sandbox.

## Checking a build on a Mac

Nothing here has run on macOS yet: the code is linted for macOS from Linux (`tools/cross-check.sh macos`), and the scripts and workflow are checked only for syntax. Before the first submission, on a Mac:

1. `tools/macos/app-store.sh unsigned` builds, and `dist/app-store/Snappy.app` starts from Finder with no Dock icon. It's signed ad hoc with the sandbox on, so it behaves as the store build will, and `codesign -d --entitlements - dist/app-store/Snappy.app` lists `app-sandbox`.
2. Activity Monitor's Sandbox column says Yes for `snappy` and `snappy-app`.
3. Snappy asks for Accessibility, appears in the Accessibility list, and after the grant snaps windows by drag and by shortcut, in Safari, Finder, and Chrome.
4. Settings land in `~/Library/Containers/io.github.farhadzaidi.Snappy/Data/Library/Application Support/Snappy/`, and Snappy's window shows the debug log's path there.
5. "Open Snappy…" opens the window, and its pages, layouts, and editor draw (the web view isn't blank). If it is blank, the sandbox is keeping WebKit from something; try adding `com.apple.security.network.client` to `AppStore.entitlements`.
6. Turning on Caps Lock as the Snappy key shows the App Store message and changes nothing; `hidutil property --get UserKeyMapping` stays as it was.
7. Shortcuts work. Then move Mission Control's shortcut to Control-Option-Command-M in System Settings → Keyboard → Keyboard Shortcuts and record that chord in Snappy. A GitHub build refuses it. If the store build takes it, the sandbox hides `com.apple.symbolichotkeys`, as the table expects.
8. Turning on Open at login lists Snappy under System Settings → General → Login Items. After logging out and in, Snappy starts without opening its window. Opening it from Launchpad or Finder, the first time and while it runs, opens the window.
9. Export settings to a file in Documents, change a setting, and import the file: the setting comes back.
10. Quitting from the menu brings parked workspace windows back.
11. `tools/macos/app-store.sh signed` with the real identities, then `pkgutil --check-signature` passes, and `upload` is accepted. If `altool --upload-app` has been retired by then, upload the package with the Transporter app instead and update the script.
12. The first TestFlight build (macOS TestFlight takes the same package) installs and passes items 2 to 10.
