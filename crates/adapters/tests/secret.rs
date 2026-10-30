use coldstart_adapters::{MemorySecretStore, SecretStore, SecretStoreError};

#[test]
fn memory_secret_store_roundtrips_without_exposing_values() {
    let store = MemorySecretStore::default();
    store
        .set("coldstart:test:github", "sensitive-token-value")
        .expect("secret must save");

    assert_eq!(
        store
            .get("coldstart:test:github")
            .expect("secret must load"),
        "sensitive-token-value"
    );
    let debug = format!("{store:?}");
    assert!(!debug.contains("sensitive-token-value"));
    assert!(debug.contains("[REDACTED]"));

    store
        .delete("coldstart:test:github")
        .expect("secret must delete");
    assert!(matches!(
        store.get("coldstart:test:github"),
        Err(SecretStoreError::NotFound(_))
    ));
    store
        .delete("coldstart:test:github")
        .expect("deleting a missing secret must be idempotent");
}

#[test]
fn memory_secret_store_rejects_empty_references_and_values() {
    let store = MemorySecretStore::default();
    assert!(matches!(
        store.set(" ", "value"),
        Err(SecretStoreError::EmptyReference)
    ));
    assert!(matches!(
        store.set("coldstart:test:empty", ""),
        Err(SecretStoreError::EmptySecret)
    ));
}
