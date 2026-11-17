# The Mac App Store

This page is for the maintainer publishing Snappy on the Mac App Store. It explains what's different about the store build and why, the one-time setup, how each release reaches the store, and what to check on a Mac before the first submission.

## What the store build is

Every app on the Mac App Store runs in Apple's App Sandbox, which limits what it can touch. Snappy's store build is the usual daemon, built with the `mac-app-store` feature (defined in `crates/macos`, and passed through by `apps/snappy`), which leaves out what the sandbox doesn't allow.

- `tools/macos/app-store.sh` builds, signs, and packages it.
- `.github/workflows/mac-app-store.yml` does the same for each release, and uploads the package to App Store Connect.

The store listing is called **Snappy - Window Zones**, because "Snappy" is taken on the App Store. The Microsoft Store listing uses the same name. Under the icon, in the menu bar, and in System Settings, it's still **Snappy**, because that name comes from the app's own `CFBundleDisplayName`, not from the listing.

## What users of the store build lose

- **Caps Lock as the Snappy key.** The setting shows turned off and disabled, with "Apps from the App Store can't remap Caps Lock, so it can't be the Snappy key." under it. Settings imported from a GitHub build with it on load with it off, without a fuss. The Snappy key itself works as everywhere else.
- **Possibly, shortcuts the user moved.** If the sandbox hides macOS's shortcut settings, Snappy may offer a chord the user has moved one of macOS's own shortcuts onto. macOS's default shortcuts are still known.
- **Shared settings with a GitHub build.** The store build keeps its files in its sandbox container, so the two builds don't see each other's settings. Export from one and import into the other to move them.

Everything else works as in the GitHub build.

## The sandbox, feature by feature

Here's everything Snappy does on macOS, whether the sandbox allows it, and why. The API names are there to search the code and Apple's documentation.

