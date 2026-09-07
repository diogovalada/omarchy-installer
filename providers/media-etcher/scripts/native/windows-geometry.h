/* Omarchy Windows geometry compatibility extension for @ronomon/direct-io 3.0.1.
 * Queried on the retained descriptor. Zero physical size means unavailable,
 * never an assumed 512-byte physical sector. The caller must reconcile it with
 * freshly reidentified MSFT_Disk geometry before enabling writes.
 */
static void omarchy_geometry_error(struct task_data* task, const char* query, DWORD error) {
  snprintf(task->geometry_error, sizeof(task->geometry_error), "%s failed (Win32 error %lu)", query, (unsigned long) error);
  task->error = task->geometry_error;
}

static void omarchy_windows_geometry(struct task_data* task) {
  HANDLE handle = uv_get_osfhandle(task->fd);
  if (handle == INVALID_HANDLE_VALUE) {
    task->error = "EBADF: bad file descriptor";
    return;
  }
  STORAGE_PROPERTY_QUERY query;
  STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR alignment;
  DISK_GEOMETRY_EX geometry;
  DWORD bytes = 0;
  ZeroMemory(&query, sizeof(query));
  ZeroMemory(&alignment, sizeof(alignment));
  ZeroMemory(&geometry, sizeof(geometry));
  query.QueryType = PropertyStandardQuery;
  query.PropertyId = StorageAccessAlignmentProperty;
  if (DeviceIoControl(handle, IOCTL_STORAGE_QUERY_PROPERTY, &query, sizeof(query),
      &alignment, sizeof(alignment), &bytes, NULL)) {
    if (bytes < sizeof(alignment) || alignment.Size < sizeof(alignment) ||
        alignment.BytesPerLogicalSector == 0 || alignment.BytesPerPhysicalSector == 0 ||
        alignment.BytesOffsetForSectorAlignment != 0) {
      task->error = "Storage access alignment descriptor is incomplete or unsupported";
      return;
    }
    task->device_sector_logical = alignment.BytesPerLogicalSector;
    task->device_sector_physical = alignment.BytesPerPhysicalSector;
  } else {
    DWORD error = GetLastError();
    // Only explicit lack of support qualifies. Permission, device removal,
    // malformed request, and I/O errors must remain failures.
    if (error != ERROR_INVALID_FUNCTION && error != ERROR_NOT_SUPPORTED) {
      omarchy_geometry_error(task, "IOCTL_STORAGE_QUERY_PROPERTY", error);
      return;
    }
    task->alignment_query_error = error;
  }
  bytes = 0;
  if (!DeviceIoControl(handle, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, NULL, 0,
      &geometry, sizeof(geometry), &bytes, NULL)) {
    omarchy_geometry_error(task, "IOCTL_DISK_GET_DRIVE_GEOMETRY_EX", GetLastError());
    return;
  }
  if (bytes < (DWORD) FIELD_OFFSET(DISK_GEOMETRY_EX, Data) || geometry.DiskSize.QuadPart <= 0 ||
      geometry.Geometry.BytesPerSector == 0 ||
      (task->device_sector_logical && task->device_sector_logical != geometry.Geometry.BytesPerSector)) {
    task->error = "Opened disk reports incomplete or conflicting geometry";
    return;
  }
  task->device_sector_logical = geometry.Geometry.BytesPerSector;
  task->device_size = geometry.DiskSize.QuadPart;
}
