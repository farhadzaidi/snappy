# Package managers

This page is for the maintainer keeping Snappy's Homebrew and winget packages up to date. It covers the one token both need, how the workflow runs, the one-time setup for each, and what to do when something fails.

When you publish a release on GitHub, the **Publish packages** workflow (`.github/workflows/publish-packages.yml`) updates both:

| Channel | For | Package |
|---|---|---|
| Homebrew | macOS | cask `snappy` in `farhadzaidi/homebrew-tap` |
| winget | Windows | `FarhadZaidi.Snappy` in `microsoft/winget-pkgs` |

Both publish with one token, the `PACKAGE_PUBLISHING_TOKEN` secret. Each channel is a job of its own. Without the secret, a job still builds and checks its package, then skips publishing, with a notice that names the missing secret.

## The publishing token

One classic personal access token publishes to both channels. It has to be classic: winget's pull requests go to a repository you don't own, which fine-grained tokens can't reach.

1. On GitHub, go to Settings → Developer settings → Personal access tokens → Tokens (classic) → Generate new token (classic):
   - **Note:** Snappy releases
   - **Scopes:** `public_repo` only. It covers pushing to the tap and opening pull requests on winget-pkgs.
   - **Expiration:** up to a year. Note the date; publishing stops when it expires.

2. Save it as a secret of the Snappy repository:

   ```sh
   gh secret set PACKAGE_PUBLISHING_TOKEN --repo farhadzaidi/snappy
   ```

When it expires, make a new one the same way and set the secret again.

## How it runs

- **Publishing a release** runs it. Publishing a draft counts; creating a draft doesn't.
- **Prereleases** are skipped: package managers only get stable versions like `1.2.3`.
- **By hand**, from the Actions tab (Publish packages → Run workflow) or the command line. It takes a tag and a dry-run switch, which is on unless you turn it off:

  ```sh
  gh workflow run publish-packages.yml --repo farhadzaidi/snappy -f tag=v0.1.0 -f dry_run=true
  ```

  A dry run renders every package, checks it, and prints it in the log, but pushes nothing.

  Running again with `dry_run=false` publishes. That's how you retry a channel that failed, or catch one up after adding its secret. Channels that already have the version are left alone: the tap sees nothing to change, and Komac, the tool that opens winget's pull requests, looks for an existing one first.

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

## Homebrew

Homebrew installs Mac apps as **casks**. Snappy's cask lives in a tap, a repository of your own that Homebrew reads, so it needs no one's approval. Each release, the workflow writes the new version and the disk image's checksum into `Casks/snappy.rb` there and pushes it.

The cask also says what to clean up: uninstalling stops the launch agent and quits Snappy, and `--zap` removes its settings, state, and logs.

### Setup

1. Create the tap. It must be public, named `homebrew-tap`, and have at least one commit:

   ```sh
   gh repo create farhadzaidi/homebrew-tap --public --add-readme --description "Homebrew tap for Snappy"
   ```

2. Set up [the publishing token](#the-publishing-token), if it isn't already.

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

2. Set up [the publishing token](#the-publishing-token), if it isn't already.

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

## When something fails

- **The token expired.** Make a new one [the same way](#the-publishing-token), set the secret again, and run the workflow by hand for the release with `dry_run=false`.
- **The cask needs a change.** Edit the template in `packaging/` and check it with a local render or a dry run. The next release carries it. To fix the current release, run the workflow by hand.
- **winget's metadata needs a change.** After the first version, winget's description, tags, and the like come from the previous version in winget-pkgs, not from `packaging/winget/`. Change them in a pull request to winget-pkgs, and in `packaging/winget/` to keep the two alike.
- **winget's pull request needs a fix.** Komac's branch is in your fork; push to it, or close the pull request and run the workflow again.
