Register-ArgumentCompleter -Native -CommandName cmdnavi, cmdnavi.exe -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $elements = @($commandAst.CommandElements | Select-Object -Skip 1 | Where-Object { $_.Extent.StartOffset -lt $cursorPosition })
    $words = @()
    foreach ($element in $elements) {
        if ($element -is [System.Management.Automation.Language.StringConstantExpressionAst]) {
            $words += $element.Value
        } else {
            $words += $element.Extent.Text
        }
    }
    $insideWord = $elements.Count -gt 0 -and $elements[-1].Extent.EndOffset -ge $cursorPosition
    if ($insideWord) {
        $position = $words.Count - 1
        $words[$position] = $wordToComplete.TrimStart([char[]]@( "'", '"'))
    } else {
        $position = $words.Count
        $words += ''
    }
    # A sentinel preserves an empty final argument in Windows PowerShell 5.1.
    $words[$position] = '__CMDNAVI_PREFIX__' + $words[$position]
    $previousEncoding = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
        $json = & cmdnavi --complete powershell $position -- @words 2>$null
        $completionExitCode = $LASTEXITCODE
    } finally {
        [Console]::OutputEncoding = $previousEncoding
    }
    if ($completionExitCode -eq 0 -and $json) {
        foreach ($item in (ConvertFrom-Json -InputObject ($json -join "`n"))) {
            $tip = if ($item.description) { $item.description } else { $item.value }
            [System.Management.Automation.CompletionResult]::new($item.insert, $item.value, 'ParameterValue', $tip)
        }
    }
}
