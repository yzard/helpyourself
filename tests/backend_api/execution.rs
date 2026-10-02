use helpyourself::{error::AppError, execution::CpuExecutor, temporary::TemporaryFiles};
use std::time::Duration;

#[tokio::test]
async fn canceled_blocking_work_retains_admission_and_temporary_lease_until_drained() {
    let directory = tempfile::tempdir().unwrap();
    let temporary = TemporaryFiles::new(directory.path().join("tmp"));
    temporary.recover().await.unwrap();
    let path = directory.path().join("tmp/canceled");
    let lease = temporary.reserve(path.clone()).unwrap();
    tokio::fs::write(lease.path(), b"synthetic fragment")
        .await
        .unwrap();
    let cpu = CpuExecutor::new(1).unwrap();
    let running_cpu = cpu.clone();
    let (started, observed) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let work = tokio::spawn(async move {
        running_cpu
            .run(move || {
                started.send(()).unwrap();
                wait.recv().unwrap();
                assert_eq!(std::fs::read(lease.path()).unwrap(), b"synthetic fragment");
                Ok(())
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), observed)
        .await
        .unwrap()
        .unwrap();
    work.abort();
    assert!(work.await.unwrap_err().is_cancelled());
    assert!(matches!(
        cpu.run(|| Ok(())).await,
        Err(AppError::RateLimited)
    ));
    temporary.cleanup().await.unwrap();
    assert!(path.exists(), "running blocking work still owns the file");
    let draining_cpu = cpu.clone();
    let drain = tokio::spawn(async move { draining_cpu.shutdown().await });
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(!drain.is_finished());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), drain)
        .await
        .unwrap()
        .unwrap();
    temporary.cleanup().await.unwrap();
    assert!(!path.exists());
    assert!(matches!(
        cpu.run(|| Ok(())).await,
        Err(AppError::RateLimited)
    ));
}

#[tokio::test]
async fn blocking_panic_releases_capacity_without_reporting_success() {
    let cpu = CpuExecutor::new(1).unwrap();
    assert!(matches!(
        cpu.run(|| -> Result<(), AppError> { panic!("synthetic CPU failure") })
            .await,
        Err(AppError::Internal)
    ));
    assert_eq!(cpu.run(|| Ok(42)).await.unwrap(), 42);
    cpu.shutdown().await;
}
