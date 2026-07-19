param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "debug"
)

$ErrorActionPreference = "Stop"
$taskRepository = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$taskOutput = Join-Path $taskRepository "app\android\app\src\main\jniLibs"

if (-not (Get-Command cargo-ndk -ErrorAction SilentlyContinue)) {
    throw "cargo-ndk 4.1.2 is required. Install it with: cargo install cargo-ndk --version 4.1.2 --locked"
}

Push-Location $taskRepository
try {
    $taskArguments = @(
        "ndk",
        "-t", "arm64-v8a",
        "-t", "x86_64",
        "-o", $taskOutput,
        "build",
        "-p", "mini-ffi"
    )
    if ($Profile -eq "release") {
        $taskArguments += "--release"
    }
    & cargo @taskArguments
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}
