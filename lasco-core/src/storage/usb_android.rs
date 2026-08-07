//! Wired USB storage via Android's Storage Access Framework.

use std::sync::OnceLock;

use async_trait::async_trait;
use jni::objects::{GlobalRef, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

use super::{Result, Storage, StorageError};

const DIRECTORY_MIME: &str = "vnd.android.document/directory";
const FILE_MIME: &str = "application/octet-stream";

#[derive(Debug)]
struct AndroidRuntime {
    vm: JavaVM,
    context: GlobalRef,
}

static ANDROID_RUNTIME: OnceLock<AndroidRuntime> = OnceLock::new();

/// Called once by the FFI JNI entry point with the application context.
pub fn initialize_android_runtime(vm: JavaVM, context: GlobalRef) -> Result<()> {
    ANDROID_RUNTIME
        .set(AndroidRuntime { vm, context })
        .map_err(|_| StorageError::Unavailable("Android USB runtime is already initialized".to_string()))
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

    fn with_env<T>(&self, f: impl FnOnce(&mut JNIEnv<'_>) -> jni::errors::Result<T>) -> Result<T> {
        let runtime = ANDROID_RUNTIME.get().ok_or_else(|| {
            StorageError::Unavailable("Android USB runtime has not been initialized".to_string())
        })?;
        runtime
            .vm
            .attach_current_thread(f)
            .map_err(|e| StorageError::Unavailable(format!("Android USB operation failed: {e}")))
    }

    fn parse_uri<'a>(&self, env: &mut JNIEnv<'a>, raw: &str) -> jni::errors::Result<JObject<'a>> {
        let raw = env.new_string(raw)?;
        env.call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object((&raw).into())],
        )?
        .l()
    }

    fn resolver<'a>(&self, env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
        let runtime = ANDROID_RUNTIME.get().expect("runtime checked by with_env");
        env.call_method(runtime.context.as_obj(), "getContentResolver", "()Landroid/content/ContentResolver;", &[])?
            .l()
    }

    fn root<'a>(&self, env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
        let tree = self.parse_uri(env, &self.tree_uri)?;
        let id = env.call_static_method(
            "android/provider/DocumentsContract",
            "getTreeDocumentId",
            "(Landroid/net/Uri;)Ljava/lang/String;",
            &[JValue::Object(&tree)],
        )?.l()?;
        env.call_static_method(
            "android/provider/DocumentsContract",
            "buildDocumentUriUsingTree",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&tree), JValue::Object(&id)],
        )?.l()
    }

    fn string(env: &mut JNIEnv<'_>, object: JObject<'_>) -> jni::errors::Result<String> {
        Ok(env.get_string(&JString::from(object))?.into())
    }

    fn document_id<'a>(&self, env: &mut JNIEnv<'a>, uri: &JObject<'a>) -> jni::errors::Result<JObject<'a>> {
        env.call_static_method(
            "android/provider/DocumentsContract", "getDocumentId", "(Landroid/net/Uri;)Ljava/lang/String;", &[JValue::Object(uri)],
        )?.l()
    }

    fn find_child<'a>(&self, env: &mut JNIEnv<'a>, parent: &JObject<'a>, name: &str) -> jni::errors::Result<Option<JObject<'a>>> {
        let tree = self.parse_uri(env, &self.tree_uri)?;
        let parent_id = self.document_id(env, parent)?;
        let children = env.call_static_method(
            "android/provider/DocumentsContract", "buildChildDocumentsUriUsingTree", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&tree), JValue::Object(&parent_id)],
        )?.l()?;
        let resolver = self.resolver(env)?;
        let null = JObject::null();
        let cursor = env.call_method(
            &resolver, "query", "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
            &[JValue::Object(&children), JValue::Object(&null), JValue::Object(&null), JValue::Object(&null), JValue::Object(&null)],
        )?.l()?;
        if cursor.is_null() { return Ok(None); }
        let name_key = env.new_string("_display_name")?;
        let id_key = env.new_string("document_id")?;
        let name_col = env.call_method(&cursor, "getColumnIndex", "(Ljava/lang/String;)I", &[JValue::Object((&name_key).into())])?.i()?;
        let id_col = env.call_method(&cursor, "getColumnIndex", "(Ljava/lang/String;)I", &[JValue::Object((&id_key).into())])?.i()?;
        let mut found = None;
        while env.call_method(&cursor, "moveToNext", "()Z", &[])?.z()? {
            let candidate = Self::string(env, env.call_method(&cursor, "getString", "(I)Ljava/lang/String;", &[JValue::Int(name_col)])?.l()?)?;
            if candidate == name {
                let id = env.call_method(&cursor, "getString", "(I)Ljava/lang/String;", &[JValue::Int(id_col)])?.l()?;
                found = Some(env.call_static_method(
                    "android/provider/DocumentsContract", "buildDocumentUriUsingTree", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
                    &[JValue::Object(&tree), JValue::Object(&id)],
                )?.l()?);
                break;
            }
        }
        env.call_method(&cursor, "close", "()V", &[])?;
        Ok(found)
    }

    fn create_child<'a>(&self, env: &mut JNIEnv<'a>, parent: &JObject<'a>, mime: &str, name: &str) -> jni::errors::Result<JObject<'a>> {
        let resolver = self.resolver(env)?;
        let mime = env.new_string(mime)?;
        let name = env.new_string(name)?;
        env.call_static_method(
            "android/provider/DocumentsContract", "createDocument", "(Landroid/content/ContentResolver;Landroid/net/Uri;Ljava/lang/String;Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&resolver), JValue::Object(parent), JValue::Object((&mime).into()), JValue::Object((&name).into())],
        )?.l()
    }

    fn resolve<'a>(&self, env: &mut JNIEnv<'a>, key: &str, create_parents: bool) -> jni::errors::Result<Option<JObject<'a>>> {
        let parts = validated_parts(key).map_err(|_| jni::errors::Error::NullPtr("invalid storage key"))?;
        let mut current = self.root(env)?;
        for part in &parts[..parts.len().saturating_sub(1)] {
            current = match self.find_child(env, &current, part)? {
                Some(uri) => uri,
                None if create_parents => self.create_child(env, &current, DIRECTORY_MIME, part)?,
                None => return Ok(None),
            };
        }
        let name = parts.last().expect("validated key has a part");
        self.find_child(env, &current, name)
    }
}

