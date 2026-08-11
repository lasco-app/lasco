use std::fmt;

use aes_gcm::{Aes256Gcm, Key as AesKey};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub use crate::encryption::library_salt::LibrarySalt;

// RFC 9106 recommends 64 MiB RAM, 128-bit salt, 3 iterations for memory-constrained settings.
pub const ARGON2_M_COST: u32 = 1 << 16; // 64 MiB
pub const ARGON2_T_COST: u32 = 3;
pub const ARGON2_P_COST: u32 = 1;

pub(crate) const KEK_SIZE: usize = size_of::<AesKey<Aes256Gcm>>();
const _: () = assert!(KEK_SIZE == 32); // AES-256 key is fixed at 256 bits by the spec

/// Key-encryption key derived from the user's password and `LibrarySalt` via Argon2id.
///
/// Decrypts `mk_{username}.enc` to recover the `MasterKey`. Never stored on disk.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Kek([u8; KEK_SIZE]);

impl Kek {
    #[must_use]
    pub fn from_raw(bytes: [u8; KEK_SIZE]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for Kek {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Kek(...)")
    }
}

impl AsRef<[u8; KEK_SIZE]> for Kek {
    fn as_ref(&self) -> &[u8; KEK_SIZE] {
        &self.0
    }
}

/// Derive a `Kek` from `password` and `salt` using Argon2id.
#[must_use]
pub fn derive_kek(password: &str, salt: LibrarySalt) -> Kek {
    let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, Some(KEK_SIZE))
        .expect("hardcoded Argon2 params are not valid");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut kek_bytes = [0; KEK_SIZE];
    argon2
        .hash_password_into(password.as_bytes(), &salt.0, &mut kek_bytes)
        .expect("Argon2id derivation failed with valid params");
    Kek::from_raw(kek_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send<T: Send>() {}
    const _: () = {
        let _ = _assert_send::<Kek>;
    };

    #[test]
    fn kek_debug_does_not_leak_key_material() {
        let kek = Kek::from_raw([42; KEK_SIZE]);
        assert_eq!(format!("{kek:?}"), "Kek(...)");
    }

    #[test]
    fn derive_kek_is_deterministic() {
        let salt = LibrarySalt([42; KEK_SIZE]);
        let kek1 = derive_kek("derive kek is deterministic", salt);
        let kek2 = derive_kek("derive kek is deterministic", salt);
        assert_eq!(kek1.as_ref(), kek2.as_ref());
    }
}
