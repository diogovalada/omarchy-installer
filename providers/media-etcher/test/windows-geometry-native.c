// Exercise the production C query implementation with mocked Win32 replies.
// No disk handles, disk discovery, or writes are performed by this executable.
#include <windows.h>
#include <winioctl.h>
#include <stdint.h>
#include <stdio.h>
#include <assert.h>
struct task_data {
  int fd;
  int64_t device_sector_logical, device_sector_physical, device_size;
  const char* error;
  char geometry_error[160];
  int64_t alignment_query_error;
};
static DWORD query_error, geometry_error, alignment_bytes, geometry_bytes, offset, geometry_sector;
static int geometry_calls;
static HANDLE test_handle(int fd) { assert(fd == 7); return (HANDLE)(uintptr_t)77; }
static BOOL test_ioctl(HANDLE handle, DWORD code, void* input, DWORD input_size,
    void* output, DWORD output_size, DWORD* bytes, void* overlapped) {
  assert(handle == (HANDLE)(uintptr_t)77 && overlapped == NULL);
  if (code == IOCTL_STORAGE_QUERY_PROPERTY) {
    STORAGE_PROPERTY_QUERY* query = input;
    STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR* alignment = output;
    assert(input_size == sizeof(*query) && output_size == sizeof(*alignment));
    assert(query->PropertyId == StorageAccessAlignmentProperty && query->QueryType == PropertyStandardQuery);
    if (query_error) { SetLastError(query_error); return FALSE; }
    alignment->Size = sizeof(*alignment);
    alignment->BytesPerLogicalSector = 512;
    alignment->BytesPerPhysicalSector = 4096;
    alignment->BytesOffsetForSectorAlignment = offset;
    *bytes = alignment_bytes;
  } else {
    DISK_GEOMETRY_EX* geometry = output;
    assert(code == IOCTL_DISK_GET_DRIVE_GEOMETRY_EX && input == NULL && input_size == 0);
    assert(output_size == sizeof(*geometry));
    geometry_calls++;
    if (geometry_error) { SetLastError(geometry_error); return FALSE; }
    geometry->DiskSize.QuadPart = 8015314944LL;
    geometry->Geometry.BytesPerSector = geometry_sector;
    *bytes = geometry_bytes;
  }
  return TRUE;
}
#define uv_get_osfhandle test_handle
#define DeviceIoControl test_ioctl
#include "../scripts/native/windows-geometry.h"
static struct task_data run(DWORD error) {
  struct task_data task = {0}; task.fd = 7; query_error = error; geometry_calls = 0;
  omarchy_windows_geometry(&task);
  // Return only failure presence; the diagnostic pointer lives in task itself.
  task.error = task.error ? "failed" : NULL;
  return task;
}
int main(void) {
  struct task_data task;
  DWORD errors[] = { ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED };
  DWORD failures[] = { ERROR_ACCESS_DENIED, ERROR_NOT_READY, ERROR_GEN_FAILURE, ERROR_INVALID_PARAMETER, ERROR_DEVICE_NOT_CONNECTED };
  size_t i;
  alignment_bytes = sizeof(STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR);
  geometry_bytes = sizeof(DISK_GEOMETRY_EX); geometry_sector = 512;
  task = run(0);
  assert(!task.error && task.device_sector_physical == 4096 && task.device_sector_logical == 512 && task.device_size == 8015314944LL);
  for (i = 0; i < sizeof(errors)/sizeof(errors[0]); i++) {
    task = run(errors[i]);
    assert(!task.error && task.device_sector_physical == 0 && task.alignment_query_error == errors[i]);
    assert(task.device_sector_logical == 512 && task.device_size == 8015314944LL && geometry_calls == 1);
  }
  for (i = 0; i < sizeof(failures)/sizeof(failures[0]); i++) {
    task = run(failures[i]); assert(task.error && geometry_calls == 0);
    assert(strstr(task.geometry_error, "Win32 error") != NULL);
  }
  geometry_error = ERROR_DEVICE_NOT_CONNECTED;
  task = run(ERROR_INVALID_FUNCTION); assert(task.error && strstr(task.geometry_error, "1167"));
  geometry_error = 0;
  alignment_bytes = 8; task = run(0); assert(task.error && geometry_calls == 0);
  alignment_bytes = sizeof(STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR);
  offset = 512; task = run(0); assert(task.error); offset = 0;
  geometry_bytes = 8; task = run(ERROR_INVALID_FUNCTION); assert(task.error);
  geometry_bytes = sizeof(DISK_GEOMETRY_EX);
  geometry_sector = 4096; task = run(0); assert(task.error);
  geometry_sector = 0; task = run(ERROR_INVALID_FUNCTION); assert(task.error);
  puts("14 native Windows geometry scenarios passed; no devices accessed.");
  return 0;
}
