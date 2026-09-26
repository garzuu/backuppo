[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$BinaryPath = (Join-Path $PSScriptRoot "bkpo.exe"),
    [string]$ResticPath = (Join-Path $PSScriptRoot "restic.exe"),
    [string]$ConfigPath = (Join-Path $PSScriptRoot "config\agent-windows.yaml"),
    [string]$InstallDirectory = "$env:ProgramFiles\Backuppo Agent",
    [string]$DataDirectory = "$env:ProgramData\Backuppo",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$serviceName = "Backuppo"

if ($Uninstall) {
    if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
        Stop-Service -Name $serviceName -ErrorAction SilentlyContinue
        & sc.exe delete $serviceName | Out-Null
        Write-Host "Backuppo Agent removed. Configuration and backups were preserved."
    }
    return
}

if (-not (Test-Path $BinaryPath -PathType Leaf)) { throw "bkpo.exe not found: $BinaryPath" }
if (-not (Test-Path $ConfigPath -PathType Leaf)) { throw "Agent config not found: $ConfigPath" }
$existingService = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
if ($existingService) { Stop-Service -Name $serviceName -ErrorAction SilentlyContinue }

New-Item -ItemType Directory -Force -Path $InstallDirectory, $DataDirectory | Out-Null
$installedBinary = Join-Path $InstallDirectory "bkpo.exe"
$installedConfig = Join-Path $DataDirectory "config.yaml"
Copy-Item $BinaryPath $installedBinary -Force
if (Test-Path $ResticPath -PathType Leaf) { Copy-Item $ResticPath (Join-Path $InstallDirectory "restic.exe") -Force }
if (-not (Test-Path $installedConfig)) { Copy-Item $ConfigPath $installedConfig }
& $installedBinary check --config $installedConfig
if ($LASTEXITCODE -ne 0) { throw "Agent configuration validation failed." }

if (-not $existingService) {
    $binPath = '\"{0}\" service --config \"{1}\"' -f $installedBinary, $installedConfig
    & sc.exe create $serviceName binPath= $binPath start= auto DisplayName= "Backuppo Agent" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "sc.exe create failed: $LASTEXITCODE" }
    & sc.exe description $serviceName "Backuppo backup agent with local Web UI" | Out-Null
    & sc.exe failure $serviceName reset= 86400 actions= restart/10000/restart/30000/""/0 | Out-Null
}
Start-Service $serviceName
Write-Host "Backuppo Agent installed. UI: http://127.0.0.1:8787"
