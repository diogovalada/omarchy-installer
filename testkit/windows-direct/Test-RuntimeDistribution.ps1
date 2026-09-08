# Test archive availability and import validation without Docker or host APIs.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
function Assert($Condition,$Message) { if (-not $Condition) { throw $Message } }
function Reject($Action,$Message) { $rejected=$false; try { & $Action } catch { $rejected=$true }; Assert $rejected $Message }
# Load the real pure helpers without executing the provider entrypoint.
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $root 'providers/direct-x86/Invoke-DirectX86.ps1'),[ref]$tokens,[ref]$errors)
Assert ($errors.Count -eq 0) 'Provider must parse.'
foreach ($name in @('Fail','Assert-Fields')) {
    $definition=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$true)
    . ([scriptblock]::Create($definition.Extent.Text))
}
. (Join-Path $root 'providers/direct-x86/RuntimePreparation.ps1')
$script:builderRoot='C:\fixture\image-builder-x86'
$script:archiveSize=4
$script:archivePresent=$false
$script:descriptorPresent=$true
$script:descriptor=[pscustomobject]@{schemaVersion=1;imageId=('sha256:'+('1'*64));archive='runtime.tar';sizeBytes=4;sha256=('a'*64)}
function Test-Path { param($LiteralPath) Assert ($LiteralPath -eq (Join-Path $script:builderRoot 'runtime-distribution.json')) 'Unexpected existence check'; return $script:descriptorPresent }
function Read-Json { param($Path) return $script:descriptor }
function Get-Runtime { return @{imageId=('sha256:'+('1'*64))} }
function Assert-Path { param($Path) Assert $script:archivePresent 'Actual archive missing'; return $Path }
function Get-Item { param($LiteralPath) return @{Length=$script:archiveSize} }

$evidence=[pscustomobject]@{length=4;sha256=('a'*64)}
Assert ($null -ne (Get-RuntimeDistribution $evidence)) 'Probe must accept matching compiled metadata without reading the archive.'
Reject { Get-RuntimeDistribution ([pscustomobject]@{length=5;sha256=('a'*64)}) } 'Wrong archive size accepted'
Reject { Get-RuntimeDistribution ([pscustomobject]@{length=4;sha256=('b'*64)}) } 'Wrong archive hash accepted'
Reject { Get-RuntimeDistribution ([pscustomobject]@{length=4;sha256=('a'*64);extra=1}) } 'Unexpected metadata accepted'
Reject { Get-RuntimeDistribution } 'Import must require the actual archive'
$script:archivePresent=$true
Assert ($null -ne (Get-RuntimeDistribution)) 'Import must accept an existing archive of the expected size'
$script:archiveSize=3
Reject { Get-RuntimeDistribution } 'Truncated archive accepted'
$script:descriptorPresent=$false
Assert ($null -eq (Get-RuntimeDistribution $evidence)) 'Missing descriptor must not advertise a packaged runtime'
$script:descriptorPresent=$true
$script:archiveSize=4
$script:actualHash='b'*64
$script:loaded=$false
function Assert-Admin { }
function Emit { }
function Get-Docker { return 'Invoke-FixtureDocker' }
function Sha { param($Path) return $script:actualHash }
function Invoke-FixtureDocker {
    $global:LASTEXITCODE=0
    if ($args[0] -eq 'info') { return 'linux' }
    if ($args[1] -eq 'load') { $script:loaded=$true; return }
    if ($script:loaded) { return ('sha256:'+('1'*64)) }
    $global:LASTEXITCODE=1
}
Reject { Invoke-RuntimePreparation ([pscustomobject]@{}) } 'Import must reject archive bytes with the wrong hash'
Assert (-not $script:loaded) 'Unverified archive was imported'
$script:actualHash='a'*64
Invoke-RuntimePreparation ([pscustomobject]@{})
Assert $script:loaded 'Verified archive was not imported through the mocked Docker boundary'
Write-Output 'Passed runtime distribution checks; no Docker or host APIs invoked.'
