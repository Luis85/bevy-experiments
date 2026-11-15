#requires -Version 5.1
# Playtest package helpers shared by scripts/package_playtest.ps1 and scripts/verify_playtest.ps1.
# Dot-source it after lib\Native.ps1: . (Join-Path $PSScriptRoot 'lib\Playtest.ps1')
# Nothing here starts a process; the two scripts do that through Invoke-Native.
# Self-tests: scripts/tests/run_script_tests.ps1.

# ZipFile lives in System.IO.Compression.FileSystem on .NET Framework 4.x (Windows PowerShell 5.1).
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem

# The size limit of the whole packaged docs folder. The curated list is $CuratedDocs in package_playtest.ps1;
# repo_checks::playtest_guide_has_no_links_outside_the_package pins that list and holds the same limit.
$script:PlaytestDocsLimitBytes = 65536
$script:PlaytestManifestVersion = 2

function Get-PackageRelativePath {
    # The '/'-separated path of a file below a root. Stands in for [IO.Path]::GetRelativePath, which .NET Framework
    # does not have.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + '\'
    $full = [IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) { throw "Not below ${Root}: $Path" }
    return $full.Substring($rootFull.Length).Replace('\', '/')
}

function Get-WorkspaceVersion {
    # The [workspace.package] version of <Workspace>\Cargo.toml.
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$Workspace)
    $inPackage = $false
    foreach ($line in [IO.File]::ReadAllLines((Join-Path $Workspace 'Cargo.toml'))) {
        $trimmed = $line.Trim()
        if ($trimmed.StartsWith('[')) { $inPackage = ($trimmed -eq '[workspace.package]'); continue }
        if ($inPackage -and $trimmed -match '^version\s*=\s*"([^"]+)"') { return $Matches[1] }
    }
    throw "No [workspace.package] version in $Workspace\Cargo.toml"
}

function Get-BuildCommitLabel {
    # What LASTLIGHT_BUILD_COMMIT is set to for a package build: the 7-digit commit, plus '+dirty' for a dirty tree.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Commit,
        [bool]$Dirty = $false
    )
    if ($Commit -cnotmatch '^[0-9a-f]{40}$') { throw "Not a full commit id: $Commit" }
    $label = $Commit.Substring(0, 7)
    if ($Dirty) { $label += '+dirty' }
    return $label
}

function Invoke-WithBuildCommit {
    # Runs the -Build block (package_playtest.ps1's cargo build) with LASTLIGHT_BUILD_COMMIT set to -Label, returns
    # the block's output, and restores the variable's earlier value (or its absence) afterwards, also when the block
    # throws. option_env!("LASTLIGHT_BUILD_COMMIT") rebuilds horde-defense when the value changes, so the build and
    # the copy of its executable belong to one package run.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][scriptblock]$Build
    )
    $savedCommit = [Environment]::GetEnvironmentVariable('LASTLIGHT_BUILD_COMMIT', 'Process')
    try {
        [Environment]::SetEnvironmentVariable('LASTLIGHT_BUILD_COMMIT', $Label, 'Process')
        return (& $Build)
    }
    finally { [Environment]::SetEnvironmentVariable('LASTLIGHT_BUILD_COMMIT', $savedCommit, 'Process') }
}

function Get-BuildStamp {
    # The stamp build_info.rs prints for that build: 'v0.1.0 (abc1234)' or 'v0.1.0 (abc1234+dirty)'.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$Commit,
        [bool]$Dirty = $false
    )
    return ('v{0} ({1})' -f $Version, (Get-BuildCommitLabel -Commit $Commit -Dirty $Dirty))
}

function Get-PlaytestPackageName {
    # Lastlight-playtest-v<version>-<sha7>-<utc>; the zip adds '.zip'.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][DateTime]$CreatedUtc
    )
    return ('Lastlight-playtest-v{0}-{1}-{2}' -f $Version, $Commit.Substring(0, 7), $CreatedUtc.ToString('yyyyMMdd-HHmmss'))
}

