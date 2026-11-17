# The Microsoft Store

Snappy goes to the Microsoft Store as an MSIX bundle holding an x64 and an Arm64 package. Microsoft signs it when it passes certification, so no code-signing certificate is needed.

- `packaging/msix/AppxManifest.xml.template`: the package manifest, with placeholders for the identity, version, and architecture.
- `packaging/msix/Assets/`: the logos, written by `tools/icon/generate.sh` from `assets/icon/snappy.svg`. Commit them again whenever the icon changes.
- `tools/release/msix.ps1`: packs one architecture, or bundles the packed ones.
- `.github/workflows/microsoft-store.yml`: builds both architectures, bundles them, and submits the bundle.

The package's version is the daemon's Cargo version with a fourth part of 0, such as `0.1.0.0`; the Store keeps the fourth part for itself. The repository stores no version of its own for it.

## One-time setup

### 1. A Partner Center account

1. Sign up as an individual developer at [storedeveloper.microsoft.com](https://storedeveloper.microsoft.com). The publisher display name is **Syed F Zaidi**, which must match `PublisherDisplayName` in the manifest.
2. In Partner Center, under **Apps and games**, choose **New product** → **MSIX or PWA app** and reserve the name **Snappy - Window Zones**, the Store listing's name. Then, under **Product management** → **Manage app names**, reserve **Snappy** as well: the package's Start menu name (`DisplayName` in `uap:VisualElements`) is plain "Snappy", and the Store only accepts names reserved for the app. If "Snappy" is taken, set that `DisplayName` to the listing name too.
3. Open the product's **Product management** → **Product identity** page. Note:
   - **Package/Identity/Name**, such as `SyedFZaidi.Snappy-WindowZones`;
   - **Package/Identity/Publisher**, such as `CN=1A2B3C4D-…`;
   - **Package/Properties/PublisherDisplayName**, which should be `Syed F Zaidi`;
   - the **Store ID**, such as `9N…`, which is the product ID.

### 2. Access for the workflow

The workflow signs in to Partner Center as a Microsoft Entra application: a login for software, with its own ID and key. It belongs to the Partner Center account, not to Snappy, so one serves every app the account publishes. Entra's free tier is all it needs; nothing here asks for an Azure subscription or payment.

1. **A tenant.** In Partner Center, under **Account settings** → **Tenants**, choose **Create Microsoft Entra ID**. It asks for a domain, which becomes `<name>.onmicrosoft.com`, and a new global admin login for it. Keep that login: it's how the tenant is managed later. If the tenant doesn't appear in the table afterwards, choose **Associate Microsoft Entra ID** and sign in with the new admin login.
2. **The application.** Under **Account settings** → **User management** → **Microsoft Entra applications**, choose **Add Microsoft Entra application** → **Create Microsoft Entra application**. Name it for what it does, such as "GitHub Store publishing". If it asks for a reply URL or App ID URI, use addresses on the tenant's own domain, such as `https://<name>.onmicrosoft.com`; they aren't used. Give it the **Developer** role, which can upload packages and submit them and nothing more. If a submission is ever refused for lack of permission, raise it to **Manager**.
3. **Its key.** Open the application and, under **Keys**, choose **Add new key**. Copy the **Client ID** and the **Key**; the key is shown once. Note when it expires, since submissions fail after that until it's replaced the same way.
4. **The IDs.** The **tenant ID** is on the application's page and on the **Tenants** page. The **seller ID** is under **Account settings** → **Legal info** → **Developer**.

### 3. The repository's secrets and variables

Under the repository's **Settings** → **Secrets and variables** → **Actions**:

| Secret | Value |
|---|---|
| `MS_STORE_TENANT_ID` | the Microsoft Entra tenant ID |
| `MS_STORE_SELLER_ID` | the Partner Center seller ID |
| `MS_STORE_CLIENT_ID` | the application's client ID |
| `MS_STORE_CLIENT_SECRET` | the application's key |

| Variable | Value |
|---|---|
| `MS_STORE_PRODUCT_ID` | the Store ID |
| `MSIX_IDENTITY_NAME` | Package/Identity/Name |
| `MSIX_PUBLISHER` | Package/Identity/Publisher, the whole `CN=…` |

Until all seven are set, the workflow builds the bundle, keeps it as an artifact, and skips submitting with a notice that names what's missing.

### 4. The first submission, by hand

The Store's submission API only updates an app that's already in the Store, so the first submission goes through Partner Center.

1. Set the two `MSIX_` variables, then run the workflow from the **Actions** tab with the release tag and **dry-run** checked. Download the `msixbundle` artifact.
2. In Partner Center, start the product's first submission:
   - **Pricing and availability**: free, in every market.
   - **Properties**: the category **Productivity** (or **Utilities & tools**), and the privacy policy URL, `https://github.com/farhadzaidi/snappy/blob/main/PRIVACY.md`.
   - **Age ratings**: fill in the questionnaire. Snappy has no user content, no communication between users, no purchases, and no network access, so it comes to the lowest rating.
   - **Packages**: upload `Snappy_<version>.msixupload` from the artifact.
   - **Store listings** (English): the description, a few features, at least one screenshot (1366×768 or larger) of the settings window and of the zones over a desktop, and the short description. The README's opening paragraphs work as the description.
   - **Submission options** → **Restricted capabilities**: explain `runFullTrust`, using the text below.
3. Submit. Certification takes from a few hours to a few days.

The justification for `runFullTrust`:

> Snappy is a desktop window manager. It arranges other apps' windows into zones the user chooses, by moving and resizing them with the Win32 window APIs, registers global keyboard shortcuts with RegisterHotKey, watches for window drags with WinEvent hooks, and shows a notification-area icon. With the user's choice to hold Caps Lock as its shortcut key, it uses a low-level keyboard hook that looks only at Caps Lock and the keys pressed while it's held. None of this is possible from an app container, so Snappy runs as a full-trust desktop app. It has no network access and never reads window titles, window contents, or what the user types.

## Later releases

Publishing a GitHub release runs the workflow for that release's tag:

1. **package** builds the web UI, then the daemon and the app for `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`, each on a Windows runner, and packs each with `tools/release/msix.ps1 -Arch`. It first checks the tag is the Cargo version with a `v` in front.
2. **bundle** combines the two packages with `tools/release/msix.ps1 -Bundle` into `Snappy_<version>.msixbundle` and `Snappy_<version>.msixupload`, kept as the `msixbundle` artifact.
3. **submit** signs in with the Microsoft Store Developer CLI (`msstore`) and runs `msstore publish` with the `.msixupload`. This replaces the pending submission with one copied from the last published submission and commits it, so the listing stays as it was. Listing changes are made in Partner Center.

Certification still takes its usual time after that. Its progress shows in Partner Center, or with `msstore submission status <product ID>`.

To build or submit again for an existing tag, run the workflow from the **Actions** tab. With **dry-run** checked it stops after the bundle.

## Building a package locally

On Windows, with the Windows SDK installed:

```powershell
pnpm -C apps/app/ui build
cargo build --release --target x86_64-pc-windows-msvc -p snappy -p snappy-app
tools/release/msix.ps1 -Arch x64 -IdentityName Snappy.Test -Publisher "CN=Snappy Test"
tools/release/msix.ps1 -Bundle
```

Windows installs only signed packages, so to try one outside the Store, make a certificate whose subject matches `-Publisher`, and sign with it:

```powershell
$cert = New-SelfSignedCertificate -Type Custom -Subject "CN=Snappy Test" -KeyUsage DigitalSignature `
    -FriendlyName "Snappy test packages" -CertStoreLocation Cert:\CurrentUser\My `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
$password = Read-Host -AsSecureString -Prompt "Password for the .pfx"
Export-PfxCertificate -Cert $cert -FilePath snappy-test.pfx -Password $password

tools/release/msix.ps1 -Arch x64 -IdentityName Snappy.Test -Publisher "CN=Snappy Test" `
    -Certificate snappy-test.pfx -CertificatePassword $password
```

Then, in the Windows test VM rather than on your own desktop (see [AGENTS.md](../../AGENTS.md#safety)), trust the certificate and install the package, in an administrator PowerShell:

```powershell
Import-PfxCertificate -FilePath snappy-test.pfx -CertStoreLocation Cert:\LocalMachine\TrustedPeople -Password $password
Add-AppxPackage target\msix\packages\Snappy_0.1.0.0_x64.msix
```

`Remove-AppxPackage` with the package's full name (from `Get-AppxPackage *Snappy*`) uninstalls it, and its files with it.

[Windows](../platforms/windows.md#the-msix-package) explains what changes when Snappy runs from a package.
