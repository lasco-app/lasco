//! Wired USB storage via Android's Storage Access Framework.
//!
//! Android owns access to removable volumes. The app receives an opaque tree
//! URI from `ACTION_OPEN_DOCUMENT_TREE` and persists its read/write grant; all
//! file operations below therefore go through `DocumentsContract`, never a raw
//! filesystem path.

use std::sync::OnceLock;

use async_trait::async_trait;
use jni::objects::{GlobalRef, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

use super::{AtomicWriteMode, Result, Storage, StorageError};

const DIRECTORY_MIME: &str = "vnd.android.document/directory";
const FILE_MIME: &str = "application/octet-stream";
const DISPLAY_NAME: &str = "_display_name";
const DOCUMENT_ID: &str = "document_id";
const MIME_TYPE: &str = "mime_type";

#[derive(Debug)]
struct AndroidRuntime {
    vm: JavaVM,
    context: GlobalRef,
}

static ANDROID_RUNTIME: OnceLock<AndroidRuntime> = OnceLock::new();

/// A provider- and volume-scoped SAF tree location.
///
/// Android's external-storage provider encodes a tree document ID as
/// `<volume-id>:<relative/path>`. Keeping the parsed form lets the FFI reject
/// two remotes that would address the same folder without ever treating the
/// `content://` URI as a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbAndroidTreeIdentity {
    authority: String,
    volume_id: String,
    relative_path: Vec<String>,
}

impl UsbAndroidTreeIdentity {
    fn new(authority: String, document_id: String) -> Result<Self> {
        let (volume_id, raw_path) = document_id.split_once(':').ok_or_else(|| {
            StorageError::Unavailable(
                "USB tree URI did not contain an external-storage volume ID".to_string(),
            )
        })?;
        if authority.is_empty() || volume_id.is_empty() {
            return Err(StorageError::Unavailable(
                "USB tree URI did not identify a storage volume".to_string(),
            ));
        }
        let relative_path = raw_path
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if relative_path.iter().any(|part| part == "." || part == "..") {
            return Err(StorageError::Unavailable(
                "USB tree URI contained an invalid folder path".to_string(),
            ));
        }
        Ok(Self {
            authority,
            volume_id: volume_id.to_string(),
            relative_path,
        })
    }

    /// Whether these trees are equal or one is nested inside the other.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.authority == other.authority
            && self.volume_id.eq_ignore_ascii_case(&other.volume_id)
            && (path_is_prefix(&self.relative_path, &other.relative_path)
                || path_is_prefix(&other.relative_path, &self.relative_path))
    }
}

fn path_is_prefix(prefix: &[String], path: &[String]) -> bool {
    prefix.len() <= path.len() && prefix.iter().zip(path).all(|(left, right)| left == right)
}

/// Called once by the FFI JNI entry point with the application context.
pub fn initialize_android_runtime(vm: JavaVM, context: GlobalRef) -> Result<()> {
    ANDROID_RUNTIME
        .set(AndroidRuntime { vm, context })
        .map_err(|_| {
            StorageError::Unavailable("Android USB runtime is already initialized".to_string())
        })
}

#[derive(Debug)]
pub struct StorageUsbAndroid {
    tree_uri: String,
}

impl StorageUsbAndroid {
    pub fn new(tree_uri: impl Into<String>) -> Result<Self> {
        let tree_uri = tree_uri.into();
        if !tree_uri.starts_with("content://") {
            return Err(StorageError::Unavailable(
                "USB tree URI is not a Storage Access Framework content URI".to_string(),
            ));
        }
        Ok(Self { tree_uri })
    }

    /// Reads the provider authority and normalized tree document ID used to
    /// compare persisted Android USB remotes. This deliberately does not
    /// inspect the raw URI text: equivalent SAF trees can be serialized with
    /// different URI encodings.
    pub fn tree_identity(tree_uri: &str) -> Result<UsbAndroidTreeIdentity> {
        let storage = Self::new(tree_uri)?;
        storage.with_env(|env| {
            let tree = storage.parse_uri(env, tree_uri)?;
            let authority = env
                .call_method(&tree, "getAuthority", "()Ljava/lang/String;", &[])?
                .l()?;
            if authority.is_null() {
                return Err(jni::errors::Error::NullPtr("tree URI has no authority"));
            }
            let authority = Self::string(env, authority)?;
            let document_id = env
                .call_static_method(
                    "android/provider/DocumentsContract",
                    "getTreeDocumentId",
                    "(Landroid/net/Uri;)Ljava/lang/String;",
                    &[JValue::Object(&tree)],
                )?
                .l()?;
            if document_id.is_null() {
                return Err(jni::errors::Error::NullPtr("tree URI has no document ID"));
            }
            UsbAndroidTreeIdentity::new(authority, Self::string(env, document_id)?)
                .map_err(|_| jni::errors::Error::NullPtr("invalid USB tree identity"))
        })
    }

