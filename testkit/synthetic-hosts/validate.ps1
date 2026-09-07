[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$fixtureFiles = Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.json' |
    Where-Object Name -ne 'host-fixture.schema.json' |
    Sort-Object Name

$ids = @{}
foreach ($fixtureFile in $fixtureFiles) {
    $fixture = Get-Content -LiteralPath $fixtureFile.FullName -Raw | ConvertFrom-Json
    foreach ($required in 'fixture_version', 'id', 'description', 'policy_id', 'host', 'disk_fixture', 'expect') {
        if ($null -eq $fixture.$required) {
            throw "$($fixtureFile.Name): missing $required"
        }
    }
    if ($fixture.fixture_version -ne 1) {
        throw "$($fixtureFile.Name): fixture_version must be 1"
    }
    if ($ids.ContainsKey($fixture.id)) {
        throw "$($fixtureFile.Name): duplicate fixture id $($fixture.id)"
    }
    $ids[$fixture.id] = $true
    $diskPath = Join-Path (Join-Path $PSScriptRoot '..\synthetic-disks') $fixture.disk_fixture
    if (-not (Test-Path -LiteralPath $diskPath -PathType Leaf)) {
        throw "$($fixtureFile.Name): missing disk fixture $($fixture.disk_fixture)"
    }
}

Write-Output "Validated $($fixtureFiles.Count) synthetic host fixtures."
