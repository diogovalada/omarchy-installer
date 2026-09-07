$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
# Compile the file transfer test beside the production internal transfer method.
# Nothing invokes NativeDisk's physical-device, partition or firmware methods.
Add-Type -Path @((Join-Path $root 'providers/direct-x86/NativeDisk.cs'),(Join-Path $root 'providers/direct-x86/NativeBootEvidence.cs'),(Join-Path $PSScriptRoot 'PartitionTransferTests.cs'))
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('omarchy-transfer-test-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
try {
    $checks=[PartitionTransferTests]::Run($temporary)
    Write-Output "Passed $checks partition transfer cases with real Windows unbuffered I/O into temporary files. No disk, partition or firmware APIs invoked."
} finally {
    # The test removes each exact file. Directory.Delete without recursion will
    # fail safely if unexpected contents remain.
    [IO.Directory]::Delete($temporary)
}
