#requires -Version 5.1
<#
.SYNOPSIS
Downloads the official Minecraft server jar, extracts its data pack, and runs
Mojang's data generator to prepare the files Voxel Arcade needs for one version.

Only the Rust server needs to be shipped: this script is a development tool.
Run it once per Minecraft version, then commit data/versions/<version>/.

Example:
  powershell -ExecutionPolicy Bypass -File scripts/prepare-data.ps1 -Version 26.3
#>
param(
    [string]$Version = "26.3",
    [string]$RepoRoot = (Split-Path -Parent $PSScriptRoot),
    [string]$JavaExe = $env:JAVA_EXE,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression.FileSystem

$cache = Join-Path $env:TEMP "voxel-arcade-data"
$versionDir = Join-Path $RepoRoot "data/versions/$Version"
$reportsDir = Join-Path $versionDir "generated/reports"
New-Item -ItemType Directory -Force -Path $cache, $versionDir, $reportsDir | Out-Null

Write-Host "Resolving Minecraft $Version from Mojang's manifest..."
$manifest = Invoke-RestMethod -Uri "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json" -TimeoutSec 60
$summary = $manifest.versions | Where-Object { $_.id -eq $Version } | Select-Object -First 1
if (-not $summary) { throw "Minecraft version $Version was not found in Mojang's manifest." }
$meta = Invoke-RestMethod -Uri $summary.url -TimeoutSec 60

$serverJar = Join-Path $cache "server-$Version.jar"
if ($Force -or -not (Test-Path $serverJar)) {
    Write-Host "Downloading $($meta.downloads.server.url)"
    Invoke-WebRequest -Uri $meta.downloads.server.url -OutFile $serverJar -TimeoutSec 900
}
$expected = $meta.downloads.server.sha1
$actual = (Get-FileHash -Algorithm SHA1 -LiteralPath $serverJar).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "server.jar checksum mismatch: expected $expected, got $actual" }

function Find-Java {
    param([string]$Explicit, [int]$Required)
    if ($Explicit) { return $Explicit }
    $cmd = Get-Command java -ErrorAction SilentlyContinue
    if ($cmd) {
        $text = (& java -version 2>&1 | Out-String)
        if ($text -match '"(\d+)') {
            $major = [int]$Matches[1]
            if ($major -ge $Required) { return $cmd.Source }
        }
        Write-Host "System Java is older than $Required; downloading a runtime."
    }
    $os = if ($env:OS -eq "Windows_NT") { "windows" } elseif (Test-Path "/proc/self") { "linux" } else { "mac" }
    $arch = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
    $zip = Join-Path $cache "jre-$Required-$os-$arch.zip"
    if ($Force -or -not (Test-Path $zip)) {
        $url = "https://api.adoptium.net/v3/binary/latest/$Required/ga/$os/$arch/jre/hotspot/normal/eclipse"
        Write-Host "Downloading Java $Required from $url"
        Invoke-WebRequest -Uri $url -OutFile $zip -TimeoutSec 900
    }
    $dir = Join-Path $cache "jre-$Required-$os-$arch"
    if ($Force -or -not (Test-Path $dir)) {
        Expand-Archive -LiteralPath $zip -DestinationPath $dir -Force
    }
    $exe = Get-ChildItem -LiteralPath $dir -Recurse -File |
        Where-Object { $_.Name -eq "java.exe" -or $_.Name -eq "java" } |
        Select-Object -First 1
    if (-not $exe) { throw "Downloaded Java runtime has no java executable." }
    return $exe.FullName
}

$java = Find-Java -Explicit $JavaExe -Required ([int]$meta.javaVersion.majorVersion)
Write-Host "Using Java: $java"

