use bytes::Bytes;
use openfiles_core::{vendor::build_backend, OpenFilesConfig, OpenFilesEngine, ProviderKind};

#[tokio::test]
async fn write_flush_list_and_read_local_backend() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = OpenFilesConfig::default();
    cfg.backend.provider = ProviderKind::LocalFs;
    cfg.backend.root = tmp.path().join("objects").to_string_lossy().to_string();
    cfg.cache.dir = tmp.path().join("cache");
    cfg.sync.export_batch_window_secs = 0;

    let backend = build_backend(&cfg.backend).unwrap();
    let engine = OpenFilesEngine::new(cfg, backend).await.unwrap();
    engine
        .write_file("/a/b/hello.txt", Bytes::from_static(b"hello"))
        .await
        .unwrap();
    let entries = engine.list_dir("/a/b").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "/a/b/hello.txt");
    assert_eq!(
        engine.read_all("/a/b/hello.txt").await.unwrap(),
        Bytes::from_static(b"hello")
    );
}

#[tokio::test]
async fn rename_is_copy_delete_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = OpenFilesConfig::default();
    cfg.backend.root = tmp.path().join("objects").to_string_lossy().to_string();
    cfg.cache.dir = tmp.path().join("cache");
    cfg.sync.export_batch_window_secs = 0;

    let backend = build_backend(&cfg.backend).unwrap();
    let engine = OpenFilesEngine::new(cfg, backend).await.unwrap();
    engine
        .write_file("/old.txt", Bytes::from_static(b"data"))
        .await
        .unwrap();
    engine.rename_path("/old.txt", "/new.txt").await.unwrap();
    assert!(engine.stat("/old.txt").await.is_err());
    assert_eq!(
        engine.read_all("/new.txt").await.unwrap(),
        Bytes::from_static(b"data")
    );
}
