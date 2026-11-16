# Package managers

When you publish a release on GitHub, the **Publish packages** workflow (`.github/workflows/publish-packages.yml`) brings three package managers up to date:

| Channel | For | Package | Secret |
|---|---|---|---|
| Homebrew | macOS | cask `snappy` in `farhadzaidi/homebrew-tap` | `HOMEBREW_TAP_TOKEN` |
| winget | Windows | `FarhadZaidi.Snappy` in `microsoft/winget-pkgs` | `WINGET_TOKEN` |
| AUR | Arch Linux | `snappy-app-bin` | `AUR_SSH_PRIVATE_KEY` |

Each channel is a job of its own. A job whose secret isn't set still renders and checks its package, then skips publishing with a notice that names the missing secret. So you can turn the channels on one at a time, in any order.

## How it runs

- **Publishing a release** runs it. Publishing a draft counts; creating a draft doesn't.
- **Prereleases** are skipped: package managers only get stable versions like `1.2.3`.
- **By hand**, from the Actions tab (Publish packages → Run workflow) or the command line. It takes a tag and a dry-run switch, which is on unless you turn it off:

  ```sh
  gh workflow run publish-packages.yml --repo farhadzaidi/snappy -f tag=v0.1.0 -f dry_run=true
  ```

  A dry run renders every package, checks it, and prints it in the log, but pushes nothing. Running again with `dry_run=false` publishes, which is how you retry a channel that failed or catch one up after adding its secret. Channels that already have the version are left as they are: the tap and the AUR see nothing to change, and Komac looks for an existing pull request first.

The release has to be published for the workflow to see it. To look at the packages while the release is still a draft, render them yourself.

GitHub runs the workflow file as it is in the tagged commit, so a tag made before this workflow existed doesn't run it on publish. Run it by hand for that tag instead.

### Rendering locally

The packages are made from templates in `packaging/` by one script, which the workflow runs too:

```sh
gh release download v0.1.0 --repo farhadzaidi/snappy --pattern SHA256SUMS --dir target/release-files
tools/release/render-packages.sh 0.1.0 target/release-files/SHA256SUMS target/packages
```

That writes:

- `target/packages/homebrew/Casks/snappy.rb`
- `target/packages/winget/manifests/f/FarhadZaidi/Snappy/0.1.0/`, three manifests
- `target/packages/aur/snappy-app-bin/PKGBUILD`

To check the AUR package the way the workflow does, and write its `.SRCINFO`, run the check in an Arch container. Add `--download` once the release is published, to check the tarball's checksum too:

```sh
docker run --rm -v "$PWD:/work" -w /work archlinux:base \
    tools/release/check-aur-package.sh target/packages/aur/snappy-app-bin
```

## Homebrew

Homebrew installs Mac apps as **casks**. Snappy's cask lives in a tap, a repository of your own that Homebrew reads, so it needs no one's approval. Each release, the workflow writes the new version and the disk image's checksum into `Casks/snappy.rb` there and pushes it.

The cask also says what to clean up: uninstalling stops the launch agent and quits Snappy, and `--zap` removes its settings, state, and logs.

### Setup

1. Create the tap. It must be public, named `homebrew-tap`, and have at least one commit:

   ```sh
   gh repo create farhadzaidi/homebrew-tap --public --add-readme --description "Homebrew tap for Snappy"
   ```

2. Make a fine-grained personal access token. On GitHub, go to Settings → Developer settings → Personal access tokens → Fine-grained tokens → Generate new token:
   - **Resource owner:** farhadzaidi
   - **Repository access:** Only select repositories → `farhadzaidi/homebrew-tap`
   - **Permissions:** Repository permissions → Contents → **Read and write**. Metadata, read-only, is added for you.
   - **Expiration:** up to a year. Note the date; publishing stops when it expires.

3. Save it as a secret of the Snappy repository:

   ```sh
   gh secret set HOMEBREW_TAP_TOKEN --repo farhadzaidi/snappy
   ```

### Installing

```sh
brew install --cask farhadzaidi/tap/snappy
```

`brew upgrade` picks up new versions. `brew uninstall --cask --zap snappy` removes Snappy and its files.

Homebrew doesn't get around Gatekeeper. If the disk image isn't signed and notarized, macOS blocks Snappy the first time it opens, whichever way it was installed.

## winget

winget is Windows' package manager. Its packages live in [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs), and every version arrives as a pull request that bots validate, by installing it in a virtual machine, and moderators merge. Snappy's package lists the x64 and arm64 MSIs, installed per user.

