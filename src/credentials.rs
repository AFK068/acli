use anyhow::Context;
use uuid::Uuid;

pub(crate) fn save(id: Uuid, key: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!key.trim().is_empty(), "API key must not be empty");
    entry(id)?
        .set_password(key)
        .context("Could not save API key in the system credential store")
}

pub(crate) fn load(id: Uuid) -> anyhow::Result<String> {
    entry(id)?.get_password().map_err(|error| match error {
        keyring::Error::NoEntry => anyhow::anyhow!("No API key saved for profile '{id}'"),
        // These errors contain secret bytes: do not include them in diagnostics.
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(_, _) => {
            anyhow::anyhow!("Stored API key for profile '{id}' is not valid text")
        }
        error => anyhow::Error::new(error)
            .context("Could not read API key from the system credential store"),
    })
}

pub(crate) fn delete(id: Uuid) -> anyhow::Result<()> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => {
            Err(error).context("Could not delete API key from the system credential store")
        }
    }
}

#[cfg(not(test))]
fn entry(id: Uuid) -> anyhow::Result<keyring::Entry> {
    keyring::Entry::new("acli", &id.to_string()).context(
        "System credential store is unavailable. Unlock it or configure api_key_env instead",
    )
}

#[cfg(test)]
pub(crate) fn entry(id: Uuid) -> anyhow::Result<keyring::Entry> {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    Ok(keyring::Entry {
        inner: keyring_core::Entry::new("acli", &id.to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_keys_by_profile_id() -> anyhow::Result<()> {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        save(first, "first-test-key")?;
        save(second, "second-test-key")?;
        assert_eq!(load(first)?, "first-test-key");
        assert_eq!(load(second)?, "second-test-key");
        delete(first)?;
        assert!(load(first).is_err());
        assert_eq!(load(second)?, "second-test-key");
        delete(second)?;
        delete(second)?;
        Ok(())
    }

    #[test]
    fn rejects_blank_keys() {
        for key in ["", " \t\n"] {
            assert!(save(Uuid::new_v4(), key).is_err());
        }
    }

    #[test]
    fn does_not_expose_invalid_secret_bytes() -> anyhow::Result<()> {
        let id = Uuid::new_v4();
        let entry = entry(id)?;
        let mock = entry
            .inner
            .as_any()
            .downcast_ref::<keyring_core::mock::Cred>()
            .unwrap();
        mock.set_error(keyring::Error::BadEncoding(b"test-secret".to_vec()));
        let error = load(id).unwrap_err();
        assert!(!format!("{error:?}").contains("test-secret"));
        assert!(error.to_string().contains("not valid text"));
        Ok(())
    }
}
