use helpyourself::temporary::TemporaryFiles;

#[tokio::test]
async fn cleanup_preserves_live_leases_and_recovery_removes_interrupted_directories() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("tmp");
    let temporary = TemporaryFiles::new(root.clone());
    temporary.recover().await.unwrap();
    let path = root.join("job/page");
    let live = temporary.reserve(path.clone()).unwrap();
    tokio::fs::create_dir_all(&path).await.unwrap();
    tokio::fs::write(path.join("page.png"), b"synthetic pixels")
        .await
        .unwrap();
    temporary.cleanup().await.unwrap();
    assert!(path.exists());
    let copy = live.clone();
    drop(live);
    temporary.cleanup().await.unwrap();
    assert!(path.exists());
    drop(copy);
    temporary.cleanup().await.unwrap();
    assert!(!path.exists());
    assert!(temporary.reserve(root.join("../outside")).is_err());
    let stale = root.join("interrupted/nested");
    tokio::fs::create_dir_all(&stale).await.unwrap();
    tokio::fs::write(stale.join("private-fragment"), b"synthetic")
        .await
        .unwrap();
    temporary.recover().await.unwrap();
    assert!(
        tokio::fs::read_dir(root)
            .await
            .unwrap()
            .next_entry()
            .await
            .unwrap()
            .is_none()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn startup_recovery_unlinks_symlinks_without_touching_their_target() {
    let directory = tempfile::tempdir().unwrap();
    let outside = directory.path().join("outside");
    tokio::fs::create_dir(&outside).await.unwrap();
    tokio::fs::write(outside.join("keep"), b"keep")
        .await
        .unwrap();
    let root = directory.path().join("tmp");
    let temporary = TemporaryFiles::new(root.clone());
    temporary.recover().await.unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    temporary.recover().await.unwrap();
    assert_eq!(
        tokio::fs::read(outside.join("keep")).await.unwrap(),
        b"keep"
    );
}
