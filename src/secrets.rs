//! WordPress Application Password storage via the Secret Service (GNOME
//! Keyring, or the portal-based equivalent under Flatpak) through `oo7`.
//! Async: callers drive these with `glib::MainContext::spawn_local`, not a
//! separate runtime - `oo7`'s `async-std` backend's I/O reactor runs on its
//! own regardless of which executor polls the outer future, so it composes
//! fine with GLib's main loop without pulling in tokio.

use std::collections::HashMap;

const SERVICE_ATTR: &str = "blocksmith";

fn attributes<'a>(url: &'a str, username: &'a str) -> HashMap<&'static str, &'a str> {
    HashMap::from([("service", SERVICE_ATTR), ("url", url), ("username", username)])
}

pub async fn store_app_password(url: &str, username: &str, password: &str) -> oo7::Result<()> {
    let keyring = oo7::Keyring::new().await?;
    keyring.unlock().await?;
    keyring
        .create_item(
            "Blocksmith WordPress Application Password",
            &attributes(url, username),
            password,
            true, // replace any existing item for this url+username
        )
        .await
}

pub async fn load_app_password(url: &str, username: &str) -> oo7::Result<Option<String>> {
    let keyring = oo7::Keyring::new().await?;
    keyring.unlock().await?;
    let items = keyring.search_items(&attributes(url, username)).await?;
    let Some(item) = items.first() else {
        return Ok(None);
    };
    item.unlock().await?;
    let secret = item.secret().await?;
    Ok(Some(String::from_utf8_lossy(&secret).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the real Secret Service (gnome-keyring or the portal
    /// equivalent) rather than a mock, since a mismatch between "compiles"
    /// and "actually round-trips through D-Bus" is exactly the kind of bug
    /// this integration is prone to. Ignored by default since it needs a
    /// live keyring daemon; run explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn store_and_load_round_trip_against_real_keyring() {
        futures_lite::future::block_on(async {
            let url = "https://blocksmith-test.invalid";
            let username = "test-user";
            let password = "s3cr3t-app-password";

            store_app_password(url, username, password).await.expect("store failed");
            let loaded = load_app_password(url, username).await.expect("load failed");
            assert_eq!(loaded.as_deref(), Some(password));

            let keyring = oo7::Keyring::new().await.expect("keyring open failed");
            keyring
                .delete(&attributes(url, username))
                .await
                .expect("cleanup delete failed");

            let after_delete = load_app_password(url, username).await.expect("load after delete failed");
            assert_eq!(after_delete, None);
        });
    }
}
