use crate::SecretString;
use anyhow::{Context, Result};
use oxideterm_secret_store::NativeSecretStore;
#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
const SERVICE_NAME: &str = "com.oxideterm.ssh";
#[allow(dead_code)]
const LEGACY_ACCOUNT_SEPARATOR: &str = "@";

#[derive(Clone, Debug)]
pub(crate) struct ConnectionKeychain {
    service: String,
    #[cfg(test)]
    test_store: Option<Arc<Mutex<HashMap<String, SecretString>>>>,
    #[cfg(test)]
    test_max_secret_bytes: Option<usize>,
}

impl Default for ConnectionKeychain {
    fn default() -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            #[cfg(test)]
            test_max_secret_bytes: None,
        }
    }
}

impl ConnectionKeychain {
    pub(crate) fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            #[cfg(test)]
            test_max_secret_bytes: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_max_secret_bytes_for_tests(
        service: impl Into<String>,
        max_secret_bytes: usize,
    ) -> Self {
        Self {
            service: service.into(),
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            test_max_secret_bytes: Some(max_secret_bytes),
        }
    }

    pub(crate) fn store(&self, id: &str, secret: &SecretString) -> Result<()> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            if self
                .test_max_secret_bytes
                .is_some_and(|limit| secret.expose_secret().len() > limit)
            {
                // Tests use this to emulate OS credential backends that reject
                // large managed SSH keys, such as RSA private-key blobs.
                anyhow::bail!("test keychain secret exceeds configured byte limit");
            }
            store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .insert(id.to_string(), secret.clone());
            return Ok(());
        }

        NativeSecretStore::new(&self.service)
            .store(&self.native_account(id), secret.expose_secret())
            .with_context(|| format!("failed to store password in OS keychain for {id}"))
    }

    pub(crate) fn get(&self, id: &str) -> Result<SecretString> {
        self.get_optional(id)?
            .ok_or_else(|| anyhow::anyhow!("Password not saved for this connection"))
    }

    pub(crate) fn get_optional(&self, id: &str) -> Result<Option<SecretString>> {
        self.get_optional_unguarded(id)
    }

    fn get_optional_unguarded(&self, id: &str) -> Result<Option<SecretString>> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            return Ok(store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .get(id)
                .cloned());
        }

        NativeSecretStore::new(&self.service)
            .get_and_relax(&self.native_account(id))
            // Move the keychain result directly into its zeroizing domain owner
            // so no unmanaged String copy survives this boundary.
            .map(|secret| secret.map(SecretString::from))
            .with_context(|| format!("failed to load password from OS keychain for {id}"))
    }

    pub(crate) fn delete(&self, id: &str) -> Result<()> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .remove(id);
            return Ok(());
        }

        NativeSecretStore::new(&self.service)
            .delete(&self.native_account(id))
            .with_context(|| format!("failed to delete password from OS keychain for {id}"))
    }

    fn native_account(&self, id: &str) -> String {
        format!("{}@{}", whoami::username(), id)
    }
}
