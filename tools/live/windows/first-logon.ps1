# Runs once, elevated, at the first logon of the Snappy test VM, from the config ISO.
# Installs the OpenSSH server with the host's key and the display driver, keeps the snappy
# desktop session logged in and unlocked, and writes C:\Windows\Temp\snappy-setup.done when
# everything succeeded.
$ErrorActionPreference = 'Stop'
Start-Transcript -Path 'C:\Windows\Temp\snappy-setup.log' -Append

function Set-RegistryValue($Path, $Name, $Value, $Type = 'DWord') {
    if (-not (Test-Path $Path)) {
        New-Item -Path $Path -Force | Out-Null
    }
    New-ItemProperty -Path $Path -Name $Name -Value $Value -PropertyType $Type -Force | Out-Null
}

function Install-OpenSshServer {
    $target = 'C:\Program Files\OpenSSH'
    $staging = 'C:\Windows\Temp\openssh'
    Expand-Archive -Path (Join-Path $PSScriptRoot 'OpenSSH-ARM64.zip') -DestinationPath $staging -Force
    Copy-Item -Path (Join-Path $staging 'OpenSSH-ARM64') -Destination $target -Recurse -Force
    & (Join-Path $target 'install-sshd.ps1')

    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    if ($machinePath -notlike "*$target*") {
        [Environment]::SetEnvironmentVariable('Path', "$machinePath;$target", 'Machine')
    }

    Set-RegistryValue 'HKLM:\SOFTWARE\OpenSSH' 'DefaultShell' 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' 'String'
    New-NetFirewallRule -Name 'sshd' -DisplayName 'OpenSSH Server' -Direction Inbound -Protocol TCP -LocalPort 22 -Action Allow -Profile Any | Out-Null

    # The first start creates C:\ProgramData\ssh with the host keys and sshd_config.
    Set-Service -Name sshd -StartupType Automatic
    Start-Service sshd

    # sshd reads an administrator's keys from this file, and only if just SYSTEM and Administrators can write it.
    $authorizedKeys = 'C:\ProgramData\ssh\administrators_authorized_keys'
    Copy-Item -Path (Join-Path $PSScriptRoot 'authorized_keys') -Destination $authorizedKeys -Force
    icacls.exe $authorizedKeys /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null
    Restart-Service sshd
}

# Setup runs on ramfb; later boots use virtio-gpu, which this driver binds to.
function Install-DisplayDriver {
    pnputil.exe /add-driver (Join-Path $PSScriptRoot 'viogpudo\viogpudo.inf') /install | Out-Null
}

function Enable-PermanentAutoLogon {
    $winlogon = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
    Set-RegistryValue $winlogon 'AutoAdminLogon' '1' 'String'
    Set-RegistryValue $winlogon 'DefaultUserName' 'snappy' 'String'
    Set-RegistryValue $winlogon 'DefaultPassword' 'snappy' 'String'
    Set-RegistryValue $winlogon 'DefaultDomainName' '.' 'String'
    Remove-ItemProperty -Path $winlogon -Name 'AutoLogonCount' -ErrorAction SilentlyContinue
}

function Disable-LockAndSleep {
    powercfg.exe /hibernate off
    foreach ($setting in 'monitor-timeout-ac', 'standby-timeout-ac', 'hibernate-timeout-ac', 'disk-timeout-ac') {
        powercfg.exe /change $setting 0
    }

    Set-RegistryValue 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\Personalization' 'NoLockScreen' 1
    Set-RegistryValue 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' 'InactivityTimeoutSecs' 0
    Set-RegistryValue 'HKCU:\Software\Policies\Microsoft\Windows\Control Panel\Desktop' 'ScreenSaveActive' '0' 'String'
    Set-RegistryValue 'HKCU:\Control Panel\Desktop' 'ScreenSaveActive' '0' 'String'
    Set-RegistryValue 'HKCU:\Control Panel\Desktop' 'ScreenSaverIsSecure' '0' 'String'
}

# Elevation still happens, so elevated test apps keep a higher integrity level, but without a prompt.
function Disable-ElevationPrompts {
    $system = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'
    Set-RegistryValue $system 'ConsentPromptBehaviorAdmin' 0
    Set-RegistryValue $system 'PromptOnSecureDesktop' 0
}

# Automatic updates would restart the VM in the middle of a test run.
function Disable-AutomaticUpdates {
    $updates = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU'
    Set-RegistryValue $updates 'NoAutoUpdate' 1
    Set-RegistryValue $updates 'NoAutoRebootWithLoggedOnUsers' 1
}

# The "finish setting up your device" and welcome screens would cover test windows.
function Disable-SetupNags {
    Set-RegistryValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\UserProfileEngagement' 'ScoobeSystemSettingEnabled' 0
    $contentDelivery = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager'
    foreach ($name in 'SubscribedContent-310093Enabled', 'SubscribedContent-338389Enabled', 'SoftLandingEnabled', 'SystemPaneSuggestionsEnabled') {
        Set-RegistryValue $contentDelivery $name 0
    }
}

Install-OpenSshServer
Install-DisplayDriver
Enable-PermanentAutoLogon
Disable-LockAndSleep
Disable-ElevationPrompts
Disable-AutomaticUpdates
Disable-SetupNags

Set-Content -Path 'C:\Windows\Temp\snappy-setup.done' -Value (Get-Date -Format o)
Stop-Transcript