function Get-PackageFileRecord {
    # One record per file below the package root (path, bytes, sha256), sorted by path with ordinal comparison.
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$PackageRoot)
    $paths = New-Object System.Collections.Generic.List[string]
    foreach ($file in @(Get-ChildItem -LiteralPath $PackageRoot -File -Recurse -Force)) {
        $paths.Add((Get-PackageRelativePath -Root $PackageRoot -Path $file.FullName))
    }
    $sorted = $paths.ToArray()
    [Array]::Sort($sorted, [StringComparer]::Ordinal)
    $records = New-Object System.Collections.Generic.List[object]
    foreach ($relative in $sorted) {
        $full = Join-Path $PackageRoot $relative.Replace('/', '\')
        $records.Add([ordered]@{
                path   = $relative
                bytes  = (Get-Item -LiteralPath $full -Force).Length
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $full).Hash.ToLowerInvariant()
            })
    }
    return $records.ToArray()
}

function New-PlaytestManifest {
    # Manifest version 2, keys in this order. Write it with Write-Utf8NoBom (ConvertTo-Json -Depth 6).
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$GameVersion,
        [Parameter(Mandatory = $true)][string]$Commit,
        [bool]$Dirty = $false,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Branch,
        [Parameter(Mandatory = $true)][string]$Profile,
        [AllowEmptyCollection()][string[]]$Features = @(),
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Rustc,
        [Parameter(Mandatory = $true)][DateTime]$CreatedUtc,
        [Parameter(Mandatory = $true)][string[]]$CuratedDocs,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$Files
    )
    return [ordered]@{
        version      = $script:PlaytestManifestVersion
        game         = 'Lastlight: Horde Defense'
        game_version = $GameVersion
        stamp        = (Get-BuildStamp -Version $GameVersion -Commit $Commit -Dirty $Dirty)
        commit       = $Commit
        commit_short = $Commit.Substring(0, 7)
        dirty        = $Dirty
        branch       = $Branch
        profile      = $Profile
        features     = @($Features)
        platform     = 'Windows x64'
        'rustc'      = $Rustc
        created_utc  = $CreatedUtc.ToString('o')
        curated_docs = @($CuratedDocs)
        files        = @($Files)
    }
}

