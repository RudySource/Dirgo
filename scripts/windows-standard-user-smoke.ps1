param([Parameter(Mandatory)][string]$DgoBin)
$ErrorActionPreference = 'Stop'
$name = 'dgo' + [guid]::NewGuid().ToString('N').Substring(0, 12)
$root = Join-Path $env:PUBLIC $name
$password = ConvertTo-SecureString ([guid]::NewGuid().ToString('N') + '!aA9') -AsPlainText -Force
New-Item -ItemType Directory $root | Out-Null
try {
    New-LocalUser -Name $name -Password $password | Out-Null
    $identity = "$env:COMPUTERNAME\$name"
    & icacls.exe $root /grant "${identity}:(OI)(CI)M" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Could not grant access to isolated test directory' }
    $assets = Join-Path $root 'assets'
    $stage = Join-Path $root 'stage/dirgo'
    New-Item -ItemType Directory $assets, $stage | Out-Null
    Copy-Item $DgoBin "$stage/dgo.exe"
    Copy-Item (Join-Path (Split-Path $DgoBin) 'DirgoPredictor') "$stage/DirgoPredictor" -Recurse
    Compress-Archive -Path $stage -DestinationPath "$assets/dirgo-x86_64-pc-windows-msvc.zip"
    $hash = (Get-FileHash "$assets/dirgo-x86_64-pc-windows-msvc.zip" -Algorithm SHA256).Hash
    "$hash  dirgo-x86_64-pc-windows-msvc.zip" | Set-Content "$assets/SHA256SUMS" -Encoding ascii
    Copy-Item install/dirgo-installer.ps1 "$root/installer.ps1"
    @'
$ErrorActionPreference = 'Stop'
try {
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Test must run without administrator privileges' }
    $env:DIRGO_DOWNLOAD_BASE = ([uri](Join-Path $PSScriptRoot 'assets')).AbsoluteUri.TrimEnd('/')
    $env:DIRGO_INSTALL_DIR = Join-Path $PSScriptRoot 'destination'
    $env:DIRGO_SETUP = 'skip'
    & "$PSScriptRoot/installer.ps1"
    $version = & "$env:DIRGO_INSTALL_DIR/dgo.exe" --version
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^dgo \d+\.\d+\.\d+$') { throw 'Installed binary did not start' }
    $before = (Get-FileHash "$env:DIRGO_INSTALL_DIR/dgo.exe").Hash
    Copy-Item "$env:SystemRoot/System32/whoami.exe" "$PSScriptRoot/stage/dirgo/dgo.exe" -Force
    $archive = "$PSScriptRoot/assets/dirgo-x86_64-pc-windows-msvc.zip"
    Compress-Archive -Path "$PSScriptRoot/stage/dirgo" -DestinationPath $archive -Force
    $hash = (Get-FileHash $archive -Algorithm SHA256).Hash
    "$hash  dirgo-x86_64-pc-windows-msvc.zip" | Set-Content "$PSScriptRoot/assets/SHA256SUMS" -Encoding ascii
    $ErrorActionPreference = 'Continue'
    & "$env:SystemRoot/System32/WindowsPowerShell/v1.0/powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot/installer.ps1" 2>&1 | Out-Null
    $failedExit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($failedExit -eq 0) { throw 'Installer accepted a binary that rejects --version' }
    if ((Get-FileHash "$env:DIRGO_INSTALL_DIR/dgo.exe").Hash -ne $before) { throw 'Failed installation replaced the existing binary' }
    $version | Set-Content "$PSScriptRoot/success.txt"
} catch { Write-Error $_; exit 1 }
'@ | Set-Content "$root/probe.ps1" -Encoding utf8
    $credential = New-Object Management.Automation.PSCredential($identity, $password)
    $process = Start-Process "$env:SystemRoot/System32/WindowsPowerShell/v1.0/powershell.exe" -Credential $credential -LoadUserProfile -WorkingDirectory $root -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', "$root/probe.ps1") -RedirectStandardOutput "$root/stdout.txt" -RedirectStandardError "$root/stderr.txt" -PassThru
    if (-not $process.WaitForExit(60000)) { $process.Kill(); throw 'Standard-user installer timed out' }
    Get-Content "$root/stdout.txt", "$root/stderr.txt"
    if ($process.ExitCode -ne 0 -or -not (Test-Path "$root/success.txt")) { throw 'Standard-user installation failed' }
    Write-Output 'WINDOWS-INSTALL:standard-user:ok'
} finally {
    Remove-LocalUser -Name $name -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $root -Recurse -Force
}