    fn with_env<T>(&self, f: impl FnOnce(&mut JNIEnv<'_>) -> jni::errors::Result<T>) -> Result<T> {
        let runtime = ANDROID_RUNTIME.get().ok_or_else(|| {
            StorageError::Unavailable("Android USB runtime has not been initialized".to_string())
        })?;
        let mut env = runtime
            .vm
            .attach_current_thread()
            .map_err(|e| StorageError::Unavailable(format!("Android USB operation failed: {e}")))?;
        f(&mut env)
            .map_err(|e| StorageError::Unavailable(format!("Android USB operation failed: {e}")))
    }

    fn map_not_found(error: StorageError) -> StorageError {
        match error {
            StorageError::Unavailable(message)
                if message.contains("document not found")
                    || message.contains("directory not found") =>
            {
                StorageError::NotFound
            }
            other => other,
        }
    }

    fn parse_uri<'a>(&self, env: &mut JNIEnv<'a>, raw: &str) -> jni::errors::Result<JObject<'a>> {
        let raw = env.new_string(raw)?;
        env.call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object((&*raw).into())],
        )?
        .l()
    }

    fn resolver<'a>(&self, env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
        let runtime = ANDROID_RUNTIME.get().expect("runtime checked by with_env");
        env.call_method(
            runtime.context.as_obj(),
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )?
        .l()
    }

    fn root<'a>(&self, env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
        let tree = self.parse_uri(env, &self.tree_uri)?;
        let id = env
            .call_static_method(
                "android/provider/DocumentsContract",
                "getTreeDocumentId",
                "(Landroid/net/Uri;)Ljava/lang/String;",
                &[JValue::Object(&tree)],
            )?
            .l()?;
        env.call_static_method(
            "android/provider/DocumentsContract",
            "buildDocumentUriUsingTree",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&tree), JValue::Object(&id)],
        )?
        .l()
    }

    fn string(env: &mut JNIEnv<'_>, object: JObject<'_>) -> jni::errors::Result<String> {
        Ok(env.get_string(&JString::from(object))?.into())
    }

    fn document_id<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        uri: &JObject<'a>,
    ) -> jni::errors::Result<JObject<'a>> {
        env.call_static_method(
            "android/provider/DocumentsContract",
            "getDocumentId",
            "(Landroid/net/Uri;)Ljava/lang/String;",
            &[JValue::Object(uri)],
        )?
        .l()
    }

    fn children_uri<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        parent: &JObject<'a>,
    ) -> jni::errors::Result<JObject<'a>> {
        let tree = self.parse_uri(env, &self.tree_uri)?;
        let parent_id = self.document_id(env, parent)?;
        env.call_static_method(
            "android/provider/DocumentsContract",
            "buildChildDocumentsUriUsingTree",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&tree), JValue::Object(&parent_id)],
        )?
        .l()
    }

    fn find_child<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        parent: &JObject<'a>,
        name: &str,
    ) -> jni::errors::Result<Option<JObject<'a>>> {
        let tree = self.parse_uri(env, &self.tree_uri)?;
        let children = self.children_uri(env, parent)?;
        let resolver = self.resolver(env)?;
        let null = JObject::null();
        let cursor = env
            .call_method(
                &resolver,
                "query",
                "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
                &[
                    JValue::Object(&children),
                    JValue::Object(&null),
                    JValue::Object(&null),
                    JValue::Object(&null),
                    JValue::Object(&null),
                ],
            )?
            .l()?;
        if cursor.is_null() {
            return Ok(None);
        }

        let name_key = env.new_string(DISPLAY_NAME)?;
        let id_key = env.new_string(DOCUMENT_ID)?;
        let name_col = env
            .call_method(
                &cursor,
                "getColumnIndex",
                "(Ljava/lang/String;)I",
                &[JValue::Object((&*name_key).into())],
            )?
            .i()?;
        let id_col = env
            .call_method(
                &cursor,
                "getColumnIndex",
                "(Ljava/lang/String;)I",
                &[JValue::Object((&*id_key).into())],
            )?
            .i()?;
        let mut found = None;
        while env.call_method(&cursor, "moveToNext", "()Z", &[])?.z()? {
            let candidate_object = env
                .call_method(
                    &cursor,
                    "getString",
                    "(I)Ljava/lang/String;",
                    &[JValue::Int(name_col)],
                )?
                .l()?;
            let candidate = Self::string(env, candidate_object)?;
            if candidate == name {
                let id = env
                    .call_method(
                        &cursor,
                        "getString",
                        "(I)Ljava/lang/String;",
                        &[JValue::Int(id_col)],
                    )?
                    .l()?;
                found = Some(
                    env.call_static_method(
                        "android/provider/DocumentsContract",
                        "buildDocumentUriUsingTree",
                        "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
                        &[JValue::Object(&tree), JValue::Object(&id)],
                    )?
                    .l()?,
                );
                break;
            }
        }
        env.call_method(&cursor, "close", "()V", &[])?;
        Ok(found)
    }

    fn create_child<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        parent: &JObject<'a>,
        mime: &str,
        name: &str,
    ) -> jni::errors::Result<JObject<'a>> {
        let resolver = self.resolver(env)?;
        let mime = env.new_string(mime)?;
        let name = env.new_string(name)?;
        env.call_static_method(
            "android/provider/DocumentsContract",
            "createDocument",
            "(Landroid/content/ContentResolver;Landroid/net/Uri;Ljava/lang/String;Ljava/lang/String;)Landroid/net/Uri;",
            &[
                JValue::Object(&resolver),
                JValue::Object(parent),
                JValue::Object((&*mime).into()),
                JValue::Object((&*name).into()),
            ],
        )?
        .l()
    }

    fn parent_for_key<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        key: &str,
        create_parents: bool,
    ) -> jni::errors::Result<(JObject<'a>, String)> {
        let parts =
            validated_parts(key).map_err(|_| jni::errors::Error::NullPtr("invalid storage key"))?;
        let mut current = self.root(env)?;
        for part in &parts[..parts.len().saturating_sub(1)] {
            current = match self.find_child(env, &current, part)? {
                Some(uri) => uri,
                None if create_parents => self.create_child(env, &current, DIRECTORY_MIME, part)?,
                None => return Err(jni::errors::Error::NullPtr("directory not found")),
            };
        }
        Ok((
            current,
            parts.last().expect("validated key has a part").to_string(),
        ))
    }

    fn resolve<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        key: &str,
        create_parents: bool,
    ) -> jni::errors::Result<Option<JObject<'a>>> {
        let (parent, name) = self.parent_for_key(env, key, create_parents)?;
        self.find_child(env, &parent, &name)
    }

    fn directory_for_prefix<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        prefix: &str,
    ) -> jni::errors::Result<JObject<'a>> {
        let prefix = prefix.strip_suffix('/').unwrap_or(prefix);
        if prefix.is_empty() {
            return self.root(env);
        }
        let parts = validated_parts(prefix)
            .map_err(|_| jni::errors::Error::NullPtr("invalid storage prefix"))?;
        let mut current = self.root(env)?;
        for part in parts {
            current = self
                .find_child(env, &current, part)?
                .ok_or(jni::errors::Error::NullPtr("directory not found"))?;
        }
        Ok(current)
    }

    fn write_document(
        &self,
        env: &mut JNIEnv<'_>,
        document: &JObject<'_>,
        data: &[u8],
    ) -> jni::errors::Result<()> {
        let resolver = self.resolver(env)?;
        let stream = env
            .call_method(
                &resolver,
                "openOutputStream",
                "(Landroid/net/Uri;)Ljava/io/OutputStream;",
                &[JValue::Object(document)],
            )?
            .l()?;
        let bytes = env.byte_array_from_slice(data)?;
        let write_result = env.call_method(
            &stream,
            "write",
            "([B)V",
            &[JValue::Object((&*bytes).into())],
        );
        let close_result = env.call_method(&stream, "close", "()V", &[]);
        write_result?;
        close_result?;
        Ok(())
    }

    fn delete_document(
        &self,
        env: &mut JNIEnv<'_>,
        document: &JObject<'_>,
    ) -> jni::errors::Result<()> {
        let resolver = self.resolver(env)?;
        env.call_static_method(
            "android/provider/DocumentsContract",
            "deleteDocument",
            "(Landroid/content/ContentResolver;Landroid/net/Uri;)Z",
            &[JValue::Object(&resolver), JValue::Object(document)],
        )?;
        Ok(())
    }

    fn rename_document(
        &self,
        env: &mut JNIEnv<'_>,
        document: &JObject<'_>,
        display_name: &str,
    ) -> jni::errors::Result<()> {
        let resolver = self.resolver(env)?;
        let display_name = env.new_string(display_name)?;
        let renamed = env
            .call_static_method(
                "android/provider/DocumentsContract",
                "renameDocument",
                "(Landroid/content/ContentResolver;Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
                &[
                    JValue::Object(&resolver),
                    JValue::Object(document),
                    JValue::Object((&*display_name).into()),
                ],
            )?
            .l()?;
        if renamed.is_null() {
            return Err(jni::errors::Error::NullPtr(
                "provider does not support atomic rename",
            ));
        }
        Ok(())
    }

    fn list_children<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        directory: &JObject<'a>,
        prefix: &str,
    ) -> jni::errors::Result<Vec<String>> {
        let children = self.children_uri(env, directory)?;
        let resolver = self.resolver(env)?;
        let null = JObject::null();
        let cursor = env
            .call_method(
                &resolver,
                "query",
                "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
                &[
                    JValue::Object(&children),
                    JValue::Object(&null),
                    JValue::Object(&null),
                    JValue::Object(&null),
                    JValue::Object(&null),
                ],
            )?
            .l()?;
        if cursor.is_null() {
            return Ok(Vec::new());
        }

        let name_key = env.new_string(DISPLAY_NAME)?;
        let mime_key = env.new_string(MIME_TYPE)?;
        let name_col = env
            .call_method(
                &cursor,
                "getColumnIndex",
                "(Ljava/lang/String;)I",
                &[JValue::Object((&*name_key).into())],
            )?
            .i()?;
        let mime_col = env
            .call_method(
                &cursor,
                "getColumnIndex",
                "(Ljava/lang/String;)I",
                &[JValue::Object((&*mime_key).into())],
            )?
            .i()?;
        let mut keys = Vec::new();
        while env.call_method(&cursor, "moveToNext", "()Z", &[])?.z()? {
            let mime_object = env
                .call_method(
                    &cursor,
                    "getString",
                    "(I)Ljava/lang/String;",
                    &[JValue::Int(mime_col)],
                )?
                .l()?;
            let mime = Self::string(env, mime_object)?;
            if mime == DIRECTORY_MIME {
                continue;
            }
            let name_object = env
                .call_method(
                    &cursor,
                    "getString",
                    "(I)Ljava/lang/String;",
                    &[JValue::Int(name_col)],
                )?
                .l()?;
            let name = Self::string(env, name_object)?;
            keys.push(format!("{prefix}{name}"));
        }
        env.call_method(&cursor, "close", "()V", &[])?;
        Ok(keys)
    }
}