function Test-DocLinkClosure {
    # Every Markdown file below docs\ in the package may only link (inline '[text](target)') to the web, to an
    # in-page anchor, or to a file that exists inside the package. Returns one line per dead link; none means the
    # shipped docs are closed. Packaging throws on any.
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$PackageRoot)
    $rootFull = [IO.Path]::GetFullPath($PackageRoot).TrimEnd('\', '/') + '\'
    $dead = New-Object System.Collections.Generic.List[string]
    $docs = Join-Path $PackageRoot 'docs'
    if (-not (Test-Path -LiteralPath $docs -PathType Container)) { return $dead.ToArray() }
    foreach ($file in @(Get-ChildItem -LiteralPath $docs -File -Recurse -Filter '*.md' | Sort-Object FullName)) {
        $name = Get-PackageRelativePath -Root $PackageRoot -Path $file.FullName
        $number = 0
        foreach ($line in [IO.File]::ReadAllLines($file.FullName)) {
            $number += 1
            foreach ($match in [regex]::Matches($line, '\]\(\s*([^)\s]+)[^)]*\)')) {
                $target = $match.Groups[1].Value
                if ($target -match '^(?i)(https?:|mailto:)' -or $target.StartsWith('#')) { continue }
                $relative = [Uri]::UnescapeDataString(($target -split '#', 2)[0])
                $resolved = $null
                try { $resolved = [IO.Path]::GetFullPath((Join-Path $file.DirectoryName $relative.Replace('/', '\'))) } catch { $resolved = $null }
                $inside = $null -ne $resolved -and $resolved.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)
                if (-not $inside -or -not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
                    $dead.Add("${name}:${number}: '$target' does not resolve inside the package")
                }
            }
        }
    }
    return $dead.ToArray()
}

function Get-ZipEntryName {
    # The entry names of a zip, in archive order, exactly as stored.
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$Path)
    $names = New-Object System.Collections.Generic.List[string]
    $archive = [IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($Path))
    try { foreach ($entry in $archive.Entries) { $names.Add($entry.FullName) } }
    finally { $archive.Dispose() }
    return $names.ToArray()
}

function Get-ZipEntryViolation {
    # Entries a package zip must not have: a backslash (Compress-Archive under 5.1 writes them, and they do not
    # unpack as folders elsewhere), a rooted or drive path, or a '..' segment. Returns one line per bad entry.
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$Path)
    $bad = New-Object System.Collections.Generic.List[string]
    foreach ($name in @(Get-ZipEntryName -Path $Path)) {
        if ($name.Contains('\')) { $bad.Add("backslash entry: $name"); continue }
        if ($name.StartsWith('/') -or $name.Contains(':')) { $bad.Add("rooted entry: $name"); continue }
        if (@($name -split '/') -contains '..') { $bad.Add("parent entry: $name") }
    }
    return $bad.ToArray()
}

function New-PlaytestZip {
    # Zips the files of a folder (not the folder itself) with forward-slash entry names, in ordinal path order, and
    # checks the result. Each entry is named here: under powershell.exe, ZipFile.CreateFromDirectory writes backslash
    # names (the host runs with the pre-4.6.1 compatibility switch), as Compress-Archive does.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$SourceDirectory,
        [Parameter(Mandatory = $true)][string]$Destination
    )
    $source = [IO.Path]::GetFullPath($SourceDirectory)
    $zip = [IO.Path]::GetFullPath($Destination)
    if (Test-Path -LiteralPath $zip) { throw "Zip already exists: $zip" }
    $names = New-Object System.Collections.Generic.List[string]
    foreach ($file in @(Get-ChildItem -LiteralPath $source -File -Recurse -Force)) {
        $names.Add((Get-PackageRelativePath -Root $source -Path $file.FullName))
    }
    $sorted = $names.ToArray()
    [Array]::Sort($sorted, [StringComparer]::Ordinal)
    $stream = New-Object IO.FileStream($zip, [IO.FileMode]::CreateNew)
    try {
        $archive = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create)
        try {
            foreach ($name in $sorted) {
                $full = Join-Path $source $name.Replace('/', '\')
                $entry = $archive.CreateEntry($name, [IO.Compression.CompressionLevel]::Optimal)
                $entry.LastWriteTime = [DateTimeOffset](Get-Item -LiteralPath $full -Force).LastWriteTime
                $target = $entry.Open()
                try {
                    $reader = [IO.File]::OpenRead($full)
                    try { $reader.CopyTo($target) } finally { $reader.Dispose() }
                }
                finally { $target.Dispose() }
            }
        }
        finally { $archive.Dispose() }
    }
    finally { $stream.Dispose() }
    $bad = @(Get-ZipEntryViolation -Path $zip)
    if ($bad.Count -gt 0) {
        Remove-Item -LiteralPath $zip -Force
        throw "The zip has unsafe entries: $($bad -join '; ')"
    }
    return $zip
}

function Expand-PlaytestZip {
    # Unpacks a package zip after the entry check; throws on an unsafe entry and unpacks nothing.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Destination
    )
    $bad = @(Get-ZipEntryViolation -Path $Path)
    if ($bad.Count -gt 0) { throw "Unsafe archive: $($bad -join '; ')" }
    [IO.Compression.ZipFile]::ExtractToDirectory([IO.Path]::GetFullPath($Path), [IO.Path]::GetFullPath($Destination))
}

function Test-PackageIntegrity {
    # The manifest's file list against the unpacked package: every listed file is inside the package with the listed
    # size and SHA-256, and nothing but manifest.json is unlisted. Returns one line per problem.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][string]$PackageRoot
    )
    $problems = New-Object System.Collections.Generic.List[string]
    $rootFull = [IO.Path]::GetFullPath($PackageRoot).TrimEnd('\', '/') + '\'
    $listed = @{}
    foreach ($entry in @($Manifest.files)) {
        $relative = [string]$entry.path
        $full = $null
        try { $full = [IO.Path]::GetFullPath((Join-Path $PackageRoot $relative.Replace('/', '\'))) } catch { $full = $null }
        if ($null -eq $full -or $relative.Contains('\') -or -not $full.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) {
            $problems.Add("manifest path escapes the package: $relative"); continue
        }
        $listed[$relative.ToLowerInvariant()] = $true
        if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { $problems.Add("missing file: $relative"); continue }
        if ((Get-Item -LiteralPath $full -Force).Length -ne [long]$entry.bytes) { $problems.Add("size differs: $relative"); continue }
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $full).Hash.ToLowerInvariant() -cne [string]$entry.sha256) { $problems.Add("sha256 differs: $relative") }
    }
    foreach ($file in @(Get-ChildItem -LiteralPath $PackageRoot -File -Recurse -Force)) {
        $relative = Get-PackageRelativePath -Root $PackageRoot -Path $file.FullName
        if ($relative -eq 'manifest.json') { continue }
        if (-not $listed.ContainsKey($relative.ToLowerInvariant())) { $problems.Add("unlisted file: $relative") }
    }
    return $problems.ToArray()
}

function Test-PlaytestManifest {
    # The manifest v2 rules beyond integrity: version 2, a 40-digit commit, the stamp that commit gives, the
    # workspace's game version, a features list, docs equal to curated_docs and at most 65,536 B, a docs folder
    # without dead links, and nothing from archive\ or docs\audio\. Returns one line per problem.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][string]$PackageRoot,
        [Parameter(Mandatory = $true)][string]$GameVersion
    )
    $problems = New-Object System.Collections.Generic.List[string]
    $names = @($Manifest.PSObject.Properties | ForEach-Object { $_.Name })
    if ($Manifest.version -ne $script:PlaytestManifestVersion) {
        $problems.Add("manifest version is '$($Manifest.version)', expected $script:PlaytestManifestVersion")
        return $problems.ToArray()
    }
    foreach ($key in @('game', 'game_version', 'stamp', 'commit', 'commit_short', 'dirty', 'branch', 'profile', 'features', 'platform', 'rustc', 'created_utc', 'curated_docs', 'files')) {
        if ($names -notcontains $key) { $problems.Add("manifest has no '$key'") }
    }
    if ($problems.Count -gt 0) { return $problems.ToArray() }
    $commit = [string]$Manifest.commit
    if ($commit -cnotmatch '^[0-9a-f]{40}$') { $problems.Add("commit is not 40 lower-case hex digits: '$commit'") }
    else {
        if ([string]$Manifest.commit_short -cne $commit.Substring(0, 7)) { $problems.Add("commit_short '$($Manifest.commit_short)' is not the start of the commit") }
        $expected = Get-BuildStamp -Version ([string]$Manifest.game_version) -Commit $commit -Dirty ([bool]$Manifest.dirty)
        if ([string]$Manifest.stamp -cne $expected) { $problems.Add("stamp '$($Manifest.stamp)' is not '$expected'") }
    }
    if ([string]$Manifest.game_version -cne $GameVersion) { $problems.Add("game_version '$($Manifest.game_version)' is not the workspace version '$GameVersion'") }
    $curated = @($Manifest.curated_docs | ForEach-Object { [string]$_ })
    if ($curated.Count -eq 0) { $problems.Add('curated_docs is empty') }
    $docs = @($Manifest.files | Where-Object { ([string]$_.path) -like 'docs/*' })
    $docNames = @($docs | ForEach-Object { ([string]$_.path).Substring(5) })
    $difference = @(Compare-Object -ReferenceObject @($curated | Sort-Object) -DifferenceObject @($docNames | Sort-Object) -CaseSensitive)
    if ($difference.Count -gt 0) { $problems.Add("docs are [$($docNames -join ', ')], curated_docs is [$($curated -join ', ')]") }
    $docBytes = [long]0
    foreach ($doc in $docs) { $docBytes += [long]$doc.bytes }
    if ($docBytes -gt $script:PlaytestDocsLimitBytes) { $problems.Add("docs are $docBytes B, above $script:PlaytestDocsLimitBytes B") }
    foreach ($entry in @($Manifest.files)) {
        $path = ([string]$entry.path).ToLowerInvariant()
        if ($path.StartsWith('archive/') -or $path.Contains('/archive/')) { $problems.Add("archived file in the package: $($entry.path)") }
        # The v1 audition reel stays in the source tree only (ruling S19, W7-03).
        if ($path.StartsWith('docs/audio/')) { $problems.Add("audition reel in the package: $($entry.path)") }
    }
    foreach ($dead in @(Test-DocLinkClosure -PackageRoot $PackageRoot)) { $problems.Add("dead doc link: $dead") }
    return $problems.ToArray()
}
