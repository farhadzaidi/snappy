# Releasing

How a version of Snappy goes from `main` to a GitHub release: setting the version, tagging, the workflow that builds every package, and the secrets that let it sign the macOS build.

Getting a release into package managers, the Microsoft Store, and the Mac App Store comes after this and is covered in:

- [Package managers](package-managers.md): Homebrew and winget.
- [Microsoft Store](microsoft-store.md)
- [Mac App Store](mac-app-store.md)

## The short version

```sh
tools/release/bump.sh 0.1.1               # set the version everywhere it's written
tools/check.sh                            # the version tests are part of it
git commit -am "Release 0.1.1"
git tag -a v0.1.1 -m "Snappy 0.1.1"
git push origin HEAD v0.1.1               # the tag starts the release workflow
```

Then wait for the **Release** workflow under the repository's Actions tab. It ends with a draft release holding every package. Read the generated notes, edit them into what changed for users, and press **Publish release**.

## Setting the version

`tools/release/bump.sh <version>` writes the version into every file that holds it:

- `version` in `[workspace.package]` in `Cargo.toml`, which every crate inherits, and the workspace crates' entries in `Cargo.lock`;
- `CFBundleShortVersionString` and `CFBundleVersion` in both property lists in `packaging/macos`;
- `version-name` in the GNOME extension's `metadata.json`;
- `version` in the app's `package.json`.

Tests check they all match the crates' version: `the_bundles_carry_snappys_version` in `apps/snappy/tests/bundle.rs`, and tests in `crates/gnome` and `apps/app/src-tauri`.

The script takes a plain `major.minor.patch`, with or without a leading `v`, and must be newer than the current one. Pre-release versions such as `0.2.0-rc.1` aren't accepted, because Windows Installer and macOS bundle versions are numbers only. It refuses to run with uncommitted changes, so that `git checkout -- .` undoes anything it did. It doesn't commit, tag, or push; it prints those commands for you.

## What the workflow does

Two workflows in `.github/workflows/`:

- **`build.yml`** builds every package, each on its own kind of machine, and uploads them as artifacts named `packages-<system>`. You can run it alone from the Actions tab.
- **`release.yml`** runs on a pushed tag `v*`. It checks the tag matches `Cargo.toml`'s version, calls `build.yml`, checks every expected file is there and nothing else, writes `SHA256SUMS`, attests where each file was built ([build provenance](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations)), and creates a **draft** release with notes generated from the merged pull requests.

The machines are GitHub's own:

| Packages | Runner |
|---|---|
| macOS, universal (Apple silicon and Intel) | `macos-15` |
| Windows x64 | `windows-latest` |
| Windows Arm64 | `windows-11-arm` |
| Linux x86_64 | `ubuntu-22.04`, whose older glibc lets the binaries run on more distributions |
| Linux aarch64 | `ubuntu-22.04-arm`, for the same reason |

The work is in scripts under `tools/release/`, which a maintainer can run on the matching system. Each writes to `dist/release/`:

- `linux.sh` builds the Linux packages for the machine's architecture. It needs `dpkg-deb` and `rpmbuild` besides the build tools in [CONTRIBUTING.md](../../CONTRIBUTING.md#setting-up).
- `windows.ps1 -Arch <x64|arm64>` builds the MSI and the setup program. It needs WiX 5 and NSIS; the script's header says how to install them.
- `macos.sh` builds the disk image, signing and notarizing it when the secrets below are in its environment, or with an identity of your own in `SNAPPY_SIGN_IDENTITY`.

### Dry runs

Under Actions → **Release** → **Run workflow**, leave **Dry run** ticked and pick a branch. It builds everything and leaves the packages and `SHA256SUMS` as the run's artifacts, without a release. Do one before the first release from a new setup, and after changing anything under `tools/release/`, `packaging/`, or the workflows.

A dry run on `main` also fills the build caches. A tag's run can read `main`'s caches but saves none of its own, since no later run could use them.

Running the workflow by hand on a tag with **Dry run** unticked makes the release for that tag, as pushing it does.

### Rerunning

If a job fails, fix the cause and rerun the failed jobs from the run's page. A rerun that finds the draft from last time replaces its files instead of making another.

If the fix needs a new commit, delete the draft release and the tag (`git push --delete origin v0.1.1` and `git tag -d v0.1.1`), commit the fix, and tag again. Never move a tag after its release has been published.

## Patch releases and hotfixes

A patch release, such as 0.1.1 after 0.1.0, goes exactly like any other: bump, tag, push.

