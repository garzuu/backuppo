[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$BinaryPath = (Join-Path $PSScriptRoot "backuppo-hub.exe"),
    [string]$ConfigPath = (Join-Path $PSScriptRoot "config\hub-windows.yaml"),
    [string]$InstallDirectory = "$env:ProgramFiles\Backuppo Hub",
    [string]$DataDirectory = "$env:ProgramData\BackuppoHub",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$serviceName = "BackuppoHub"

if ($Uninstall) {
    if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
        Stop-Service -Name $serviceName -ErrorAction SilentlyContinue
        & sc.exe delete $serviceName | Out-Null
        Write-Host "Backuppo Hub removed. Database and configuration were preserved."
    }
    return
}

if (-not (Test-Path $BinaryPath -PathType Leaf)) { throw "backuppo-hub.exe not found: $BinaryPath" }
if (-not (Test-Path $ConfigPath -PathType Leaf)) { throw "Hub config not found: $ConfigPath" }
$existingService = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
if ($existingService) { Stop-Service -Name $serviceName -ErrorAction SilentlyContinue }

New-Item -ItemType Directory -Force -Path $InstallDirectory, $DataDirectory | Out-Null
$installedBinary = Join-Path $InstallDirectory "backuppo-hub.exe"
$installedConfig = Join-Path $DataDirectory "config.yaml"
Copy-Item $BinaryPath $installedBinary -Force
if (-not (Test-Path $installedConfig)) { Copy-Item $ConfigPath $installedConfig }
& $installedBinary check --config $installedConfig
if ($LASTEXITCODE -ne 0) { throw "Hub configuration validation failed." }

$serviceKey = "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName"
if (-not $existingService) {
    $binPath = '\"{0}\" service --config \"{1}\"' -f $installedBinary, $installedConfig
    & sc.exe create $serviceName binPath= $binPath start= auto DisplayName= "Backuppo Hub" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "sc.exe create failed: $LASTEXITCODE" }
    & sc.exe description $serviceName "Backuppo multi-site management hub" | Out-Null
    & sc.exe failure $serviceName reset= 86400 actions= restart/10000/restart/30000/""/0 | Out-Null
}
if (-not (Get-ItemProperty -Path $serviceKey -Name Environment -ErrorAction SilentlyContinue)) {
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    $jwtBytes = New-Object byte[] 32
    $policyBytes = New-Object byte[] 32
    $rng.GetBytes($jwtBytes)
    $rng.GetBytes($policyBytes)
    $rng.Dispose()
    $jwt = -join ($jwtBytes | ForEach-Object { $_.ToString("x2") })
    $policy = [Convert]::ToBase64String($policyBytes)
    New-ItemProperty -Path $serviceKey -Name Environment -PropertyType MultiString -Value @("HUB_JWT_SECRET=$jwt", "HUB_POLICY_SIGNING_KEY=$policy", "RUST_LOG=info") -Force | Out-Null
}
Start-Service $serviceName
Write-Host "Backuppo Hub installed. UI: http://localhost:8080"
Write-Host "Create the first user with backuppo-hub.exe create-user --config `"$installedConfig`" --username admin --password-env HUB_ADMIN_PASSWORD"
