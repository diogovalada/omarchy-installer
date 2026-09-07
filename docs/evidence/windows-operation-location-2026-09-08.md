# Windows operation records in ProgramData

Windows helper workspaces now use the system-resolved ProgramData known folder:
`%ProgramData%\OmarchySetup\Operations\Omarchy-Setup-<operation UUID>`.
Receipts, cleanup results and retained operation records follow that workspace.
The location remains stable when the portable executable is moved or deleted,
including when it was launched from removable storage.

The helper creates the application namespace, operation collection and individual
workspaces with an Administrators owner and a protected DACL allowing only
Administrators and SYSTEM. Existing shared roots must have the expected owner
and ACL before reuse. Directory handles reject reparse points and deny delete
sharing during validation and creation. Individual operation names remain unique
and cannot reuse an existing directory. Program Files resolution used for runtime
discovery remains unchanged.

Validation on Windows x64:

- Native library tests: 29 passed, four environment-dependent tests skipped.
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
