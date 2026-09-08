param([Parameter(Mandatory)][string]$DgoBin)
$ErrorActionPreference = 'Stop'
$resolvedDgoBin = (Resolve-Path $DgoBin).Path
$binaryText = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($resolvedDgoBin))
$externalRuntimePattern = '(?i)\b(?:vcruntime\d+(?:_\d+)?|msvcp\d+|api-ms-win-crt-[a-z0-9-]+)\.dll\b'
$externalRuntimes = @([regex]::Matches($binaryText, $externalRuntimePattern) | ForEach-Object Value | Sort-Object -Unique)
if ($externalRuntimes.Count -gt 0) {
    throw "Executable dynamically imports MSVC/UCRT components: $($externalRuntimes -join ', ')"
}
Write-Output 'WINDOWS-RUNTIME:static-crt:ok'

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ManifestResource {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern IntPtr FindResource(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll")] public static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll")] public static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll")] public static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll")] public static extern bool FreeLibrary(IntPtr module);
}
'@
$module = [ManifestResource]::LoadLibraryEx($resolvedDgoBin, [IntPtr]::Zero, 2)
if ($module -eq [IntPtr]::Zero) { throw 'Could not open executable resources' }
try {
    $resource = [ManifestResource]::FindResource($module, [IntPtr]1, [IntPtr]24)
    if ($resource -eq [IntPtr]::Zero) { throw 'Executable has no embedded application manifest' }
    $size = [ManifestResource]::SizeofResource($module, $resource)
    $pointer = [ManifestResource]::LockResource([ManifestResource]::LoadResource($module, $resource))
    $bytes = New-Object byte[] $size
    [Runtime.InteropServices.Marshal]::Copy($pointer, $bytes, 0, $size)
    [xml]$manifest = [Text.Encoding]::UTF8.GetString($bytes).Trim([char]0)
    $level = $manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
    if (-not $level -or $level.level -ne 'asInvoker' -or $level.uiAccess -ne 'false') {
        throw 'Executable must request asInvoker with uiAccess=false'
    }
    Write-Output 'WINDOWS-MANIFEST:asInvoker:ok'
} finally {
    [void][ManifestResource]::FreeLibrary($module)
}
