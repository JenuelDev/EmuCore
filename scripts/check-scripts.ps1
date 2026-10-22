$ErrorActionPreference = 'Stop'
$emuErrors = [Collections.Generic.List[string]]::new()
foreach ($emuScript in (Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.ps1')) {
    $emuTokens = $null
    $emuParseErrors = $null
    [Management.Automation.Language.Parser]::ParseFile($emuScript.FullName, [ref]$emuTokens, [ref]$emuParseErrors) | Out-Null
    foreach ($emuError in $emuParseErrors) { $emuErrors.Add("$($emuScript.Name): $($emuError.Message)") }
}
if ($emuErrors.Count -gt 0) { throw ($emuErrors -join [Environment]::NewLine) }
Write-Output 'PowerShell script syntax checks passed.'
