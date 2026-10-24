# Builds Mynah from source on Windows and leaves Mynah.exe in the target directory.
# Used by the Scoop manifest (bucket/mynah.json); also fine to run by hand:
#
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1 -Dir .
#
# Needs: Visual Studio Build Tools with the "Desktop development with C++" workload
# (the one thing no package manager installs for you), plus rustup, Node.js, pnpm,
# cmake and the Vulkan SDK, all of which Scoop pulls in as dependencies.
param(
    [string]$Dir = (Split-Path -Parent (Split-Path -Parent $PSCommandPath))
)
$ErrorActionPreference = "Stop"

function Require-Tool($name, $hint) {
    if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
        throw "$name not found. $hint"
    }
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$msvc = $null
if (Test-Path $vswhere) {
    $msvc = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
}
if (-not $msvc) {
    throw @"
Visual Studio Build Tools with the C++ workload are required to compile Rust on Windows.
Install them once, then re-run:
  winget install Microsoft.VisualStudio.2022.BuildTools --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive"
"@
}

Require-Tool rustup "Install with: scoop install rustup"
Require-Tool pnpm   "Install with: scoop install pnpm"
Require-Tool cmake  "Install with: scoop install cmake"

# Make sure a stable toolchain is active (a fresh rustup has none).
& rustup show active-toolchain 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
    & rustup toolchain install stable --profile minimal
    & rustup default stable
}

# whisper.cpp's Vulkan backend needs the SDK; Scoop's `vulkan` package sets VULKAN_SDK
# for new shells, so pick it up explicitly when we are still in the installing shell.
if (-not $env:VULKAN_SDK) {
    $vk = (& scoop prefix vulkan 2>$null)
    if ($vk -and (Test-Path $vk)) {
        $env:VULKAN_SDK = $vk
        $env:PATH = "$vk\Bin;$env:PATH"
    }
}
if (-not $env:VULKAN_SDK) {
    throw "VULKAN_SDK is not set. Install the Vulkan SDK (scoop install vulkan) or set VULKAN_SDK to it."
}

Push-Location $Dir
try {
    & pnpm install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw "pnpm install failed" }
    & pnpm tauri build --no-bundle --ci
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

    Copy-Item -Force "src-tauri\target\release\mynah.exe" (Join-Path $Dir "Mynah.exe")

    # The build tree is several GB; the exe is self-contained.
    foreach ($d in @("src-tauri\target", "node_modules", "dist")) {
        if (Test-Path $d) { Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue }
    }
    Write-Host "Built $(Join-Path $Dir 'Mynah.exe')"
} finally {
    Pop-Location
}