When `main` already holds work that isn't ready to ship, make the fix on a release branch cut from the last tag instead:

```sh
git switch -c release/0.1 v0.1.0          # once per minor version; reuse it for 0.1.2
git cherry-pick <the fix's commit>        # land the fix on main first, then bring it over
tools/release/bump.sh 0.1.1
git commit -am "Release 0.1.1"
git tag -a v0.1.1 -m "Snappy 0.1.1"
git push origin release/0.1 v0.1.1
```

The workflow runs on the tag wherever it points. Keep `main`'s version ahead of the release branch's; its next release is then 0.2.0, or whatever `main` gets bumped to.

## The packages

`{v}` is the version without the `v`. These names are fixed: the package manager automation finds files by them.

| File | What it is |
|---|---|
| `Snappy-{v}-macos-universal.dmg` | `Snappy.app`, built by `tools/macos/bundle.sh --universal`, and a link to Applications to drag it onto. |
| `Snappy-{v}-windows-x64.msi`, `Snappy-{v}-windows-arm64.msi` | Windows Installer packages, from `packaging/windows/snappy.wxs` (WiX 5). |
| `Snappy-{v}-windows-x64-setup.exe`, `Snappy-{v}-windows-arm64-setup.exe` | Setup programs, from `packaging/windows/snappy.nsi` (NSIS 3), with a page that offers to open Snappy afterwards. |
| `snappy-app_{v}_amd64.deb`, `snappy-app_{v}_arm64.deb` | Debian and Ubuntu packages. |
| `snappy-app-{v}-1.x86_64.rpm`, `snappy-app-{v}-1.aarch64.rpm` | Fedora, openSUSE, and other RPM-based distributions. |
| `snappy-app-{v}-linux-x86_64.tar.gz`, `snappy-app-{v}-linux-aarch64.tar.gz` | The same files as the Linux packages, for anywhere else. |
| `SHA256SUMS` | The SHA-256 of every file above. |

### Windows

Both kinds of package install for the current user only, with no administrator rights, into `%LOCALAPPDATA%\Programs\Snappy`, the folder the README's manual install uses. They add a Start menu shortcut, **Snappy**, which starts `snappy.exe`, and an entry in Settings → Apps to uninstall.

Installing over a running Snappy, and uninstalling, first quit it the way its menu's Quit does, so every window hidden in another workspace comes back on screen. The setup program sends a close request to the daemon's hidden window and waits for it to exit. The MSI uses WiX's `CloseApplication`, which sends the session-end messages Snappy also quits on, and ends the process only if it's still running after 30 seconds.

Uninstalling also removes **Open at login**, the `Snappy` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, when it starts the copy being removed. Upgrading keeps it.

Use one kind of package or the other, not both. Snappy's settings, in `%APPDATA%\Snappy` and `%LOCALAPPDATA%\Snappy`, stay after an uninstall.

Neither is signed, since there's no budget for a Windows code signing certificate. SmartScreen warns on first run: **More info**, then **Run anyway**. The build summary says so too.

### Linux

The package is called `snappy-app`, because `snappy` is taken by Google's compression library in Fedora, Debian, and Arch. The commands are still `snappy` and `snappy-app`, and Snappy calls itself Snappy everywhere.

All three formats are made by `tools/release/linux.sh` from one tree, laid out as it lands under `/usr`:

```
usr/bin/snappy
usr/bin/snappy-app
usr/share/applications/io.github.farhadzaidi.Snappy.desktop
usr/share/icons/hicolor/scalable/apps/io.github.farhadzaidi.Snappy.svg
usr/share/icons/hicolor/256x256/apps/io.github.farhadzaidi.Snappy.png
usr/share/licenses/snappy-app/LICENSE.md
```

- The **deb** is that tree and the control file `packaging/linux/control`, packed by `dpkg-deb`. It depends on `libwebkit2gtk-4.1-0` and `libgtk-3-0`.
- The **rpm** is built by `rpmbuild` from `packaging/linux/snappy-app.spec`, which copies the tree in and builds nothing. It requires `webkit2gtk4.1` and `gtk3`, as Fedora names them, plus the shared libraries the binaries link, which `rpmbuild` finds itself.
- The **tarball** holds the tree under a folder `snappy-app-{v}-linux-<arch>/`. To use it without a package manager, copy its `usr/` into `~/.local/`.

The daemon looks for `snappy-app` beside its own executable, so both in `/usr/bin` works. The desktop entry starts `snappy`, which opens Snappy's window. No package starts Snappy at login; that's Snappy's own **Open at login** setting.

### macOS

