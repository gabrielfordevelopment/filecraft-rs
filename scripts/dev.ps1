param(
    [ValidateSet('run', 'check', 'clippy', 'test', 'build', 'fmt')]
    [string] $Action = 'run'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

# Cargo needs the MSVC and Windows SDK libraries as well as the linker executable.
if (-not $env:VCINSTALLDIR) {
    $locator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $locator)) {
        throw 'Visual Studio Installer was not found. Install the MSVC C++ build tools and Windows SDK.'
    }
    $installation = & $locator -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $installation) {
        throw 'No Visual Studio installation with the MSVC C++ build tools was found.'
    }
    $developerCommand = Join-Path $installation 'Common7/Tools/VsDevCmd.bat'
    $commandText = 'call "{0}" -no_logo -arch=x64 -host_arch=x64 >nul && set' -f $developerCommand
    $environmentLines = & $env:ComSpec /d /s /c $commandText
    if ($LASTEXITCODE -ne 0) {
        throw 'The Visual Studio developer environment could not be initialized.'
    }
    foreach ($line in $environmentLines) {
        if ($line -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
        }
    }
}

$isolatedSettings = Join-Path $repoRoot 'target/manual-test-settings'
$CargoArguments = switch ($Action) {
    'run' { @('run', '--locked', '--release', '--', '--settings-dir', $isolatedSettings) }
    'build' { @('build', '--locked', '--release') }
    'fmt' { @('fmt', '--all', '--', '--check') }
    'clippy' { @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings') }
    default { @($Action, '--locked') }
}

Push-Location -LiteralPath $repoRoot
try {
    & cargo @CargoArguments
    $cargoExit = $LASTEXITCODE
    if ($cargoExit -ne 0) {
        Write-Error "Cargo exited with code $cargoExit."
    }
} finally {
    Pop-Location
}
exit $cargoExit
