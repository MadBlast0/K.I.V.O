# Install/launch smoke test for the NSIS installer (RELEASE.md §2, REL-06, DIST-01/02/05).
# Meant for a clean Windows runner: it installs KIVO for the current user, starts the runtime,
# checks it answers over IPC, upgrades in place while it runs, and uninstalls. Never run it on a
# PC where KIVO is in real use: it replaces the installed copy and removes it at the end.
#
#   pwsh scripts/smoke-install.ps1 -Installer path\to\KIVO_x.y.z_x64-setup.exe
param([Parameter(Mandatory)] [string] $Installer)

$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:LOCALAPPDATA 'Programs\KIVO'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\KIVO'
$config = Join-Path $env:APPDATA 'KIVO\config\kivo.toml'
$runtime = Join-Path $dir 'kivo-runtime.exe'

function Check([bool] $ok, [string] $what) {
    if (-not $ok) { throw "smoke test failed: $what" }
    Write-Host "ok: $what"
}

function Install([string[]] $options) {
    $p = Start-Process $Installer -ArgumentList $options -Wait -PassThru
    Check ($p.ExitCode -eq 0) "installer $($options -join ' ') exits 0 (got $($p.ExitCode))"
}

function Healthy {
    $p = Start-Process $runtime -ArgumentList '--health' -Wait -PassThru
    Check ($p.ExitCode -eq 0) 'the runtime answers over IPC (--health)'
}

function StartupEntry {
    (Get-ItemProperty $runKey -Name KIVO -ErrorAction SilentlyContinue).KIVO
}

# 1. Fresh silent install, with the startup option (DIST-05).
Install @('/S', '/STARTUP')
foreach ($file in 'KIVO.exe', 'kivo-runtime.exe', 'kivo-infer.exe', 'uninstall.exe') {
    Check (Test-Path (Join-Path $dir $file)) "installed $file"
}
Check (Test-Path $uninstallKey) 'uninstaller registered (Apps & features)'
$shortcut = Get-ChildItem ([Environment]::GetFolderPath('Programs')) -Recurse -Filter 'KIVO.lnk' -ErrorAction SilentlyContinue
Check ($null -ne $shortcut) 'Start-menu shortcut'
Check ((StartupEntry) -eq "`"$runtime`" --autostart") 'startup entry registered'
Check (-not (Get-ChildItem $dir -Recurse -Include *.onnx, *.ort, *.bin)) 'no models in the installer'
$size = (Get-Item $Installer).Length / 1MB
Write-Host ('installer size: {0:N1} MB' -f $size)

# 2. First start: the runtime comes up, answers, and keeps the installer's startup choice.
Start-Process $runtime -ArgumentList '--no-app' | Out-Null
Healthy
Check ((Get-Content $config -Raw) -match 'start_with_windows\s*=\s*true') 'first start adopts the startup choice'
Check ($null -ne (StartupEntry)) 'startup entry kept after the first start'

# 3. In-place upgrade while KIVO runs: the installer stops it and replaces the files.
Install @('/S')
Check ($null -eq (Get-Process kivo-runtime -ErrorAction SilentlyContinue)) 'upgrade stopped the runtime'
Check ($null -ne (StartupEntry)) 'upgrade keeps the startup entry'
Start-Process $runtime -ArgumentList '--no-app' | Out-Null
Healthy

# 4. Uninstall (the uninstaller copies itself to %TEMP% and returns at once, so wait for it).
Start-Process (Join-Path $dir 'uninstall.exe') -ArgumentList '/S' -Wait | Out-Null
$deadline = (Get-Date).AddSeconds(60)
while ((Test-Path $runtime) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
Check (-not (Test-Path $runtime)) 'uninstall removed the files'
Check ($null -eq (Get-Process kivo-runtime, kivo-infer, KIVO -ErrorAction SilentlyContinue)) 'no KIVO processes left'
Check ($null -eq (StartupEntry)) 'uninstall removed the startup entry'
Check (-not (Test-Path $uninstallKey)) 'uninstall removed its registration'
Write-Host 'smoke test passed'
