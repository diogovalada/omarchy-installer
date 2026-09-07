use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};
use std::path::PathBuf;

pub const MAX_FRAME: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootDefault {
    Omarchy,
    Windows,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootMenu {
    pub default_os: BootDefault,
    pub timeout_seconds: u32,
}
impl Default for BootMenu {
    fn default() -> Self {
        Self {
            default_os: BootDefault::Omarchy,
            timeout_seconds: 5,
        }
    }
}
impl BootMenu {
    pub fn validate(&self) -> Result<(), String> {
        if ![5, 10, 15, 30].contains(&self.timeout_seconds) {
            return Err("Choose a boot menu countdown of 5, 10, 15 or 30 seconds".into());
        }
        Ok(())
    }
    pub fn summary(&self) -> String {
        let name = match self.default_os {
            BootDefault::Omarchy => "Omarchy",
            BootDefault::Windows => "Windows",
        };
        format!("Show the Omarchy / Windows boot menu at normal startup. Start {name} automatically after {} seconds. Choosing Windows uses its existing firmware entry and briefly restarts the PC. A one-time choice does not change the saved default.", self.timeout_seconds)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceImage {
    pub path: PathBuf,
    pub file_name: String,
    pub length: u64,
    pub sha256: String,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "target_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DirectTarget {
    Free {
        start_offset_bytes: u64,
    },
    Shrink {
        partition_number: u32,
        partition_guid: String,
    },
    Delete {
        partition_number: u32,
        partition_guid: String,
        start_offset_bytes: u64,
        partition_size_bytes: u64,
        confirmation: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Destination {
    InspectDirect,
    PrepareFirmware,
    PrepareRuntime,
    Usb {
        identity: Value,
    },
    InspectUsb {
        identity: Value,
    },
    UsbPreserve {
        identity: Value,
        plan: crate::usb_preserve::Plan,
        restore: bool,
    },
    DirectX86 {
        disk_number: u32,
        disk_unique_id: String,
        target: DirectTarget,
        allocation_bytes: u64,
        boot_menu: BootMenu,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRequest {
    pub protocol: u32,
    pub source: SourceImage,
    pub destination: Destination,
}

pub fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().map_err(|e| e.to_string())?;
        if buffer.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err("Incomplete provider frame".into())
            };
        }
        let end = buffer.iter().position(|b| *b == b'\n');
        let take = end.map_or(buffer.len(), |end| end + 1);
        if bytes.len() + take > MAX_FRAME {
            return Err("Provider frame exceeds the size limit".into());
        }
        bytes.extend_from_slice(&buffer[..take]);
        reader.consume(take);
        if end.is_some() {
            if bytes.iter().all(u8::is_ascii_whitespace) {
                bytes.clear();
                continue;
            }
            return serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| format!("Invalid provider message: {e}"));
        }
    }
}

pub fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() >= MAX_FRAME {
        return Err("Provider frame exceeds the size limit".into());
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}
