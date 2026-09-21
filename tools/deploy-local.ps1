<#
.SYNOPSIS
  Builds the plugin and copies it over the copy the launcher installed, for a quick local try.
.DESCRIPTION
  The client loads plugins at startup and locks their files while it runs, so it must be closed.
  The installed copy is backed up once per day beside it. A later launcher update replaces
  whatever this put there.
#>
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'acdream\plugins\acdream.mosstank'),
    [switch]$SkipTests
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (Get-Process -Name 'AcDream.App' -ErrorAction SilentlyContinue) { throw 'Close the game first: it holds the plugin files open.' }
if (-not (Test-Path -LiteralPath $InstallDir)) { throw "No installed plugin at '$InstallDir'. Install it from the launcher once first." }
dotnet build $repo -c Release --nologo -v q | Out-Host
if ($LASTEXITCODE) { throw 'Build failed.' }
if (-not $SkipTests) { dotnet test $repo -c Release --no-build --nologo | Out-Host; if ($LASTEXITCODE) { throw 'Tests failed.' } }
$out = Join-Path $repo 'src\AcDream.Plugins.MossTank\bin\Release\net10.0'
$backup = "$InstallDir.backup-$(Get-Date -Format yyyy-MM-dd)"
if (-not (Test-Path -LiteralPath $backup)) { Copy-Item -LiteralPath $InstallDir -Destination $backup -Recurse }
$names = @('AcDream.Plugins.MossTank.dll','AcDream.Plugins.MossTank.pdb','AcDream.Plugins.MossTank.deps.json','plugin.json') +
    @(Get-ChildItem -LiteralPath $out -Filter 'mosstank*.xml' | ForEach-Object Name)
foreach ($n in $names) { Copy-Item -LiteralPath (Join-Path $out $n) -Destination (Join-Path $InstallDir $n) -Force }
Write-Host "deployed $($names.Count) files to $InstallDir (backup: $backup)"
