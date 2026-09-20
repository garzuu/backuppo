[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$BinaryPath = (Join-Path $PSScriptRoot "..\..\bkpo.exe"),
    [string]$ConfigPath,
    [string]$InstallDirectory = "$env:ProgramFiles\Backuppo",
    [string]$DataDirectory = "$env:ProgramData\Backuppo",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$serviceName = "Backuppo"

if ($Uninstall) {
    if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
        if ($PSCmdlet.ShouldProcess($serviceName, "Stop and remove Windows service")) {
            Stop-Service -Name $serviceName -ErrorAction SilentlyContinue
            & sc.exe delete $serviceName | Out-Null
            Write-Host "Service $serviceName removed. Configuration and history were preserved."
        }
    }
    return
}

if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
    throw "bkpo.exe not found: $BinaryPath"
}
if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
    throw "ConfigPath is required when installing the service."
}
if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) {
    throw "Configuration file not found: $ConfigPath"
}
if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
    throw "Service $serviceName already exists. Run this script with -Uninstall first."
}

if ($PSCmdlet.ShouldProcess($serviceName, "Install Windows service")) {
    New-Item -ItemType Directory -Force -Path $InstallDirectory, $DataDirectory | Out-Null
    $installedBinary = Join-Path $InstallDirectory "bkpo.exe"
    $installedConfig = Join-Path $DataDirectory "config.yaml"
    Copy-Item -LiteralPath $BinaryPath -Destination $installedBinary -Force
    Copy-Item -LiteralPath $ConfigPath -Destination $installedConfig -Force

    $binPath = '"{0}" service --config "{1}"' -f $installedBinary, $installedConfig
    & sc.exe create $serviceName binPath= $binPath start= auto DisplayName= "Backuppo Backup Agent" | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "sc.exe create failed with exit code $LASTEXITCODE"
    }
    & sc.exe description $serviceName "Scheduled backups with automatic restore verification" | Out-Null
    & sc.exe failure $serviceName reset= 86400 actions= restart/10000/restart/30000/""/0 | Out-Null
    Start-Service -Name $serviceName
    Write-Host "Backuppo installed and started. Config: $installedConfig"
}
