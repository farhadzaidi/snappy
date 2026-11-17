# Releasing

This page is for the maintainer cutting a release. It covers how a version of Snappy goes from `main` to a published GitHub release: setting the version, tagging, the workflow that builds every package, what each package is, and the secrets that let the macOS build be signed.

After the GitHub release, these pages cover getting it into the other places people install from:

- [Package managers](package-managers.md): Homebrew and winget.
- [The Microsoft Store](microsoft-store.md)
- [The Mac App Store](mac-app-store.md)

## The short version

```sh
tools/release/bump.sh patch               # 0.1.0 → 0.1.1, set everywhere it's written
tools/check.sh                            # the version tests are part of it
git commit -am "Release 0.1.1"
git tag -a v0.1.1 -m "Snappy 0.1.1"
git push origin HEAD v0.1.1               # the tag starts the release workflow
```

Then wait for the **Release** workflow under the repository's Actions tab. It ends with a draft release holding every package. Read the generated notes, rewrite them as what changed for users, and press **Publish release**.

## Setting the version

The version is written in several files, and `tools/release/bump.sh` sets it in all of them. Give it `patch`, `minor`, or `major` to go up one step, or a version such as `0.3.0`.

It writes:

- the workspace version in `Cargo.toml`, which every crate inherits, and the workspace crates' entries in `Cargo.lock`;
- both version fields in both property lists in `packaging/macos`;
- the version name in the GNOME extension's `metadata.json`;
- the version in the app's `package.json`.

Tests check that they all match: `the_bundles_carry_snappys_version` in `apps/snappy/tests/bundle.rs`, and tests in `crates/gnome` and `apps/app/src-tauri`.

A few things it won't do:

- **No pre-release versions** such as `0.2.0-rc.1`, because Windows Installer and macOS bundle versions can only be numbers. A leading `v` is fine.
- **No going backwards.** The new version must be newer than the current one.
- **No uncommitted changes.** It refuses to run on a dirty working tree, so `git checkout -- .` always undoes what it did.
- **No committing, tagging, or pushing.** It prints those commands for you to run.

## What the workflow does

Two workflows in `.github/workflows/` do the work.

**`release.yml`** runs when a tag starting with `v` is pushed. It:

1. checks that the tag matches the version in `Cargo.toml`;
2. runs `build.yml` to build every package;
3. checks that every expected file is there, and nothing else;
4. writes `SHA256SUMS`;
5. records where each file was built, as [build provenance](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations);
6. creates a **draft** release, with notes generated from the merged pull requests.

**`build.yml`** builds each package on its own kind of machine, and uploads them as artifacts named `packages-<system>`. You can also run it on its own from the Actions tab.

The machines are GitHub's:

| Packages | Runner |
|---|---|
| macOS, universal (Apple silicon and Intel) | `macos-15` |
| Windows x64 | `windows-latest` |
| Windows Arm64 | `windows-11-arm` |
| Linux x86_64 | `ubuntu-22.04`, whose older glibc lets the binaries run on more distributions |
| Linux aarch64 | `ubuntu-22.04-arm`, for the same reason |

### Building packages by hand

The real work is in scripts under `tools/release/`, which you can run yourself on the matching system. Each writes to `dist/release/`.

On Linux, for the machine's own architecture. It needs `dpkg-deb` and `rpmbuild` besides the tools in [CONTRIBUTING.md](../../CONTRIBUTING.md#setting-up):

```sh
tools/release/linux.sh
```

On Windows, for `x64` or `arm64`. It needs WiX 5 and NSIS, and the script's header says how to install them:

```powershell
pwsh tools\release\windows.ps1 -Arch x64
```

