param([string]$Binary = (Join-Path $PSScriptRoot '../target/debug/cmdnavi.exe'))
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$previousConfig = $env:XDG_CONFIG_HOME
$previousPath = $env:PATH
$temp = Join-Path ([IO.Path]::GetTempPath()) ('cmdnavi-completion-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    $env:XDG_CONFIG_HOME = $temp
    $env:PATH = (Split-Path $Binary) + [IO.Path]::PathSeparator + $env:PATH
    & $Binary add network 'ip addr' 'IP address' | Out-Null
    & $Binary add git 'git status' 'status' | Out-Null
    . (Join-Path $PSScriptRoot '../completions/cmdnavi.ps1')
    function Matches([string]$Line) {
        @((TabExpansion2 $Line $Line.Length).CompletionMatches | ForEach-Object { $_.CompletionText })
    }
    foreach ($case in @(
        @('cmdnavi a', 'add'),
        @('cmdnavi edit net', 'network'),
        @('cmdnavi --print network ip', "'ip addr'"),
        @('cmdnavi add network ip', "'ip addr'"),
        @('cmdnavi --move network 1 g', 'git')
    )) {
        $result = Matches $case[0]
        if ($result -notcontains $case[1]) { throw "Missing $($case[1]) for $($case[0]): $result" }
    }
    foreach ($line in @('cmdnavi ', 'cmdnavi edit ', 'cmdnavi --search ', 'cmdnavi add network "ip addr" ', 'cmdnavi --move network 1 ')) {
        $result = Matches $line
        if ($result -contains "'ip addr'" -or $result -contains "'git status'") { throw "Command candidate leaked into $line" }
    }
    $sentinel = Join-Path $temp 'never-execute'
    $japanese = -join @([char]0x65e5, [char]0x672c, [char]0x8a9e)
    $dangerous = "echo '$japanese'; `$(New-Item '$sentinel')"
    & $Binary add shell $dangerous '' | Out-Null
    $result = Matches 'cmdnavi --print shell echo'
    $expected = "'" + $dangerous.Replace("'", "''") + "'"
    if ($result -notcontains $expected) { throw 'Command quoting failed' }
    if (Test-Path $sentinel) { throw 'Completion evaluated stored command text' }
    & $Binary add new-category pwd | Out-Null
    if ((Matches 'cmdnavi edit new-c') -notcontains 'new-category') { throw 'Dynamic category update failed' }
    Write-Host 'PowerShell completion checks passed: context, quoting, metacharacters, dynamic categories.'
} finally {
    $env:XDG_CONFIG_HOME = $previousConfig
    $env:PATH = $previousPath
    Remove-Item -LiteralPath $temp -Recurse -Force
}
