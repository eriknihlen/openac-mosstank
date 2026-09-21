#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Assembles the plugin release zip and its .sha256 sidecar.

.DESCRIPTION
  The layout is the one the OpenAC launcher's installer accepts, and every rule
  below is there because the installer enforces it:

  - `plugin.json` sits at the ZIP ROOT, not inside a folder. The installer
    looks for the entry `plugin.json` exactly, by ordinal comparison.
  - The declared `entryDll` sits at the zip root too, found the same way.
  - Every file's extension is on the installer's allowlist: .dll .pdb .json
    .xml .txt .md .png .jpg .jpeg .ttf .otf. A build output carries files
    outside it, so this script copies in what belongs rather than zipping the
    whole output folder.
  - No `runtimes/` directory at any depth.
  - No client assembly beside the plugin: the plugin takes the contract
    compile-only, so the only AcDream assembly in the zip is the plugin's own.
  - The `plugin.json` published as its own release asset has to be
    BYTE-IDENTICAL to the zip's copy, so both come from the same file here.
  - The sidecar is the output of `shasum -a 256 <zip>`: the lowercase hash, a
    space, the zip's file name.

  Asset names are fixed: `<id>-<version>.zip` and `<id>-<version>.zip.sha256`,
  with id and version read from the built manifest.
#>
[CmdletBinding()]
param(
    [string] $Configuration = 'Release',

    # Where the zip, the sidecar and the manifest copy are written.
    [string] $OutputDirectory = 'artifacts'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$buildOutput = Join-Path $repo "src/AcDream.Plugins.MossTank/bin/$Configuration/net10.0"
if (-not (Test-Path $buildOutput)) {
    throw "No build output at $buildOutput. Build the solution first."
}

$manifestPath = Join-Path $buildOutput 'plugin.json'
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$id = $manifest.id
$version = $manifest.version
$entryDll = $manifest.entryDll
if (-not $id -or -not $version -or -not $entryDll) {
    throw "The built plugin.json is missing id, version or entryDll."
}

# What ships. Everything else in the build output stays out, either because the
# launcher's allowlist refuses it or because nothing at runtime reads it.
$payload = @(
    'plugin.json'
    $entryDll
    [System.IO.Path]::ChangeExtension($entryDll, '.pdb')
    [System.IO.Path]::ChangeExtension($entryDll, '.deps.json')
) + (Get-ChildItem $buildOutput -Filter 'mosstank*.xml' | ForEach-Object { $_.Name })

$staging = Join-Path ([System.IO.Path]::GetTempPath()) "mosstank-zip-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force $staging | Out-Null
try {
    foreach ($name in $payload) {
        $source = Join-Path $buildOutput $name
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "The build output has no '$name'."
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $staging $name)
    }

    # The installer's own rules, checked here so a mistake fails the build
    # rather than an install. acdream-plugincheck re-runs all of them against
    # the finished zip with the launcher's real code.
    $allowed = '.dll', '.pdb', '.json', '.xml', '.txt', '.md', '.png', '.jpg', '.jpeg', '.ttf', '.otf'
    foreach ($file in Get-ChildItem $staging -Recurse -File) {
        if ($file.Extension.ToLowerInvariant() -notin $allowed) {
            throw "'$($file.Name)' has an extension the launcher refuses."
        }
        if ($file.Name -like 'AcDream.*.dll' -and $file.Name -ne $entryDll) {
            throw "'$($file.Name)' is a second client assembly beside the plugin."
        }
    }

    $absoluteOutput = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) {
        $OutputDirectory
    }
    else {
        Join-Path $repo $OutputDirectory
    }
    New-Item -ItemType Directory -Force $absoluteOutput | Out-Null

    $zipName = "$id-$version.zip"
    $zipPath = Join-Path $absoluteOutput $zipName
    if (Test-Path -LiteralPath $zipPath) { Remove-Item -Force -LiteralPath $zipPath }
    # -Path with a trailing \* zips the folder's CONTENTS, so plugin.json lands
    # at the zip root instead of under a folder named after the staging dir.
    Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $zipPath -CompressionLevel Optimal

    $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    # shasum's own shape: hash, a space, the file name, one trailing newline.
    [System.IO.File]::WriteAllText("$zipPath.sha256", "$hash  $zipName`n")

    # The release's standalone plugin.json asset, byte-identical to the one the
    # zip carries because it is a copy of the same file.
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $absoluteOutput 'plugin.json') -Force

    Write-Host "id:      $id"
    Write-Host "version: $version"
    Write-Host "zip:     $zipPath"
    Write-Host "sha256:  $hash"
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        foreach ($entry in $archive.Entries) { Write-Host "  $($entry.FullName)" }
    }
    finally { $archive.Dispose() }
}
finally {
    Remove-Item -Recurse -Force $staging -ErrorAction SilentlyContinue
}
