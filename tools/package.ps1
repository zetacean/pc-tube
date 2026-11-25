# Builds the release exe and packs a portable zip: dist\PC-Tube-<version>-win64.zip
# With -Installer it also builds dist\PC-Tube-<version>-setup.exe (needs Inno Setup 6).
# usage: .\tools\package.ps1 [-Installer]
param([switch]$Installer)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
cargo build --release
if ($LASTEXITCODE) { throw 'cargo build --release failed' }

$name = "PC-Tube-$version-win64"
$stage = Join-Path 'dist' $name
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item 'target\release\pc_tube.exe' (Join-Path $stage 'PC Tube.exe')
# the download manager: PC Tube starts it from the folder it is in
Copy-Item 'target\release\pctube_dm.exe' (Join-Path $stage 'PC Tube Downloads.exe')
Copy-Item 'LICENSE', 'README.md' $stage

$zip = Join-Path 'dist' "$name.zip"
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
$mb = [math]::Round((Get-Item $zip).Length / 1MB, 1)
"Listo: $zip ($mb MB)"

if ($Installer) {
    $iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
    if (-not $iscc) {
        $iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
                  "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    if (-not $iscc -and $env:CI) {
        # GitHub's Windows runners normally have it; install it if not.
        choco install innosetup -y --no-progress | Out-Null
        $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
    }
    if (-not $iscc -or -not (Test-Path $iscc)) { throw 'Inno Setup 6 no esta instalado (https://jrsoftware.org/isinfo.php)' }
    & $iscc "/DAppVersion=$version" '/DExePath=..\target\release\pc_tube.exe' '/DDmExePath=..\target\release\pctube_dm.exe' 'installer\pctube.iss'
    if ($LASTEXITCODE) { throw 'iscc failed' }
    "Listo: dist\PC-Tube-$version-setup.exe"
}