The workflow uses [Komac](https://github.com/russellbanks/Komac), a pinned release checked against its checksum. Komac reads the previous version's manifests from winget-pkgs, so it can only update a package that's already there. The **first version is submitted by hand**, below; from then on, each release opens its pull request by itself. Until the first one is merged, the job skips with a notice.

### Setup

1. Fork winget-pkgs. Komac pushes each version to a branch of your fork and opens the pull request from there:

   ```sh
   gh repo fork microsoft/winget-pkgs --clone=false
   ```

2. Make a classic personal access token. Fine-grained tokens can't open pull requests on a repository you don't own. On GitHub, go to Settings → Developer settings → Personal access tokens → Tokens (classic) → Generate new token (classic):
   - **Scopes:** `public_repo` only
   - **Expiration:** up to a year. Note the date.

3. Save it as a secret of the Snappy repository:

   ```sh
   gh secret set WINGET_TOKEN --repo farhadzaidi/snappy
   ```

### The first version

Do this once, after the first release is published.

1. Render the manifests, as in [Rendering locally](#rendering-locally), and read them over in `target/packages/winget/manifests/f/FarhadZaidi/Snappy/<version>/`. Their text comes from `packaging/winget/`; later versions carry it forward.

2. Optionally, on a Windows machine, check and try them. Installing from a local manifest has to be allowed first, from an administrator prompt:

   ```powershell
   winget settings --enable LocalManifestFiles
   winget validate --manifest .\target\packages\winget\manifests\f\FarhadZaidi\Snappy\0.1.0
   winget install --manifest .\target\packages\winget\manifests\f\FarhadZaidi\Snappy\0.1.0
   ```

3. Submit them with Komac. Install it from [its releases](https://github.com/russellbanks/Komac/releases) or with `winget install RussellBanks.Komac`, then, with the same classic token:

   ```sh
   komac token update
   komac submit target/packages/winget/manifests/f/FarhadZaidi/Snappy/0.1.0
   ```

   Komac shows the pull request it opened. To do it without Komac instead, copy the version folder to `manifests/f/FarhadZaidi/Snappy/` in a clone of your fork, commit it on a new branch, push, and open a pull request against `microsoft/winget-pkgs`, filling in its checklist.

4. Watch the pull request. The validation bot comments with what it found; moderators may ask questions. Push fixes to the same branch. Once it's merged, the package shows up in winget within a few hours.

If a release was published while the first pull request was open, run the workflow by hand for it after the merge, with `dry_run=false`.

### Installing

```powershell
winget install FarhadZaidi.Snappy
```

`winget upgrade FarhadZaidi.Snappy` picks up new versions.

## AUR

The Arch User Repository holds build scripts that Arch users build and install with `makepkg` or an AUR helper. `snappy-app-bin` installs the prebuilt release tarball rather than building from source, which is what `-bin` means. The name `snappy` was taken by Arch's compression library, so the package is named after `snappy-app`; it provides and conflicts with `snappy-app`, in case a from-source package appears.

The workflow checks the PKGBUILD in an Arch container (namcap, the `.SRCINFO`, and the tarball's checksum), then pushes it with [github-actions-deploy-aur](https://github.com/KSXGitHub/github-actions-deploy-aur), pinned to a commit. That action writes the `.SRCINFO` again and commits both files.

### Setup

1. Make an account at [aur.archlinux.org](https://aur.archlinux.org/register).

2. Make an SSH key just for publishing. It has no passphrase, because the workflow can't type one:

   ```sh
   ssh-keygen -t ed25519 -C "snappy AUR publishing" -N "" -f ~/.ssh/aur_snappy
   ```

3. On the AUR, open My Account and paste the public key, `~/.ssh/aur_snappy.pub`, into **SSH Public Key**. Save with your password.

4. Save the private key as a secret of the Snappy repository:

   ```sh
   gh secret set AUR_SSH_PRIVATE_KEY --repo farhadzaidi/snappy < ~/.ssh/aur_snappy
   ```

There's nothing to create by hand: the first push makes the `snappy-app-bin` package, owned by your account. Make sure the name is still free at <https://aur.archlinux.org/packages/snappy-app-bin> before the first release.

Every release ships with `pkgrel=1`. To fix the packaging without a new release, clone `ssh://aur@aur.archlinux.org/snappy-app-bin.git`, raise `pkgrel`, run `makepkg --printsrcinfo > .SRCINFO`, and push. Change `packaging/aur/PKGBUILD.template` too, so the next release keeps the fix.

### Installing

With an AUR helper:

```sh
yay -S snappy-app-bin
```

Or by hand:

```sh
git clone https://aur.archlinux.org/snappy-app-bin.git
cd snappy-app-bin
makepkg -si
```

## When something fails

- **A token or key expired.** Make a new one the same way, set the secret again, and run the workflow by hand for the release with `dry_run=false`.
- **The cask or PKGBUILD needs a change.** Edit the template in `packaging/` and check it with a local render or a dry run. The next release carries it. To fix the current release, run the workflow by hand for the cask; for the AUR, raise `pkgrel` by hand as above.
- **winget's metadata needs a change.** After the first version, winget's description, tags, and the like come from the previous version in winget-pkgs, not from `packaging/winget/`. Change them in a pull request to winget-pkgs, and in `packaging/winget/` to keep the two alike.
- **winget's pull request needs a fix.** Komac's branch is in your fork; push to it, or close the pull request and run the workflow again.
