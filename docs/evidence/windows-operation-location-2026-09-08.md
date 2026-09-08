# Portable receipts and protected Windows workspaces

Windows helper workspaces now use the system-resolved ProgramData known folder:
`%ProgramData%\OmarchySetup\Operations\Omarchy-Setup-<operation UUID>`.
Completed receipts and cleanup records are exported by the unelevated desktop to
`Omarchy-Setup-Records\operation-<operation UUID>.json` beside the portable EXE.
The launcher supplies its own path so export does not use the extracted cache.
An unwritable output folder or one on the selected USB prompts for another folder
before elevation. Both the desktop and helper check the records filesystem against
the USB identity; direct-install plans include it among protected paths.

Export writes, flushes and reads back a temporary file before publishing without
overwriting an existing record. The desktop acknowledges the bound export plan
only after saving it. A completed USB workspace is removed only after that
acknowledgement and successful temporary-file cleanup. Cleanup preflights the
whole workspace, rejects links and unexpected files, removes a fixed record
allowlist plus the private temporary/profile contents, then removes empty folders.
Successful inspection/preparation workspaces can also be removed. Failed work,
failed exports and direct-install recovery workspaces remain in ProgramData.

The helper creates the application namespace, operation collection and individual
workspaces with an Administrators owner and a protected DACL allowing only
Administrators and SYSTEM. Existing shared roots must have the expected owner
and ACL before reuse. Directory handles reject reparse points and deny delete
sharing during validation and creation. Individual operation names remain unique
and cannot reuse an existing directory. Program Files resolution used for runtime
discovery remains unchanged.

Validation on Windows x64:

- Native library tests: 32 passed, four environment-dependent tests skipped.
- UI tests: 22 passed. Svelte check: no errors, four existing warnings.
- Export tests verify readable saved contents, refusal to replace existing
  receipts and removal of abandoned temporary exports. Cleanup tests verify that
  unknown files preserve all records and that known profile caches and empty
  nested directories can be removed.
- Explicit administrator integration test: passed. Created two successive empty
  operation folders in the new root, rejected duplicate creation, validated
  nested provider folder permissions and removed both test folders.
- Actual root SDDL: `O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`.
- Permission rejection covers an unexpected owner, inherited ACL, additional
  user access, empty ACL, null ACL and an ordinary temporary directory.
- The seven user-requested legacy Program Files operation folders were inspected
  for links, active process references and recovery requirements, then deleted.
  No matching legacy folders remained. Their 22 files totalled 1,126,535 bytes.

No USB or host storage operation was performed for this change. Build and local
cleanup details are retained in the ignored `artifacts/receipt-location` folder.
