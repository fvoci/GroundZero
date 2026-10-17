#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 파일 저장소 호환성 (D-06, D-61, O-31): 로컬과 S3 호환 저장소가 같은 여러 조각 업로드를 지원하는지.
//! S3 시험(`s3_*`)은 `FVOCI_TEST_S3_*`가 있을 때만 돈다(`cargo xtask test`가 테스트용 SeaweedFS를 띄운다).

use bytes::Bytes;
use futures_util::StreamExt;
use fvoci_server::config::{S3Config, Secret, StorageConfig};
use fvoci_server::error::AppError;
use fvoci_server::storage::{Part, Storage};
use sha2::{Digest, Sha256};

fn local() -> Storage {
    let dir = std::env::temp_dir().join(format!("fvoci-storage-{}", uuid::Uuid::now_v7().simple()));
    Storage::new(&StorageConfig::Local { dir })
}

fn s3(secret: Option<&str>) -> Storage {
    let env = |k: &str| std::env::var(k).expect("run via `cargo xtask test`");
    Storage::new(&StorageConfig::S3(S3Config {
        endpoint: env("FVOCI_TEST_S3_ENDPOINT").parse().unwrap(),
        region: "us-east-1".into(),
        bucket: env("FVOCI_TEST_S3_BUCKET"),
        access_key: env("FVOCI_TEST_S3_ACCESS_KEY"),
        secret_key: Secret::new(
            secret.map_or_else(|| env("FVOCI_TEST_S3_SECRET_KEY"), str::to_owned),
        ),
    }))
}

async fn read_all(storage: &Storage, key: &str) -> Vec<u8> {
    let mut stream = storage.get(key).await.unwrap();
    let mut out = Vec::new();
    while let Some(chunk) = stream.next().await {
        out.extend_from_slice(&chunk.unwrap());
    }
    out
}

/// 시작 → 조각 두 개(앞은 S3 최소 크기 5 MiB) → 이어 올리기 확인 → 같은 번호 다시 넣기 → 마치기 → 무결성 →
/// 지우기, 그리고 그만두기.
async fn round_trip(storage: Storage) {
    let key = format!("test/{}", uuid::Uuid::now_v7().simple());
    let first = Bytes::from(vec![7u8; 5 * 1024 * 1024]);
    let upload = storage.begin(&key).await.unwrap();
    let etag1 = storage
        .put_part(&key, &upload, 1, first.clone())
        .await
        .unwrap();
    storage
        .put_part(&key, &upload, 2, Bytes::from_static(b"wrong"))
        .await
        .unwrap();
    // 이어 올리기: 받은 조각을 보고, 잘못 보낸 조각은 다시 넣는다.
    let parts = storage.list_parts(&key, &upload).await.unwrap();
    assert_eq!(
        parts.iter().map(|p| (p.number, p.size)).collect::<Vec<_>>(),
        [(1, first.len() as u64), (2, 5)]
    );
    let etag2 = storage
        .put_part(&key, &upload, 2, Bytes::from_static(b"tail!!"))
        .await
        .unwrap();
    let parts = [
        Part {
            number: 1,
            etag: etag1,
            size: first.len() as u64,
        },
        Part {
            number: 2,
            etag: etag2,
            size: 6,
        },
    ];
    storage.complete(&key, &upload, &parts).await.unwrap();

    let mut whole = first.to_vec();
    whole.extend_from_slice(b"tail!!");
    let (digest, size) = storage.digest(&key).await.unwrap();
    assert_eq!(size, whole.len() as u64);
    assert_eq!(digest, hex::encode(Sha256::digest(&whole)));
    assert_eq!(read_all(&storage, &key).await, whole);

    storage.delete(&key).await.unwrap();
    assert!(matches!(storage.get(&key).await, Err(AppError::NotFound)));
    storage.delete(&key).await.unwrap();

    // 그만두면 받은 조각이 없어진다.
    let other = format!("test/{}", uuid::Uuid::now_v7().simple());
    let upload = storage.begin(&other).await.unwrap();
    storage
        .put_part(&other, &upload, 1, Bytes::from_static(b"x"))
        .await
        .unwrap();
    storage.abort(&other, &upload).await.unwrap();
    assert!(storage.list_parts(&other, &upload).await.is_err());
    storage.abort(&other, &upload).await.unwrap();
    assert!(storage.get("../etc/passwd").await.is_err());
}

#[tokio::test]
async fn local_storage_round_trip() {
    round_trip(local()).await;
}

#[tokio::test]
async fn s3_storage_round_trip() {
    round_trip(s3(None)).await;
}

#[tokio::test]
async fn s3_rejects_a_wrong_secret() {
    let storage = s3(Some("not-the-secret"));
    let err = storage.begin("test/denied").await.unwrap_err();
    assert!(matches!(err, AppError::Storage(_)), "{err:?}");
}
