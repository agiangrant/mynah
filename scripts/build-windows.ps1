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

$msvc = @($msvc)[0]

# Load the Visual Studio developer environment (cl.exe, link.exe, SDK paths) into this
# session. whisper.cpp's Vulkan shader generator configures a nested CMake project that
# needs cl.exe on PATH, which a plain PowerShell window does not provide.
$devcmd = Join-Path $msvc "Common7\Tools\VsDevCmd.bat"
if (-not (Test-Path $devcmd)) { throw "VsDevCmd.bat not found under $msvc" }
$envDump = & cmd.exe /s /c "`"$devcmd`" -arch=x64 -host_arch=x64 -no_logo && set"
foreach ($line in $envDump) {
    if ($line -match '^([^=]+)=(.*)$') {
        [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
}
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) { throw "cl.exe still not on PATH after VsDevCmd" }

# Ninja keeps the nested CMake projects on the same generator and is much faster than MSBuild.
if (Get-Command ninja -ErrorAction SilentlyContinue) { $env:CMAKE_GENERATOR = "Ninja" }

# ggml turns ccache on whenever it finds one, but ccache cannot handle MSVC's /Zi debug
# info, which the Vulkan shader generator's compiler check uses (fatal error C1041).
$env:CCACHE_DISABLE = "1"

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

# whisper.cpp's nested Vulkan tool build nests ~210 characters of directories under the
# cargo target dir; from Scoop's app folder that passes Windows' 260-character path limit
# (fatal error C1041 on vc140.pdb). Build in a short, freshly created, owner-only
# directory under the user's profile instead (never a shared location).
$suffix = -join ((48..57) + (97..122) | Get-Random -Count 6 | ForEach-Object { [char]$_ })
$target = Join-Path $env:USERPROFILE ".mb-$suffix"
if ($target.Length -gt 48) {
    throw "User profile path is too long for the build ($target). Set CARGO_TARGET_DIR to a short directory you own (for example a drive mapped with subst) and re-run."
}
New-Item -ItemType Directory -Path $target -ErrorAction Stop | Out-Null  # fails if it already exists
$env:CARGO_TARGET_DIR = $target

function Remove-BuildDir([string]$path) {
    if (-not (Test-Path -LiteralPath $path)) { return }
    $item = Get-Item -LiteralPath $path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        Write-Warning "Not deleting $path because it is a junction or symlink"
        return
    }
    # rmdir does not follow junctions inside the tree, unlike Remove-Item -Recurse.
    & cmd.exe /c "rmdir /s /q `"$path`""
    if ($LASTEXITCODE -ne 0) { Write-Warning "Could not fully delete $path" }
}

Push-Location $Dir
try {
    & pnpm install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw "pnpm install failed" }
    & pnpm tauri build --no-bundle --ci
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

    Copy-Item -Force (Join-Path $target "release\mynah.exe") (Join-Path $Dir "Mynah.exe")
    Write-Host "Built $(Join-Path $Dir 'Mynah.exe')"
} finally {
    Pop-Location
    # The build tree is several GB; the exe is self-contained.
    Remove-BuildDir $target
    Remove-BuildDir (Join-Path $Dir "node_modules")
    Remove-BuildDir (Join-Path $Dir "dist")
}
