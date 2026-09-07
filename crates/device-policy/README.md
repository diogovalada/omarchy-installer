# device-policy

Pure, fail-closed classification and revalidation for destructive media
targets. This crate deliberately does not enumerate devices or perform I/O.
Platform adapters must translate an OS snapshot into the types in this crate,
then require a successful `revalidate_usb_target` immediately before handing a
typed operation to a raw-disk helper.

USB targets are accepted only when all of the following are proven:

- the locator and stable identity are unique in the snapshot;
- the target is a whole, removable, writable device;
- its topology is known and does not overlap the system or source disk;
- it is large enough for the selected image; and
- its safety-relevant observation and host storage context have not changed
  between selection and execution.

Unknown and contradictory observations are denials, never defaults.
