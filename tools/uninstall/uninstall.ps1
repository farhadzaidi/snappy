# Removes Snappy from a Windows PC, however it was installed. Published with each release, so
# people can run, in PowerShell:
#
#   irm https://github.com/farhadzaidi/snappy/releases/latest/download/uninstall.ps1 | iex
#
# With options:
#
#   & ([scriptblock]::Create((irm https://github.com/farhadzaidi/snappy/releases/latest/download/uninstall.ps1))) -KeepSettings
#
#   -KeepSettings   keep Snappy's settings file, for installing it again later
#   -Yes            answer yes to every question
#
# First Snappy undoes its own changes with `snappy.exe --uninstall`: it quits so hidden windows
# come back, turns off Open at login, and deletes its files. Then this removes the program the
# way it was installed: the .msi, which winget uses too, or the -setup.exe. A copy from the
# Microsoft Store is quit the same way first, then removed after asking. It asks once before
# changing anything. Nothing here needs administrator rights. Written for Windows PowerShell 5.1, which every Windows 11 has.
param(
    [switch] $KeepSettings,
    [switch] $Yes
)

$Publisher = 'Syed F Zaidi'
$StorePackage = 'SyedFZaidi.Snappy-WindowZones'
$InstallFolder = Join-Path $env:LOCALAPPDATA 'Programs\Snappy'
# The class of the running daemon's hidden window, which takes a close request as Quit.
$DaemonWindowClass = 'Snappy.Daemon'
$QuitWaitSeconds = 30

function Write-Line([string] $Text) {
    Write-Host $Text
}

function Confirm-Step([string] $Question) {
    if ($Yes) { return $true }
    $answer = Read-Host "$Question [y/N]"
    return $answer -match '^[yY]'
}

# The entries Snappy's installers add under Settings > Apps.
function Get-UninstallEntry {
    $keys = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )
    foreach ($key in $keys) {
        Get-ItemProperty -Path $key -ErrorAction SilentlyContinue |
            Where-Object {
                $_.PSObject.Properties['DisplayName'] -and $_.DisplayName -eq 'Snappy' -and
                $_.PSObject.Properties['Publisher'] -and $_.Publisher -eq $Publisher
            }
    }
}

# The Microsoft Store's copy, by its package name, or else by what its manifest says.
function Get-StorePackage {
    $package = Get-AppxPackage -Name $StorePackage -ErrorAction SilentlyContinue
    if ($package) { return $package }
    Get-AppxPackage -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like '*Snappy*' } |
        Where-Object {
            $manifest = Get-AppxPackageManifest -Package $_.PackageFullName -ErrorAction SilentlyContinue
            $manifest -and $manifest.Package.Properties.PublisherDisplayName -eq $Publisher
        } |
        Select-Object -First 1
}

# Quits a running Snappy the way its menu's Quit does, so every hidden window comes back, and
# waits for it to exit. For the Store's copy, whose program can't be run from outside it.
function Invoke-SnappyQuit {
    if (-not (Get-Process -Name snappy -ErrorAction SilentlyContinue)) { return $true }
    if (-not ('SnappyUninstall.Window' -as [type])) {
        Add-Type -Namespace SnappyUninstall -Name Window -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern System.IntPtr FindWindowW(string className, string windowName);
[DllImport("user32.dll")]
public static extern bool PostMessageW(System.IntPtr window, uint message, System.IntPtr wParam, System.IntPtr lParam);
'@
    }
    $window = [SnappyUninstall.Window]::FindWindowW($DaemonWindowClass, $null)
    if ($window -eq [IntPtr]::Zero) { return $false }
    $closeMessage = 0x0010
    [void] [SnappyUninstall.Window]::PostMessageW($window, $closeMessage, [IntPtr]::Zero, [IntPtr]::Zero)
    Get-Process -Name snappy -ErrorAction SilentlyContinue | Wait-Process -Timeout $QuitWaitSeconds -ErrorAction SilentlyContinue
    return -not (Get-Process -Name snappy -ErrorAction SilentlyContinue)
}

# Runs Snappy's own cleanup with the program at $Program and returns its exit status.
function Invoke-Cleanup([string] $Program) {
    $arguments = @('--uninstall')
    if ($KeepSettings) { $arguments += '--keep-settings' }
    $process = Start-Process -FilePath $Program -ArgumentList $arguments -NoNewWindow -PassThru
    # Reading the handle first keeps the exit code, which Windows PowerShell otherwise loses.
    $null = $process.Handle
    $process.WaitForExit()
    return $process.ExitCode
}