fn validated_parts(key: &str) -> std::result::Result<Vec<&str>, String> {
    let parts: Vec<_> = key.split('/').collect();
    if key.is_empty() || key.starts_with('/') || parts.iter().any(|part| part.is_empty() || *part == "." || *part == "..") {
        return Err(format!("invalid storage key '{key}'"));
    }
    Ok(parts)
}

#[async_trait]
impl Storage for StorageUsbAndroid {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.with_env(|env| {
            let document = match self.resolve(env, key, true)? {
                Some(uri) => uri,
                None => {
                    let parts = validated_parts(key).map_err(|_| jni::errors::Error::NullPtr("invalid storage key"))?;
                    let mut parent = self.root(env)?;
                    for part in &parts[..parts.len() - 1] { parent = self.find_child(env, &parent, part)?.expect("created parent exists"); }
                    self.create_child(env, &parent, FILE_MIME, parts.last().expect("part"))?
                }
            };
            let resolver = self.resolver(env)?;
            let stream = env.call_method(&resolver, "openOutputStream", "(Landroid/net/Uri;)Ljava/io/OutputStream;", &[JValue::Object(&document)])?.l()?;
            let bytes = env.byte_array_from_slice(data)?;
            env.call_method(&stream, "write", "([B)V", &[JValue::Object((&bytes).into())])?;
            env.call_method(&stream, "close", "()V", &[])?;
            Ok(())
        })
    }

    async fn put_atomic(&self, _key: &str, _data: &[u8]) -> Result<()> {
        Err(StorageError::Unavailable("Android USB storage does not support atomic replacement".to_string()))
    }

    async fn put_if_absent(&self, key: &str, data: &[u8]) -> Result<bool> {
        if self.exists(key).await? { return Ok(false); }
        self.put(key, data).await?;
        Ok(true)
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.with_env(|env| {
            let document = self.resolve(env, key, false)?.ok_or(jni::errors::Error::NullPtr("document not found"))?;
            let resolver = self.resolver(env)?;
            let stream = env.call_method(&resolver, "openInputStream", "(Landroid/net/Uri;)Ljava/io/InputStream;", &[JValue::Object(&document)])?.l()?;
            let mut output = Vec::new();
            let buffer = env.new_byte_array(64 * 1024)?;
            loop {
                let count = env.call_method(&stream, "read", "([B)I", &[JValue::Object((&buffer).into())])?.i()?;
                if count < 0 { break; }
                output.extend_from_slice(&env.convert_byte_array(&buffer)?[..count as usize]);
            }
            env.call_method(&stream, "close", "()V", &[])?;
            Ok(output)
        }).map_err(|error| match error { StorageError::Unavailable(message) if message.contains("document not found") => StorageError::NotFound, other => other })
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.with_env(|env| {
            let Some(document) = self.resolve(env, key, false)? else { return Ok(()); };
            let resolver = self.resolver(env)?;
            env.call_static_method("android/provider/DocumentsContract", "deleteDocument", "(Landroid/content/ContentResolver;Landroid/net/Uri;)Z", &[JValue::Object(&resolver), JValue::Object(&document)])?;
            Ok(())
        })
    }

    async fn list(&self, _prefix: &str) -> Result<Vec<String>> {
        Err(StorageError::Unavailable("Android USB list is not implemented yet".to_string()))
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.with_env(|env| Ok(self.resolve(env, key, false)?.is_some()))
    }
}
