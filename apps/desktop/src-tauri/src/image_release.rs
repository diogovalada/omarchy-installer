//! Desktop image metadata. Local test images never become official Releases.
use omarchy_release_client::Release;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum ImageRelease {
    Official(Release),
    #[cfg(feature = "staged-iso-testing")]
    Local(LocalImage),
}

#[cfg(feature = "staged-iso-testing")]
#[derive(Clone, Debug, Serialize)]
pub(crate) struct LocalImage {
    pub version: String,
    pub file_name: String,
    pub length: u64,
    pub sha256: String,
    pub local_image: bool,
}

impl ImageRelease {
    pub fn official(&self) -> Result<&Release, String> {
        match self {
            Self::Official(release) => Ok(release),
            #[cfg(feature = "staged-iso-testing")]
            Self::Local(_) => {
                Err("Choose the local ISO again, or check for official updates.".into())
            }
        }
    }
    pub fn file_name(&self) -> &str {
        match self {
            Self::Official(r) => r.file_name(),
            #[cfg(feature = "staged-iso-testing")]
            Self::Local(r) => &r.file_name,
        }
    }
    pub fn length(&self) -> u64 {
        match self {
            Self::Official(r) => r.length(),
            #[cfg(feature = "staged-iso-testing")]
            Self::Local(r) => r.length,
        }
    }
    pub fn sha256(&self) -> &str {
        match self {
            Self::Official(r) => r.sha256(),
            #[cfg(feature = "staged-iso-testing")]
            Self::Local(r) => &r.sha256,
        }
    }
    pub fn signature(&self) -> &[u8] {
        match self {
            Self::Official(r) => r.signature(),
            #[cfg(feature = "staged-iso-testing")]
            Self::Local(_) => &[],
        }
    }
}

impl From<Release> for ImageRelease {
    fn from(release: Release) -> Self {
        Self::Official(release)
    }
}