| What Snappy does | In the sandbox | Why |
|---|---|---|
| Moving, resizing, raising, and focusing other apps' windows (`AXUIElement…`) | Works once the user grants Accessibility | Public API. Sandboxed apps may use it once the user turns Snappy on under Privacy & Security → Accessibility. Magnet, BetterSnapTool, and Moom ship on the App Store this way. |
| Asking for and checking Accessibility (`AXIsProcessTrustedWithOptions`) | Works | The system prompt and the System Settings list work for sandboxed apps. |
| Watching windows open, close, move, and change focus (`AXObserver`) | Works with Accessibility | Same API family as above. |
| The window under the pointer (`AXUIElementCopyElementAtPosition`) | Works with Accessibility | Same. |
| Turning Chrome's enhanced accessibility off for one move (`AXEnhancedUserInterface`) | Works with Accessibility | It's an Accessibility attribute of Chrome's app element. |
| The window server's list of windows on screen (`CGWindowListCopyWindowInfo`) | Works | Owner, layer, and bounds need no permission. Only window names need Screen Recording, and Snappy never reads them. |
| Watching the pointer and modifiers during a drag (AppKit's global event monitors) | Works with Accessibility | Public AppKit. Snappy uses no event tap for drags, so it never needs Input Monitoring. |
| Global shortcuts (Carbon `RegisterEventHotKey`) | Works | Public, needs no permission or entitlement, and is how App Store window managers register shortcuts. |
| Reading macOS's own shortcuts (`com.apple.symbolichotkeys`) | Probably not | The sandbox keeps an app's preferences in its container. Reading another app's takes a temporary-exception entitlement that App Review rarely grants. Snappy then falls back to macOS's defaults, such as Mission Control's Control-arrows, so only shortcuts the user has changed go unnoticed. Not yet checked on a Mac. |
| The overlays, the menu bar item, and the 1×1 windows that track Spaces | Works | They're Snappy's own windows. |
| App launches, activation, Space changes, sleep, running apps, and whether VoiceOver or Switch Control is on (NSWorkspace) | Works | Public notifications and properties. |
| The `com.apple.accessibility.api` notification | Works | Sandboxed apps may listen for distributed notifications. Snappy uses only the fact that it arrived. |
| Opening System Settings at Accessibility (an `x-apple.systempreferences:` URL) | Works | |
| Display IDs (`CGDisplayCreateUUIDFromDisplayID`) | Works | Public ColorSync API. |
| Caps Lock: remapping the key (`IOHIDEventSystemClientSetProperty` with `UserKeyMapping`) | Doesn't work | The sandbox doesn't let apps change the HID system's settings. It would take IOKit temporary exceptions that App Review doesn't grant, and no App Store app can remap keys system-wide. Left out of the build. |
| Caps Lock: the event tap that adds the Snappy key's modifiers | Left out | Useless without the remapping. An active tap would also need Accessibility in the sandbox. |
| Caps Lock: turning Caps Lock on and off (`IOHIDSetModifierLockState`) | Doesn't work | The sandbox blocks opening the `IOHIDSystem` user client. Left out. |
| Caps Lock: reading the Modifier Keys setting (the `ByHost` global preferences) | Left out | Only Caps Lock needs it. |
| Open at login through the launch agent inside the app (`SMAppService.agent`) | Not allowed | App Store apps may not install launchd jobs (App Review Guidelines 2.4.5). The store build registers the app itself instead, with `SMAppService.mainApp`, and has no `Library/LaunchAgents`. |
| Starting Snappy's window (the daemon runs the binary inside `Snappy App.app`, with pipes) | Works | A sandboxed app may run programs inside its own bundle. The helper is signed with `com.apple.security.inherit`, so it runs in the daemon's sandbox. Pipes need no entitlement, so there's no socket and no App Group. |
| The Import and Export file pickers | Works, in the app | The picker needs `com.apple.security.files.user-selected.read-write`, which goes on the daemon. A helper that inherits the sandbox may hold no entitlement but `app-sandbox` and `inherit`, and gets the daemon's. |
| Reading and writing the file the user picked | Works, in the app | The picker grants the file to the program that showed it, which is the app. The daemon only gets and sends the text, over the pipe. |
| Settings, state, and logs under `~/Library` | Works, in the container | A sandboxed program's home is its container. So the same paths land in `~/Library/Containers/io.github.farhadzaidi.Snappy/Data/Library/Application Support/Snappy/` and `…/Data/Library/Logs/Snappy/`. The app shares the same container. |
| The single-instance lock and signals (`SIGTERM`, `SIGUSR2`) | Works | The lock lives in the container. Signals need nothing. |
| Private API | None in Snappy's own code; a little in Tauri's | See [Private API](#private-api). |

### Private API

Apple rejects apps that use private API. Snappy's own code uses none. Every declaration in `crates/macos/src/ffi/` is in the public SDK headers: `AXUIElement.h`, `CarbonEvents.h`, ColorSync, and, for Caps Lock, which the store build doesn't compile, `IOHIDEventSystemClient.h` and `IOHIDLib.h`. Snappy doesn't use `_AXUIElementGetWindow`, or any `CGS` or `SLS` call.

The app's dependencies do use a little, in a release build of the Tauri versions in `Cargo.lock`:

- **wry** sets three WebKit preferences through key-value coding: `drawsBackground`, because `tauri.conf.json` sets `backgroundColor`; `allowsPictureInPictureMediaPlayback`, always; and `fullScreenEnabled`, only before macOS 12.3.
- **tao** names private `NSCursor` selectors (`_helpCursor`, `_zoomInCursor`, `_zoomOutCursor`, `_windowResize…Cursor`, `busyButClickableCursor`), called only for those CSS cursors.

The developer tools (`_inspector`, `developerExtrasEnabled`) are compiled out of release builds. Tauri apps ship on the App Store with these. If App Review ever cites one, the choices are a fix upstream or a patched dependency.

## One-time setup

All of this happens in the Apple Developer account of the team that publishes Snappy.

1. **An App ID.** Under Certificates, Identifiers & Profiles → Identifiers → App IDs, register an explicit App ID for `io.github.farhadzaidi.Snappy`, with no extra capabilities. The app's helper, `io.github.farhadzaidi.Snappy.App`, runs in the daemon's sandbox, so it needs no App ID or profile of its own. No App Group is needed either, because the two talk over pipes.
2. **Two certificates.** An **Apple Distribution** certificate, and a **Mac Installer Distribution** certificate, which shows in Keychain Access as "3rd Party Mac Developer Installer". Export each with its private key as a `.p12`, with the same password.
3. **A provisioning profile.** Under Profiles → Distribution → **Mac App Store Connect**, make one for the App ID above and the Apple Distribution certificate. Download the `.provisionprofile`.
4. **The app record.** In App Store Connect, under Apps → New App, choose platform macOS, name **Snappy - Window Zones**, bundle ID `io.github.farhadzaidi.Snappy`, and a SKU such as `snappy`. Set the category to Productivity, the price to free, and App Privacy to "Data Not Collected".
5. **An API key.** In App Store Connect, under Users and Access → Integrations → App Store Connect API → Team Keys, make a key with the **App Manager** role. Download the `.p8`, which can only be downloaded once, and note its Key ID and the Issuer ID.
6. **The secrets.** In the GitHub repository, under Settings → Secrets and variables → Actions, add the secrets below. To base64 a file:

   ```sh
   base64 -i <file> | pbcopy
   ```

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

The helper has no profile, so there's no secret for one.

## How a release reaches the store

1. **Publishing a GitHub release** runs the workflow for its tag. The workflow checks that the tag is `v` followed by Snappy's version, builds for both architectures, signs, packages, and uploads. It also keeps the package as an artifact.
2. **App Store Connect** shows the build once Apple has processed it, usually within the hour. Add it to a new macOS version with the same version number, write What's New, and submit it for review with [the notes below](#notes-for-app-review).

Each upload's build number, `CFBundleVersion`, is the workflow's run number. That only ever goes up, which App Store Connect requires of every upload.

### Running it by hand

From the Actions tab, **Run workflow** builds any tag. It's a dry run unless you untick it: built and signed, but never uploaded. Without the secrets, the workflow still builds the store variant, signed ad hoc, and keeps the package as an artifact, so the build stays checked.

GitHub doesn't start workflows for events caused by another workflow's own `GITHUB_TOKEN`. So if a release is ever published by a workflow, that workflow has to start this one itself, with `actions: write` permission:

```sh
gh workflow run mac-app-store.yml -f tag=<tag> -f dry-run=false
```

### Building and uploading on a Mac

With the identities in the login keychain:

```sh
APPLE_TEAM_ID=<team> MAS_PROVISION_PROFILE=<profile> BUILD_NUMBER=<above the last> tools/macos/app-store.sh signed
APP_STORE_CONNECT_KEY_ID=<key> APP_STORE_CONNECT_ISSUER_ID=<issuer> tools/macos/app-store.sh upload dist/app-store/Snappy-<version>.pkg
```

The upload expects the key at `~/.appstoreconnect/private_keys/AuthKey_<key id>.p8`. A build made by hand needs a build number above the workflow's last run number.

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

The store build hasn't run on macOS yet. Its code is linted for macOS from Linux (`tools/cross-check.sh macos`), and the scripts and workflow are only checked for syntax. Before the first submission, check on a Mac that:

1. `tools/macos/app-store.sh unsigned` builds, and `dist/app-store/Snappy.app` starts from Finder with no Dock icon. It's signed ad hoc with the sandbox on, so it behaves as the store build will. `codesign -d --entitlements - dist/app-store/Snappy.app` lists `app-sandbox`.
2. Activity Monitor's Sandbox column says Yes for `snappy` and `snappy-app`.
3. Snappy asks for Accessibility and appears in the Accessibility list. After the grant, it snaps windows by drag and by shortcut, in Safari, Finder, and Chrome.
4. Settings land in `~/Library/Containers/io.github.farhadzaidi.Snappy/Data/Library/Application Support/Snappy/`, and Snappy's window shows the debug log's path there.
5. **Open Snappy…** opens Snappy's window, and its pages, layouts, and editor draw. If the window is blank, the sandbox is keeping WebKit from something; try adding `com.apple.security.network.client` to `AppStore.entitlements`.
6. Caps Lock as the Snappy key shows disabled, with the App Store message under it, and `hidutil property --get UserKeyMapping` stays as it was. Check again after importing settings from a GitHub build that has it on.
7. Shortcuts work. Then move Mission Control's shortcut to Control-Option-Command-M in System Settings → Keyboard → Keyboard Shortcuts, and record that chord in Snappy. A GitHub build refuses it. If the store build takes it, the sandbox hides `com.apple.symbolichotkeys`, as the table above expects.
8. Turning on **Open at login** lists Snappy under System Settings → General → Login Items. After logging out and in, Snappy starts without opening its window. Opening it from Launchpad or Finder, both the first time and while it's running, opens the window.
9. Exporting settings to a file in Documents, changing a setting, and importing the file brings the setting back.
10. Quitting from the menu brings hidden workspace windows back.
11. `tools/macos/app-store.sh signed`, with the real identities, passes `pkgutil --check-signature`, and `upload` is accepted. If Apple has retired `altool --upload-app` by then, upload the package with the Transporter app instead, and update the script.
12. The first TestFlight build installs and passes items 2 to 10. macOS TestFlight takes the same package.