# Without Snappy's program to run, offers to delete the folders it keeps its files in.
function Invoke-LeftoverCleanup {
    if ($KeepSettings) { return }
    $found = @(
        (Join-Path $env:APPDATA 'Snappy'),
        (Join-Path $env:LOCALAPPDATA 'Snappy'),
        (Join-Path $env:LOCALAPPDATA 'io.github.farhadzaidi.Snappy.App')
    ) | Where-Object { Test-Path -LiteralPath $_ }
    if (-not $found) { return }
    Write-Line "Snappy's files are still here:"
    $found | ForEach-Object { Write-Line "  $_" }
    if (Confirm-Step 'Delete them?') {
        $found | ForEach-Object { Remove-Item -LiteralPath $_ -Recurse -Force }
        Write-Line 'Deleted them.'
    }
}

function Uninstall-Snappy {
    $entries = @(Get-UninstallEntry)
    $store = Get-StorePackage
    $program = Join-Path $InstallFolder 'snappy.exe'
    $hasProgram = Test-Path -LiteralPath $program

    if (-not $entries -and -not $store -and -not $hasProgram) {
        Write-Line "Snappy isn't installed."
        Invoke-LeftoverCleanup
        return $true
    }

    $question = 'Remove Snappy and its settings from this computer?'
    if ($KeepSettings) { $question = 'Remove Snappy from this computer, keeping its settings?' }
    if (-not (Confirm-Step $question)) {
        Write-Line 'Nothing was changed.'
        return $false
    }

    if ($hasProgram) {
        Write-Line "Removing Snappy's changes and files..."
        $status = Invoke-Cleanup $program
        if ($status -ne 0) {
            if ($status -eq 2) { Write-Line 'This copy of Snappy is too old to undo its own changes.' }
            if (-not (Confirm-Step "Remove Snappy's program anyway?")) { return $false }
        }
    } elseif ($store) {
        $running = [bool] (Get-Process -Name snappy -ErrorAction SilentlyContinue)
        if (-not (Invoke-SnappyQuit)) {
            Write-Line "Snappy didn't quit. Quit it from its menu, then run this again."
            return $false
        }
        if ($running) { Write-Line 'Quit Snappy.' }
    }

    $removed = $true
    foreach ($entry in $entries) {
        if ($entry.PSChildName -match '^\{[0-9A-Fa-f-]+\}$') {
            Write-Line 'Removing Snappy with Windows Installer...'
            $process = Start-Process -FilePath msiexec.exe -ArgumentList @('/x', $entry.PSChildName, '/passive', '/norestart') -Wait -PassThru
            # 1605: it was already gone.
            if ($process.ExitCode -notin @(0, 1605)) {
                Write-Line "Windows Installer couldn't remove Snappy. Uninstall it in Settings > Apps > Installed apps."
                $removed = $false
            }
        } elseif ($entry.PSObject.Properties['UninstallString']) {
            Write-Line 'Removing Snappy with its uninstaller...'
            $folder = $entry.InstallLocation
            $uninstaller = Join-Path $folder 'uninstall.exe'
            # `_?=` runs the uninstaller in place, so waiting for it waits for the removal.
            $process = Start-Process -FilePath $uninstaller -ArgumentList @('/S', "_?=$folder") -Wait -PassThru
            if ($process.ExitCode -ne 0) {
                Write-Line "Snappy's uninstaller didn't finish. Uninstall it in Settings > Apps > Installed apps."
                $removed = $false
            } else {
                # Run in place, the uninstaller leaves itself and its folder behind.
                Remove-Item -LiteralPath $folder -Recurse -Force -ErrorAction SilentlyContinue
            }
        }
    }
    if (-not $entries -and (Test-Path -LiteralPath $InstallFolder)) {
        Remove-Item -LiteralPath $InstallFolder -Recurse -Force
        Write-Line "Deleted $InstallFolder."
    }

    if ($store) {
        Write-Line 'Snappy came from the Microsoft Store.'
        if (Confirm-Step 'Remove it now?') {
            try {
                Remove-AppxPackage -Package $store.PackageFullName -ErrorAction Stop
                Write-Line 'Removed it.'
            } catch {
                Write-Line "Windows couldn't remove it. Uninstall Snappy in Settings > Apps > Installed apps."
                $removed = $false
            }
        } else {
            Write-Line 'To finish, uninstall Snappy in Settings > Apps > Installed apps.'
            $removed = $false
        }
    }
    return $removed
}

if (Uninstall-Snappy) {
    Write-Line 'Done.'
} else {
    Write-Line "Snappy isn't completely removed."
}
