use saispace_core::{CoreError, Db, GrantStatus, Vault};

async fn setup() -> (tempfile::TempDir, Db, Vault) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("s.db")).await.unwrap();
    let vault = Vault::open(&dir.path().join("vault.key")).unwrap();
    (dir, db, vault)
}

#[tokio::test]
async fn secret_requires_approval_before_reveal() {
    let (_dir, db, vault) = setup().await;
    db.set_secret(&vault, "GITHUB_TOKEN", "ghp_abc", "token")
        .await
        .unwrap();

    let err = db
        .reveal_secret(&vault, "GITHUB_TOKEN", "claude")
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        CoreError::SecretAccess {
            status: "pending",
            ..
        }
    ));

    let grants = db.list_secret_grants().await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].status, GrantStatus::Pending);

    db.decide_secret_grant(grants[0].id, true).await.unwrap();
    assert_eq!(
        db.reveal_secret(&vault, "GITHUB_TOKEN", "claude")
            .await
            .unwrap(),
        "ghp_abc"
    );

    let other = db
        .reveal_secret(&vault, "GITHUB_TOKEN", "opencode")
        .await
        .unwrap_err();
    assert!(matches!(other, CoreError::SecretAccess { .. }));

    db.decide_secret_grant(grants[0].id, false).await.unwrap();
    let denied = db
        .reveal_secret(&vault, "GITHUB_TOKEN", "claude")
        .await
        .unwrap_err();
    assert!(matches!(
        denied,
        CoreError::SecretAccess {
            status: "denied",
            ..
        }
    ));
}

#[tokio::test]
async fn secret_values_are_encrypted_and_updatable() {
    let (dir, db, vault) = setup().await;
    db.set_secret(&vault, "API_KEY", "first-value", "desc")
        .await
        .unwrap();
    let meta = db
        .set_secret(&vault, "API_KEY", "second-value", "")
        .await
        .unwrap();
    assert_eq!(meta.description, "desc");
    assert_eq!(db.list_secrets().await.unwrap().len(), 1);

    let raw: Vec<u8> = sqlx::query_scalar("SELECT ciphertext FROM secrets")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(!raw.windows(12).any(|w| w == b"second-value"));

    let grant = db
        .request_secret_access("API_KEY", "claude", "deploy")
        .await
        .unwrap();
    assert_eq!(grant.reason, "deploy");
    db.decide_secret_grant(grant.id, true).await.unwrap();
    let reopened = Vault::open(&dir.path().join("vault.key")).unwrap();
    assert_eq!(
        db.reveal_secret(&reopened, "API_KEY", "claude")
            .await
            .unwrap(),
        "second-value"
    );

    let foreign = Vault::open(&dir.path().join("other.key")).unwrap();
    assert!(
        db.reveal_secret(&foreign, "API_KEY", "claude")
            .await
            .is_err()
    );

    assert!(db.set_secret(&vault, "bad name", "x", "").await.is_err());
    assert!(db.set_secret(&vault, "EMPTY", "", "").await.is_err());

    db.delete_secret("API_KEY").await.unwrap();
    assert_eq!(db.list_secret_grants().await.unwrap().len(), 0);
}
