$ErrorActionPreference = 'Stop'
$emuRepoRoot = Split-Path -Parent $PSScriptRoot
$emuProblems = [Collections.Generic.List[string]]::new()
Push-Location -LiteralPath $emuRepoRoot
try {
    $emuFiles = @(rg --files --hidden -g '!.git/**' -g '!.tools/**' -g '!target/**' -g '!artifacts/**')
    if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate project files with rg' }
    foreach ($emuFile in $emuFiles) {
        $emuExtension = [IO.Path]::GetExtension($emuFile)
        if ($emuExtension -notin @('.md','.rs','.ps1','.toml','.S','.txt','.lock','.java','.c','.xml','.ini') -and $emuFile -notin @('.gitignore','.gitattributes')) { continue }
        $emuText = [IO.File]::ReadAllText((Join-Path $emuRepoRoot $emuFile))
        if ($emuText.Contains([string][char]0x2014)) { $emuProblems.Add("Prohibited character: $emuFile") }
        if ($emuText -match '(?m)[\t ]+\r?$') { $emuProblems.Add("Trailing whitespace: $emuFile") }
        if ($emuExtension -ne '.md') { continue }
        foreach ($emuMatch in [regex]::Matches($emuText, '\[[^\]]+\]\(([^)]+)\)')) {
            $emuTarget = $emuMatch.Groups[1].Value
            if ($emuTarget -match '^https?://' -or $emuTarget -match '^#') { continue }
            $emuTargetPath = Join-Path -Path (Split-Path -Parent (Join-Path $emuRepoRoot $emuFile)) -ChildPath ($emuTarget -split '#')[0]
            if (-not (Test-Path -LiteralPath $emuTargetPath)) { $emuProblems.Add("Broken link in ${emuFile}: $emuTarget") }
        }
    }
    if ($emuProblems.Count -gt 0) { throw ($emuProblems -join [Environment]::NewLine) }
    Write-Output "Documentation and text checks passed for $($emuFiles.Count) project files."
} finally {
    Pop-Location
}
