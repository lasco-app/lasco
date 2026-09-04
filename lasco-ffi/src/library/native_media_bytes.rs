/// Rust-owned plaintext bytes exposed as a borrowed native-memory view.
///
/// Clients may create their platform-native byte view from `data_pointer` and
/// `len`, but that view is valid only while this opaque object is retained.
/// Destroying the UniFFI object drops the backing `Vec` in Rust. This avoids
/// making a second full-size allocation to serialize the bytes into a UniFFI
/// `RustBuffer` and then a platform byte array.
#[derive(uniffi::Object, Debug)]
pub struct FfiNativeMediaBytes {
    bytes: Vec<u8>,
}

impl FfiNativeMediaBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

#[uniffi::export]
impl FfiNativeMediaBytes {
    /// Address of the first byte. It is an opaque native address, not an
    /// ownership handle; clients must not free it directly.
    pub fn data_pointer(&self) -> u64 {
        u64::try_from(self.bytes.as_ptr() as usize)
            .expect("pointers fit in u64 on supported UniFFI targets")
    }

    /// Number of bytes addressable from `data_pointer`.
    pub fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).expect("usize fits in u64 on supported UniFFI targets")
    }
}
