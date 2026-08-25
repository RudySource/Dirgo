param(
    [Parameter(Mandatory = $true)]
    [string]$DgoBin
)

$ErrorActionPreference = 'Stop'
$sandbox = Join-Path ([System.IO.Path]::GetTempPath()) ("dirgo-suggestions-" + [guid]::NewGuid().ToString('N'))
$root = Join-Path $sandbox 'filesystem'
$target = Join-Path $root 'Projects/Punk'
$configDir = Join-Path $sandbox 'config/dirgo'
New-Item -ItemType Directory -Force $target, $configDir | Out-Null
$escapedRoot = $root.Replace('\', '\\')
@"
schema_version = 1
roots = ["$escapedRoot"]

[suggestions]
enabled = true
"@ | Set-Content -Encoding utf8 (Join-Path $configDir 'config.toml')

$env:XDG_CONFIG_HOME = Join-Path $sandbox 'config'
$env:XDG_CACHE_HOME = Join-Path $sandbox 'cache'
$env:XDG_STATE_HOME = Join-Path $sandbox 'state'
$env:PATH = (Split-Path -Parent $DgoBin) + [System.IO.Path]::PathSeparator + $env:PATH

try {
    & $DgoBin refresh | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'refresh failed' }

    $integration = & $DgoBin init powershell | Out-String
    if ($LASTEXITCODE -ne 0) { throw 'PowerShell integration generation failed' }
    . ([scriptblock]::Create($integration))

    if ((Get-Command dgo).CommandType -ne 'Function') { throw 'dgo wrapper was not installed' }
    if (Get-Module -ListAvailable PSReadLine) {
        $handler = Get-PSReadLineKeyHandler -Chord Ctrl+f
        if ($handler.Function -ne 'DirgoSuggestion') { throw 'Ctrl+f suggestion handler is missing' }
    }
    $version = (& $DgoBin --version) -replace '^dgo\s+', ''
    $manifest = Join-Path (Split-Path -Parent $DgoBin) "DirgoPredictor/$version/DirgoPredictor.psd1"
    if ($PSVersionTable.PSVersion.Major -eq 7 -and
        $PSVersionTable.PSVersion.Minor -eq 4 -and
        (Test-Path -LiteralPath $manifest)) {
        $predictors = [System.Management.Automation.Subsystem.SubsystemManager]::GetSubsystemInfo(
            [System.Management.Automation.Subsystem.SubsystemKind]::CommandPredictor).Implementations
        if ($predictors.Name -notcontains 'Dirgo') { throw 'Dirgo predictor was not registered' }
    }

    Set-Location -LiteralPath $root
    $replacement = Invoke-DirgoSuggestion -BeforeCursor 'Set-Location pun' -AfterCursor ''
    if (-not $replacement.EndsWith("Projects\Punk'")) {
        throw "unexpected suggestion edit: $replacement"
    }
    if ((Get-Location).Path -ne $root) { throw 'requesting a suggestion executed it' }

    dgo Punk
    if ((Get-Location).Path -ne $target) { throw 'PowerShell parent-shell navigation failed' }
    Write-Host 'SUGGESTIONS:powershell:ok'
} finally {
    Set-Location -LiteralPath ([System.IO.Path]::GetTempPath())
    Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