$generated = Join-Path $versionDir "generated"
if ($Force -or -not (Test-Path (Join-Path $reportsDir "packets.json"))) {
    Write-Host "Running Mojang's data generator (about a minute)..."
    if (Test-Path $generated) { Remove-Item -LiteralPath $generated -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $generated | Out-Null
    Push-Location $cache
    try {
        & $java "-DbundlerMainClass=net.minecraft.data.Main" -jar $serverJar --reports --output $generated
        if ($LASTEXITCODE -ne 0) { throw "Mojang data generator failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

# Unpack the bundler jar's inner server jar.
$innerJar = Join-Path $cache "server-$Version-inner.jar"
if ($Force -or -not (Test-Path $innerJar)) {
    $zip = [System.IO.Compression.ZipFile]::OpenRead($serverJar)
    try {
        $listEntry = $zip.Entries | Where-Object { $_.FullName -eq "META-INF/versions.list" } | Select-Object -First 1
        if (-not $listEntry) {
            Copy-Item $serverJar $innerJar -Force
        } else {
            $reader = New-Object System.IO.StreamReader($listEntry.Open())
            try { $line = $reader.ReadLine() } finally { $reader.Close() }
            $innerName = ($line -split "`t")[2]
            $entry = $zip.Entries | Where-Object { $_.FullName -eq "META-INF/versions/$innerName" } | Select-Object -First 1
            if (-not $entry) { throw "Inner server jar $innerName is missing." }
            $input = $entry.Open()
            $output = [System.IO.File]::Create($innerJar)
            try { $input.CopyTo($output) } finally { $output.Close(); $input.Close() }
        }
    } finally { $zip.Dispose() }
}

$synced = @(
    "worldgen/biome", "chat_type", "trim_pattern", "trim_material", "wolf_variant",
    "wolf_sound_variant", "pig_variant", "pig_sound_variant", "frog_variant",
    "cat_variant", "cat_sound_variant", "cow_sound_variant", "cow_variant",
    "chicken_sound_variant", "chicken_variant", "zombie_nautilus_variant",
    "painting_variant", "sulfur_cube_archetype", "dimension_type", "damage_type",
    "banner_pattern", "enchantment", "jukebox_song", "instrument",
    "test_environment", "test_instance", "dialog", "world_clock", "timeline",
    "decorated_pot_pattern", "block_transformer", "worldgen/block_state_provider"
)
$prefixes = $synced | ForEach-Object { "data/minecraft/$_/" }
$prefixes += "data/minecraft/tags/"

$datapack = Join-Path $versionDir "datapack"
if ($Force -or -not (Test-Path $datapack)) {
    if (Test-Path $datapack) { Remove-Item -LiteralPath $datapack -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $datapack | Out-Null
    $zip = [System.IO.Compression.ZipFile]::OpenRead($innerJar)
    $count = 0
    try {
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName
            $wanted = $name.EndsWith(".json") -and ($prefixes | Where-Object { $name.StartsWith($_) } | Select-Object -First 1)
            if (-not $wanted) { continue }
            $target = Join-Path $datapack $name.Substring(5) # strip "data/"
            New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent) | Out-Null
            $input = $entry.Open()
            $output = [System.IO.File]::Create($target)
            try { $input.CopyTo($output) } finally { $output.Close(); $input.Close() }
            $count++
        }
        $versionEntry = $zip.Entries | Where-Object { $_.FullName -eq "version.json" } | Select-Object -First 1
        if ($versionEntry) {
            $input = $versionEntry.Open()
            $output = [System.IO.File]::Create((Join-Path $versionDir "version.json"))
            try { $input.CopyTo($output) } finally { $output.Close(); $input.Close() }
        }
    } finally { $zip.Dispose() }
    Write-Host "Extracted $count data pack files."
}

Copy-Item (Join-Path $reportsDir "packets.json") (Join-Path $RepoRoot "crates/arcade-protocol-data/data/packets-26.3.json") -Force
Set-Content -LiteralPath (Join-Path $versionDir "SOURCE.txt") -Value @"
Minecraft $Version
server.jar: $($meta.downloads.server.url)
sha1: $($meta.downloads.server.sha1)
data generator: $java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports
prepared: $(Get-Date -Format o)
"@

Write-Host "Done. Data is in $versionDir"
Write-Host "Build and run: cargo run --release -p arcade-server -- configs/config.toml"