fn validated_parts(key: &str) -> std::result::Result<Vec<&str>, String> {
    let parts: Vec<_> = key.split('/').collect();
    if key.is_empty()
        || key.starts_with('/')
        || parts
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err(format!("invalid storage key '{key}'"));
    }
    Ok(parts)
}

#[async_trait]
impl Storage for StorageUsbAndroid {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.put_atomic(key, data, AtomicWriteMode::Replace)
            .await
            .map(|_| ())
    }

    async fn put_atomic(&self, key: &str, data: &[u8], _mode: AtomicWriteMode) -> Result<bool> {
        self.with_env(|env| {
            let (parent, name) = self.parent_for_key(env, key, true)?;
            let temporary_name = format!("lasco-tmp-{}", uuid::Uuid::new_v4());
            let temporary = self.create_child(env, &parent, FILE_MIME, &temporary_name)?;

            if let Err(error) = self.write_document(env, &temporary, data) {
                let _ = self.delete_document(env, &temporary);
                return Err(error);
            }

            if let Err(error) = self.rename_document(env, &temporary, &name) {
                let _ = self.delete_document(env, &temporary);
                return Err(error);
            }
            Ok(true)
        })
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.with_env(|env| {
            let document = self
                .resolve(env, key, false)?
                .ok_or(jni::errors::Error::NullPtr("document not found"))?;
            let resolver = self.resolver(env)?;
            let stream = env
                .call_method(
                    &resolver,
                    "openInputStream",
                    "(Landroid/net/Uri;)Ljava/io/InputStream;",
                    &[JValue::Object(&document)],
                )?
                .l()?;
            let mut output = Vec::new();
            let buffer = env.new_byte_array(64 * 1024)?;
            let read_result = (|| {
                loop {
                    let count = env
                        .call_method(
                            &stream,
                            "read",
                            "([B)I",
                            &[JValue::Object((&*buffer).into())],
                        )?
                        .i()?;
                    if count < 0 {
                        break;
                    }
                    output.extend_from_slice(&env.convert_byte_array(&buffer)?[..count as usize]);
                }
                Ok(output)
            })();
            let close_result = env.call_method(&stream, "close", "()V", &[]);
            close_result?;
            read_result
        })
        .map_err(Self::map_not_found)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.with_env(|env| {
            let Some(document) = self.resolve(env, key, false)? else {
                return Ok(());
            };
            self.delete_document(env, &document)
        })
        .map_err(Self::map_not_found)
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.with_env(|env| {
            let directory = self.directory_for_prefix(env, prefix)?;
            self.list_children(env, &directory, prefix)
        })
        .map_err(Self::map_not_found)
    }

    async fn list_recursive(&self, _prefix: &str) -> Result<Vec<String>> {
        Err(StorageError::Unavailable(
            "Android USB recursive list is not implemented yet".to_string(),
        ))
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.with_env(|env| Ok(self.resolve(env, key, false)?.is_some()))
            .map_err(Self::map_not_found)
    }
}