On macOS. It signs and notarizes the disk image when [the signing secrets](#signing-the-macos-build) are in its environment, or signs with your own identity named in `SNAPPY_SIGN_IDENTITY`:

```sh
tools/release/macos.sh
```

### Dry runs

To build everything without making a release, go to Actions → **Release** → **Run workflow**, leave **Dry run** ticked, and pick a branch. The packages and `SHA256SUMS` are left as the run's artifacts.

Do a dry run before the first release from a new setup, and after changing anything under `tools/release/`, `packaging/`, or the workflows.

A dry run on `main` also fills the build caches. A tag's run can read `main`'s caches but saves none of its own, since no later run could use them.

Running the workflow by hand on a tag with **Dry run** unticked makes the release for that tag, just as pushing the tag does.

### When a job fails

If a job fails, fix the cause and rerun the failed jobs from the run's page. A rerun that finds the draft release from last time replaces its files rather than making another.

If the fix needs a new commit, delete the draft release and the tag, commit the fix, and tag again:

```sh
git push --delete origin v0.1.1
git tag -d v0.1.1
```

Never move a tag after its release has been published.

## Patch releases and hotfixes

A patch release, such as 0.1.1 after 0.1.0, goes like any other: bump, tag, push.

Sometimes `main` already holds work that isn't ready to ship. Then make the fix on a release branch, cut from the last tag:

```sh
git switch -c release/0.1 v0.1.0          # once per minor version; reuse it for 0.1.2
git cherry-pick <the fix's commit>        # land the fix on main first, then bring it over
tools/release/bump.sh patch
git commit -am "Release 0.1.1"
git tag -a v0.1.1 -m "Snappy 0.1.1"
git push origin release/0.1 v0.1.1
```

The workflow runs on the tag wherever it points. Keep `main`'s version ahead of the release branch's, so its next release is 0.2.0, or whatever `main` gets bumped to.

## The packages

`{v}` is the version without the `v`. These names are fixed, because the package manager workflow finds files by them.

| File | What it is |
|---|---|
| `Snappy-{v}-macos-universal.dmg` | `Snappy.app`, built by `tools/macos/bundle.sh --universal`, with a link to Applications to drag it onto |
| `Snappy-{v}-windows-x64.msi`, `Snappy-{v}-windows-arm64.msi` | Windows Installer packages, from `packaging/windows/snappy.wxs` (WiX 5) |
| `Snappy-{v}-windows-x64-setup.exe`, `Snappy-{v}-windows-arm64-setup.exe` | Setup programs, from `packaging/windows/snappy.nsi` (NSIS 3) |
| `snappy-app_{v}_amd64.deb`, `snappy-app_{v}_arm64.deb` | Debian and Ubuntu packages |
| `snappy-app-{v}-1.x86_64.rpm`, `snappy-app-{v}-1.aarch64.rpm` | Fedora, openSUSE, and other RPM-based distributions |
| `snappy-app-{v}-linux-x86_64.tar.gz`, `snappy-app-{v}-linux-aarch64.tar.gz` | The same files as the Linux packages, for anywhere else |
| `SHA256SUMS` | The SHA-256 of every file above |

### Windows

**Where it installs.** Both kinds of package install for the current user only, with no administrator rights, into `%LOCALAPPDATA%\Programs\Snappy`, the same folder as the README's manual install. They add a Start menu shortcut, **Snappy**, and an entry in Settings → Apps to uninstall. The setup program ends with a page that offers to open Snappy.

**Replacing a running Snappy.** Installing over a running Snappy, or uninstalling it, first quits it the way its menu's **Quit** does, so every hidden workspace window comes back on screen. The setup program sends Snappy's hidden window a close request and waits for it to exit. The MSI sends the messages for the session ending, which Snappy also quits on, and ends the process only if it's still running after 30 seconds.

**Open at login.** Uninstalling removes the `Snappy` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, if it starts the copy being removed. Upgrading keeps it.

**Settings stay.** Snappy's settings, in `%APPDATA%\Snappy` and `%LOCALAPPDATA%\Snappy`, stay after an uninstall. Use one kind of package or the other, not both.

**Not signed.** There's no budget for a Windows code signing certificate. So SmartScreen warns the first time Snappy runs: **More info**, then **Run anyway**. The build summary says so too.

### Linux

The package is called `snappy-app`, because the name `snappy` is taken by Google's compression library in Fedora, Debian, and Arch. The commands are still `snappy` and `snappy-app`, and Snappy calls itself Snappy everywhere.

All three formats are made by `tools/release/linux.sh` from one tree of files, laid out as it lands under `/usr`:

```
usr/bin/snappy
usr/bin/snappy-app
usr/share/applications/io.github.farhadzaidi.Snappy.desktop
usr/share/icons/hicolor/scalable/apps/io.github.farhadzaidi.Snappy.svg
usr/share/icons/hicolor/256x256/apps/io.github.farhadzaidi.Snappy.png
usr/share/licenses/snappy-app/LICENSE.md
```

- **The deb** is that tree plus the control file `packaging/linux/control`, packed by `dpkg-deb`. It depends on `libwebkit2gtk-4.1-0` and `libgtk-3-0`.
- **The rpm** is built by `rpmbuild` from `packaging/linux/snappy-app.spec`, which copies the tree in and compiles nothing. It requires `webkit2gtk4.1` and `gtk3`, as Fedora names them, plus the shared libraries the binaries link, which `rpmbuild` finds itself.
- **The tarball** holds the tree in a folder named `snappy-app-{v}-linux-<arch>/`. To use it without a package manager, copy its `usr/` into `~/.local/`.

The desktop entry starts `snappy`, which opens Snappy's window. No package starts Snappy at login; that's Snappy's own **Open at login** setting.

### macOS

The disk image holds `Snappy.app` and a link to `/Applications`. Both programs are universal binaries, joined with `lipo`. How they're signed depends on which [secrets](#signing-the-macos-build) are set, and the macOS job's summary says which:

- **With the certificate and the API key**, the app is signed with the Developer ID Application identity, with the hardened runtime and a secure timestamp, then notarized and stapled. The disk image is then signed, notarized, and stapled too. It opens without a warning.
- **With the certificate only**, it's signed but not notarized. Gatekeeper still blocks it.
- **With neither**, it's signed ad hoc. macOS blocks it until the user allows it in System Settings → Privacy & Security, and asks for Accessibility again after every update, because an ad-hoc signature changes with each build.

## Signing the macOS build

The macOS build signs and notarizes itself when these repository secrets exist. Add each under the repository's **Settings → Secrets and variables → Actions → New repository secret**. All of them need a paid Apple Developer Program membership.

| Secret | What it holds |
|---|---|
| `MACOS_CERTIFICATE_P12` | The Developer ID Application certificate and its private key, as a `.p12` file, base64-encoded |
| `MACOS_CERTIFICATE_PASSWORD` | The password the `.p12` was exported with |
| `APPLE_TEAM_ID` | The Apple Developer team's ID, ten characters, which picks that team's identity from the `.p12` |
| `APP_STORE_CONNECT_KEY_ID` | The App Store Connect API key's ID |
| `APP_STORE_CONNECT_ISSUER_ID` | The issuer ID shown above the API keys |
| `APP_STORE_CONNECT_KEY_P8` | The API key's `.p8` file, base64-encoded |

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

Keep the `.p12` and its password somewhere safe, such as a password manager, and delete other copies. The certificate lasts five years; renewing it means repeating these steps.

### The App Store Connect API key

Notarizing signs in to Apple with an API key, rather than an Apple ID and password.

1. At [appstoreconnect.apple.com](https://appstoreconnect.apple.com), open **Users and Access → Integrations → App Store Connect API**. The first time, the Account Holder must press **Request Access** and accept the terms.
2. Under **Team Keys**, press **+** (or **Generate API Key**). Name it, for example "Snappy notarization", give it **Developer** access, and press **Generate**.
3. Download the key, `AuthKey_<key ID>.p8`. Apple offers the download only once.
4. Save the **Key ID** from the key's row as `APP_STORE_CONNECT_KEY_ID`, and the **Issuer ID** above the table as `APP_STORE_CONNECT_ISSUER_ID`.
5. Encode the key and copy it, then paste it as `APP_STORE_CONNECT_KEY_P8`:

   ```sh
   base64 -i AuthKey_ABC123DEF4.p8 | pbcopy
   ```

To check the key before a release, ask Apple for the team's notarization history with it:

```sh
xcrun notarytool history --key AuthKey_ABC123DEF4.p8 --key-id ABC123DEF4 --issuer <issuer ID>
```

If a key leaks, revoke it on the same page and make a new one.

Users who first ran an ad-hoc build have to grant Accessibility once more after updating to a signed one, since macOS ties the permission to the signature.

## Checking a download

Anyone can check a file they downloaded against the release:

```sh
sha256sum --check --ignore-missing SHA256SUMS     # on macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
gh attestation verify snappy-app_0.1.1_amd64.deb --repo farhadzaidi/snappy
```

The second command shows the file was built by this repository's release workflow, from the tagged commit.
