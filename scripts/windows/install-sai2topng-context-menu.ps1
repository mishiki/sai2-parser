[CmdletBinding()]
param(
    [string]$ExePath = (Join-Path $PSScriptRoot 'sai2topng.exe'),
    [switch]$Uninstall
)

$menuKey = 'HKCU:\Software\Classes\SystemFileAssociations\.sai2\shell\sai2topng'

if ($Uninstall) {
    if (Test-Path -LiteralPath $menuKey) {
        Remove-Item -LiteralPath $menuKey -Recurse -Force
    }
    Write-Host 'Removed the Convert SAI2 to PNG context-menu command.'
    exit 0
}

$resolvedExe = (Resolve-Path -LiteralPath $ExePath -ErrorAction Stop).Path
if ([IO.Path]::GetExtension($resolvedExe) -ne '.exe') {
    throw "Expected an .exe file: $resolvedExe"
}

$commandKey = Join-Path $menuKey 'command'
New-Item -Path $commandKey -Force | Out-Null
Set-Item -LiteralPath $menuKey -Value 'Convert SAI2 to PNG'
Set-ItemProperty -LiteralPath $menuKey -Name 'Icon' -Value $resolvedExe
Set-Item -LiteralPath $commandKey -Value ('"{0}" "%1"' -f $resolvedExe)

Write-Host 'Installed the Convert SAI2 to PNG context-menu command.'
Write-Host "Executable: $resolvedExe"
Write-Host 'Windows 11 may show it under Show more options.'
