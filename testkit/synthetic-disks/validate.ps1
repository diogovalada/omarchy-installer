[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$fixtureFiles = Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.json' |
    Where-Object Name -ne 'disk-fixture.schema.json' |
    Sort-Object Name

$fixtureIds = @{}
foreach ($fixtureFile in $fixtureFiles) {
    $fixture = Get-Content -LiteralPath $fixtureFile.FullName -Raw | ConvertFrom-Json
    foreach ($required in 'fixture_version', 'id', 'description', 'policy_id', 'required_image_bytes', 'devices', 'expect') {
        if ($null -eq $fixture.$required) {
            throw "$($fixtureFile.Name): missing $required"
        }
    }
    if ($fixture.fixture_version -ne 1) {
        throw "$($fixtureFile.Name): fixture_version must be 1"
    }
    if ($fixtureIds.ContainsKey($fixture.id)) {
        throw "$($fixtureFile.Name): duplicate fixture id $($fixture.id)"
    }
    $fixtureIds[$fixture.id] = $true

    $candidateIds = @{}
    foreach ($device in $fixture.devices) {
        if ($candidateIds.ContainsKey($device.candidate_id)) {
            throw "$($fixtureFile.Name): duplicate candidate_id $($device.candidate_id)"
        }
        $candidateIds[$device.candidate_id] = $true
        if (($device.size_bytes % $device.logical_sector_bytes) -ne 0) {
            throw "$($fixtureFile.Name): $($device.candidate_id) size is not sector aligned"
        }
        foreach ($partition in $device.partitions) {
            if ($partition.end_lba -lt $partition.start_lba) {
                throw "$($fixtureFile.Name): $($device.candidate_id) partition $($partition.number) has reversed LBAs"
            }
            if ($partition.end_lba -ge ($device.size_bytes / $device.logical_sector_bytes)) {
                throw "$($fixtureFile.Name): $($device.candidate_id) partition $($partition.number) exceeds the device"
            }
        }
    }
    foreach ($candidateId in $fixture.expect.eligible_candidate_ids) {
        if (-not $candidateIds.ContainsKey($candidateId)) {
            throw "$($fixtureFile.Name): expectation references unknown candidate $candidateId"
        }
    }
    foreach ($refusal in $fixture.expect.refused) {
        if (-not $candidateIds.ContainsKey($refusal.candidate_id)) {
            throw "$($fixtureFile.Name): refusal references unknown candidate $($refusal.candidate_id)"
        }
    }
}

Write-Output "Validated $($fixtureFiles.Count) synthetic disk fixtures."
