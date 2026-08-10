use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaUuid(pub Uuid);

impl MediaUuid {
    // Not impl From<Uuid>. Prevents accidental cross-type UUID coercions.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for MediaUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AlbumUuid(pub Uuid);

impl AlbumUuid {
    // Not impl From<Uuid>.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for AlbumUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupUuid(pub Uuid);

impl GroupUuid {
    // Not impl From<Uuid>.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for GroupUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserUuid(pub Uuid);

impl UserUuid {
    // Not impl From<Uuid>.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LibraryId(pub Uuid);

impl LibraryId {
    // Not impl From<Uuid>. Prevents accidental cross-type UUID coercions.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn new() -> Self {
        LibraryId(Uuid::new_v4())
    }
}

impl Default for LibraryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for LibraryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RemoteUuid(pub Uuid);

impl RemoteUuid {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    // Not impl From<Uuid>. Prevents accidental cross-type UUID coercions.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl Default for RemoteUuid {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RemoteUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CompactedOpId(pub Uuid);

impl CompactedOpId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    // Not impl From<Uuid>. Prevents accidental cross-type UUID coercions.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl Default for CompactedOpId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CompactedOpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
