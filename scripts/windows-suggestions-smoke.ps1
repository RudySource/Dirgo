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
foreach ($name in @('Puma', 'Puddle', 'Pulse', 'Public', 'Puppet', 'Purple', 'Puzzle')) {
    New-Item -ItemType Directory -Force (Join-Path $root "Projects/$name") | Out-Null
}
$root = (Resolve-Path -LiteralPath $root).ProviderPath
$target = (Resolve-Path -LiteralPath $target).ProviderPath
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
        $dirgoPredictorRegistered = $predictors.Name -contains 'Dirgo'
        $client = [System.Management.Automation.Subsystem.Prediction.PredictionClient]::new(
            'dirgo-smoke',
            [System.Management.Automation.Subsystem.Prediction.PredictionClientKind]::Terminal)

        function Get-DirgoNativePrediction([string]$InputLine) {
            $tokens = $null
            $errors = $null
            $ast = [System.Management.Automation.Language.Parser]::ParseInput(
                $InputLine, [ref]$tokens, [ref]$errors)
            $context = [System.Management.Automation.Subsystem.Prediction.PredictionContext]::new(
                $ast, $tokens)
            for ($attempt = 0; $attempt -lt 20; $attempt++) {
                $results = [System.Management.Automation.Subsystem.Prediction.CommandPrediction]::PredictInputAsync(
                    $client, $ast, $tokens, 100).GetAwaiter().GetResult()
                $result = $results | Where-Object Name -eq 'Dirgo' | Select-Object -First 1
                if ($result.Suggestions.Count -gt 0) {
                    return $result
                }
                Start-Sleep -Milliseconds 25
            }
            return $result
        }
    }

    Set-Location -LiteralPath $root
    $replacement = Invoke-DirgoSuggestion -BeforeCursor 'Set-Location pun' -AfterCursor ''
    if (-not $replacement.EndsWith("$(Join-Path 'Projects' 'Punk')'")) {
        throw "unexpected suggestion edit: $replacement"
    }
    if ((Get-Location).Path -ne $root) { throw 'requesting a suggestion executed it' }

    if ($dirgoPredictorRegistered) {
        $env:DGO_PREDICTOR_CWD = $root
        $list = Get-DirgoNativePrediction 'dgo pu'
        if ($list.Suggestions.Count -le 5) {
            throw "native ListView returned too few candidates: $($list.Suggestions.Count)"
        }
        if ($list.Suggestions.ToolTip -notmatch '^DIR  ') {
            throw 'native directory suggestions are missing DIR labels'
        }
        $subcommand = Get-DirgoNativePrediction 'dgo sug'
        if ($subcommand.Suggestions.SuggestionText -notcontains 'dgo suggestions') {
            throw 'native predictor did not complete the suggestions subcommand'
        }
        $option = Get-DirgoNativePrediction 'dgo --upd'
        if ($option.Suggestions.SuggestionText -notcontains 'dgo --update') {
            throw 'native predictor did not complete --update'
        }
        if ((Get-Location).Path -ne $root) { throw 'native prediction executed the buffer' }
    }

    dgo Punk
    $current = (Get-Location).Path
    if ((Split-Path -Leaf $current) -ne 'Punk' -or
        (Split-Path -Leaf (Split-Path -Parent $current)) -ne 'Projects' -or
        -not (Test-Path -LiteralPath $current -PathType Container)) {
        throw "PowerShell parent-shell navigation failed: expected $target, got $current"
    }
    Write-Host 'SUGGESTIONS:powershell:ok'
} finally {
    Set-Location -LiteralPath ([System.IO.Path]::GetTempPath())
    Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
