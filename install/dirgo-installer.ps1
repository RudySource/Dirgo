$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repository = 'RudySource/Dirgo'
$downloadBase = if ($env:DIRGO_DOWNLOAD_BASE) { $env:DIRGO_DOWNLOAD_BASE } else { "https://github.com/$repository/releases/latest/download" }
$installDirectory = if ($env:DIRGO_INSTALL_DIR) { $env:DIRGO_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }
$asset = 'dirgo-x86_64-pc-windows-msvc.zip'
$temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("dirgo-install-" + [guid]::NewGuid().ToString('N'))
$stagedBinary = $null
$stagedModuleFiles = @()

function Write-Success([string]$Message) {
    Write-Host "✓ $Message" -ForegroundColor Green
}

function Save-Download([string]$Uri, [string]$Destination) {
    if ($Uri.StartsWith('file://')) {
        Copy-Item -LiteralPath ([uri]$Uri).LocalPath -Destination $Destination
    } else {
        if (-not $Uri.StartsWith('https://')) { throw 'Refusing a non-HTTPS release URL.' }
        Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination
    }
}

Write-Host 'DIRGO' -ForegroundColor Blue
Write-Host 'Go anywhere. Instantly.'
Write-Host

try {
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    $archive = Join-Path $temporaryDirectory $asset
    $checksums = Join-Path $temporaryDirectory 'SHA256SUMS'

    Write-Host 'Downloading the verified release for Windows x64...'
    Save-Download "$downloadBase/$asset" $archive
    Save-Download "$downloadBase/SHA256SUMS" $checksums

    $checksumLine = Get-Content $checksums | Where-Object { $_ -match "^[0-9a-fA-F]{64}\s+\*?$([regex]::Escape($asset))$" } | Select-Object -First 1
    if (-not $checksumLine) { throw "SHA256SUMS does not contain $asset" }
    $expected = ($checksumLine -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
    if ($actual -ne $expected) { throw 'Checksum verification failed; nothing was installed.' }
    Write-Success 'Download verified'

    $expanded = Join-Path $temporaryDirectory 'expanded'
    Expand-Archive -LiteralPath $archive -DestinationPath $expanded
    $binary = Get-ChildItem -Path $expanded -Filter 'dgo.exe' -File -Recurse | Select-Object -First 1
    if (-not $binary) { throw 'Release archive does not contain dgo.exe.' }
    $predictorManifest = Get-ChildItem -Path $expanded -Filter 'DirgoPredictor.psd1' -File -Recurse | Select-Object -First 1
    $predictorAssembly = Get-ChildItem -Path $expanded -Filter 'DirgoPredictor.dll' -File -Recurse | Select-Object -First 1
    if (-not $predictorManifest -or -not $predictorAssembly) {
        throw 'Release archive does not contain the PowerShell predictor module.'
    }
    if ($predictorManifest.DirectoryName -ne $predictorAssembly.DirectoryName) {
        throw 'PowerShell predictor files are not from the same release directory.'
    }
    $moduleVersion = Split-Path -Leaf $predictorManifest.DirectoryName
    if ($moduleVersion -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') {
        throw 'PowerShell predictor directory is not versioned.'
    }

    New-Item -ItemType Directory -Force -Path $installDirectory | Out-Null
    $installedBinary = Join-Path $installDirectory 'dgo.exe'
    $stagedBinary = Join-Path $installDirectory ('.dgo-install-' + [guid]::NewGuid().ToString('N') + '.exe')
    Copy-Item -LiteralPath $binary.FullName -Destination $stagedBinary
    & $stagedBinary --version | Out-Null
    Move-Item -LiteralPath $stagedBinary -Destination $installedBinary -Force
    $installedModule = Join-Path $installDirectory "DirgoPredictor/$moduleVersion"
    New-Item -ItemType Directory -Force -Path $installedModule | Out-Null
    foreach ($source in @($predictorManifest, $predictorAssembly)) {
        $stagedModule = Join-Path $installedModule ('.dgo-install-' + [guid]::NewGuid().ToString('N'))
        $stagedModuleFiles += $stagedModule
        Copy-Item -LiteralPath $source.FullName -Destination $stagedModule
        Move-Item -LiteralPath $stagedModule -Destination (Join-Path $installedModule $source.Name) -Force
    }
    Write-Success "Installed to $installedBinary"

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $pathEntries = @($userPath -split ';' | Where-Object { $_ })
    if ($pathEntries -notcontains $installDirectory) {
        $answer = if ($env:DIRGO_SETUP -eq 'yes') {
            'yes'
        } elseif ($env:DIRGO_SETUP -eq 'skip') {
            'no'
        } else {
            Read-Host "`nAdd $installDirectory to your user PATH? [Y/n]"
        }
        if ([string]::IsNullOrWhiteSpace($answer) -or $answer -match '^(?i:y|yes)$') {
            $newPath = (@($pathEntries) + $installDirectory) -join ';'
            [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
            Write-Success 'User PATH updated'
        } else {
            Write-Host "PATH was not changed. Add $installDirectory when ready."
        }
    } else {
        Write-Success 'Already available on your user PATH'
    }

    Write-Host "`nReady. Open a new terminal and run dgo."
} catch {
    Write-Error "Dirgo installer: $($_.Exception.Message)"
    exit 1
} finally {
    if (Test-Path -LiteralPath $temporaryDirectory) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
    }
    if ($stagedBinary -and (Test-Path -LiteralPath $stagedBinary)) {
        Remove-Item -LiteralPath $stagedBinary -Force
    }
    foreach ($stagedModule in $stagedModuleFiles) {
        if (Test-Path -LiteralPath $stagedModule) {
            Remove-Item -LiteralPath $stagedModule -Force
        }
    }
}
