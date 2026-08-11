use std::fs;

use uuid::Uuid;

use super::{Library, Result};
use crate::encryption::library_salt::read_salt_file;
use crate::encryption::master_key::{find_master_key, parse_mk_filename, write_mk_file};
use crate::error::{KeychainError, LibraryError};
use crate::operations::{LibraryPassword, LibraryUsername};

impl Library {
    pub async fn user_add(
        &self,
        username_new: LibraryUsername,
        password_new: LibraryPassword,
    ) -> Result<Uuid> {
        let lib_dir = self.inner.local_dirs.local_state_library_dir();
        let salt = read_salt_file(lib_dir.path())?;
        let password_uuid = Uuid::new_v4();
        write_mk_file(
            lib_dir.path(),
            &username_new.0,
            password_uuid,
            &self.inner.master_key,
            salt,
            &password_new.0,
        )?;
        Ok(password_uuid)
    }

    pub async fn user_list(&self) -> Result<Vec<LibraryUsername>> {
        let lib_dir = self.inner.local_dirs.local_state_library_dir();
        let mut seen = std::collections::HashSet::new();
        for entry in fs::read_dir(lib_dir.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some((username, _uuid)) = parse_mk_filename(&name) {
                seen.insert(username);
            }
        }
        Ok(seen.into_iter().map(LibraryUsername).collect())
    }

    async fn user_change_password(
        &self,
        username: LibraryUsername,
        password_old: LibraryPassword,
        password_new: LibraryPassword,
    ) -> Result<Uuid> {
        let lib_dir = self.inner.local_dirs.local_state_library_dir();

        // Verify old password before writing a new mk file.
        find_master_key(lib_dir.path(), &username.0, &password_old.0)
            .map_err(|_| LibraryError::Keychain(KeychainError::AuthenticationFailed))?;

        let salt = read_salt_file(lib_dir.path())?;
        let new_uuid = Uuid::new_v4();
        write_mk_file(
            lib_dir.path(),
            &username.0,
            new_uuid,
            &self.inner.master_key,
            salt,
            &password_new.0,
        )?;
        Ok(new_uuid)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::library::local_dirs::LocalDirs;
    use crate::library::{Credentials, Library, LibraryId};
    use crate::operations::{LibraryPassword, LibraryUsername};

    fn make_local_dirs(tmp: &TempDir, library_id: &LibraryId) -> LocalDirs {
        LocalDirs::new(tmp.path().to_path_buf(), library_id)
    }

    async fn init_fresh(tmp: &TempDir, username: &str, password: &str) -> (Library, LibraryId) {
        let library_id = LibraryId(Uuid::new_v4());
        let local_dirs = make_local_dirs(tmp, &library_id);
        local_dirs.ensure_state_dirs().unwrap();
        let (lib, _password_uuid) = Library::init(
            local_dirs,
            library_id,
            Credentials {
                username: LibraryUsername(username.to_string()),
                password: LibraryPassword(password.to_string()),
            },
        )
        .await
        .unwrap();
        (lib, library_id)
    }

    async fn open_with(
        tmp: &TempDir,
        library_id: LibraryId,
        username: &str,
        password: &str,
    ) -> crate::library::Result<Library> {
        let local_dirs = make_local_dirs(tmp, &library_id);
        Library::open(
            local_dirs,
            Credentials {
                username: LibraryUsername(username.to_string()),
                password: LibraryPassword(password.to_string()),
            },
        )
        .await
    }

    #[tokio::test]
    async fn add_second_user_open_succeeds_same_library_id() {
        let tmp = TempDir::new().unwrap();
        let (lib, library_id_a) = init_fresh(&tmp, "alice", "pass_a").await;
        let library_id = lib.library_id();

        lib.user_add(
            LibraryUsername("bob".to_string()),
            LibraryPassword("pass_b".to_string()),
        )
        .await
        .unwrap();

        let lib_bob = open_with(&tmp, library_id_a, "bob", "pass_b")
            .await
            .unwrap();
        assert_eq!(lib_bob.library_id(), library_id);
    }

    #[tokio::test]
    async fn user_list_correct_after_add() {
        let tmp = TempDir::new().unwrap();
        let (lib, _library_id) = init_fresh(&tmp, "alice", "pass_a").await;

        let mut users = lib.user_list().await.unwrap();
        users.sort();
        assert_eq!(users, vec![LibraryUsername("alice".to_string())]);

        lib.user_add(
            LibraryUsername("bob".to_string()),
            LibraryPassword("pass_b".to_string()),
        )
        .await
        .unwrap();
        let mut users = lib.user_list().await.unwrap();
        users.sort();
        assert_eq!(
            users,
            vec![
                LibraryUsername("alice".to_string()),
                LibraryUsername("bob".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn change_password_old_and_new_both_work_mk_accumulates() {
        let tmp = TempDir::new().unwrap();
        let (lib, library_id) = init_fresh(&tmp, "alice", "old_pass").await;

        let lib_dir =
            LocalDirs::new(tmp.path().to_path_buf(), &library_id).local_state_library_dir();
        let mk_count_before = std::fs::read_dir(lib_dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("mk_alice_"))
            .count();

        lib.user_change_password(
            LibraryUsername("alice".to_string()),
            LibraryPassword("old_pass".to_string()),
            LibraryPassword("new_pass".to_string()),
        )
        .await
        .unwrap();

        let mk_count_after = std::fs::read_dir(lib_dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("mk_alice_"))
            .count();

        assert_eq!(
            mk_count_after,
            mk_count_before + 1,
            "old mk file must be kept"
        );

        let result_old = open_with(&tmp, library_id, "alice", "old_pass").await;
        assert!(
            result_old.is_ok(),
            "old password still works, invalidation is not implemented yet"
        );

        let result_new = open_with(&tmp, library_id, "alice", "new_pass").await;
        assert!(result_new.is_ok(), "new password should work");
    }
}