The disk image holds `Snappy.app` and a link to `/Applications`. Both programs are universal binaries, joined with `lipo`. How it's signed depends on the secrets below, and the macOS job's summary says which:

- **With the certificate and the API key**: signed with the Developer ID Application identity, with the hardened runtime and a secure timestamp. The app is notarized and stapled, then the disk image is signed, notarized, and stapled. It opens without a warning.
- **With the certificate only**: signed but not notarized. Gatekeeper still blocks it.
- **With neither**: signed ad hoc. macOS blocks it until the user allows it in System Settings → Privacy & Security, and asks for Accessibility again after every update, because an ad-hoc signature changes with each build.

## Signing secrets

The macOS build signs and notarizes itself when these repository secrets exist. Add each one under the repository's **Settings → Secrets and variables → Actions → New repository secret**.

| Secret | What it holds |
|---|---|
| `MACOS_CERTIFICATE_P12` | The Developer ID Application certificate and its private key, as a `.p12` file, base64-encoded. |
| `MACOS_CERTIFICATE_PASSWORD` | The password the `.p12` was exported with. |
| `APPLE_TEAM_ID` | The Apple Developer team's ID, ten characters. Picks that team's identity from the `.p12`. |
| `APP_STORE_CONNECT_KEY_ID` | The App Store Connect API key's ID. |
| `APP_STORE_CONNECT_ISSUER_ID` | The issuer ID shown above the API keys. |
| `APP_STORE_CONNECT_KEY_P8` | The API key's `.p8` file, base64-encoded. |

All of them need a paid Apple Developer Program membership.

### The Developer ID Application certificate

Only the team's Account Holder can create one. On a Mac:

1. Open **Keychain Access** and choose **Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority**. Enter your email address and name, choose **Saved to disk**, and save the `.certSigningRequest` file.
2. At [developer.apple.com/account](https://developer.apple.com/account), open **Certificates, IDs & Profiles → Certificates** and press **+**.
3. Choose **Developer ID Application**, then the **G2 Sub-CA** profile. Upload the request file and download the certificate.
4. Double-click the downloaded `.cer`. It joins your login keychain, paired with the private key the request made.
5. In Keychain Access, under **login → My Certificates**, find **Developer ID Application: *your name* (*team ID*)**. Right-click it, choose **Export**, pick **Personal Information Exchange (.p12)**, and set a strong password.
6. Encode the file and copy it:

   ```sh
   base64 -i DeveloperIDApplication.p12 | pbcopy
   ```

   Paste it as `MACOS_CERTIFICATE_P12`, and the password as `MACOS_CERTIFICATE_PASSWORD`.
7. The team ID is in parentheses in the certificate's name, and under **Membership details** on the account page. Save it as `APPLE_TEAM_ID`.

Keep the `.p12` and its password somewhere safe, such as a password manager, and delete other copies. The certificate is valid for five years; renewing it means repeating these steps.

### The App Store Connect API key

`notarytool` signs in with an API key rather than an Apple ID and password.

1. At [appstoreconnect.apple.com](https://appstoreconnect.apple.com), open **Users and Access → Integrations → App Store Connect API**. The first time, the Account Holder must press **Request Access** and accept the terms.
2. Under **Team Keys**, press **+** (or **Generate API Key**). Name it, for example "Snappy notarization", give it **Developer** access, and press **Generate**.
3. Download the key, `AuthKey_<key ID>.p8`. Apple offers the download once only.
4. Save the **Key ID** from the key's row as `APP_STORE_CONNECT_KEY_ID`, and the **Issuer ID** above the table as `APP_STORE_CONNECT_ISSUER_ID`.
5. Encode the key and copy it, then paste it as `APP_STORE_CONNECT_KEY_P8`:

   ```sh
   base64 -i AuthKey_ABC123DEF4.p8 | pbcopy
   ```

To check the key before a release, ask Apple for the team's notarization history with it:

```sh
xcrun notarytool history --key AuthKey_ABC123DEF4.p8 --key-id ABC123DEF4 --issuer <issuer ID>
```

If a key leaks, revoke it on the same page and make a new one. Users who first ran an ad-hoc build grant Accessibility once more after updating to a signed one, since macOS ties the grant to the signature.

## Checking a download

Anyone can check a file they downloaded against the release:

```sh
sha256sum --check --ignore-missing SHA256SUMS     # on macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
gh attestation verify snappy-app_0.1.1_amd64.deb --repo farhadzaidi/snappy
```

The second shows the file was built by this repository's release workflow, from the tagged commit.
