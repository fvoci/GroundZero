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

/// 감사 15번: 로컬 저장소가 마치기 도중 실패하면(디스크 부족, 사라진 조각 등) 합치던 임시 파일을 남기지 않는다.
/// 남기면 파일 크기만큼의 찌꺼기가 쌓여 디스크 부족을 더 키운다.
#[tokio::test]
async fn local_complete_failure_leaves_no_temp_file() {
    let dir = std::env::temp_dir().join(format!("fvoci-storage-{}", uuid::Uuid::now_v7().simple()));
    let storage = Storage::new(&StorageConfig::Local { dir: dir.clone() });
    let key = format!("test/{}", uuid::Uuid::now_v7().simple());
    let upload = storage.begin(&key).await.unwrap();
    let etag = storage
        .put_part(&key, &upload, 1, Bytes::from(vec![1u8; 1024]))
        .await
        .unwrap();
    let parts = [
        Part {
            number: 1,
            etag: etag.clone(),
            size: 1024,
        },
        Part {
            number: 2,
            etag,
            size: 1024,
        },
    ];
    assert!(storage.complete(&key, &upload, &parts).await.is_err());
    let left: Vec<_> = std::fs::read_dir(dir.join("tmp"))
        .map(|d| d.filter_map(Result::ok).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "temp files left: {left:?}");
    let left: Vec<_> = std::fs::read_dir(dir.join("uploads").join(&upload))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("staged-"))
        .collect();
    assert!(left.is_empty(), "staged files left: {left:?}");
    let _ignored = std::fs::remove_dir_all(&dir);
}

/// #65: 로컬 저장소는 마치기 시도마다 따로 준비본에 조각을 잇고, 게시하기 전에는 최종 키를 건드리지 않는다. 다른 시도가
/// 게시하면(조각 폴더가 지워지면) 늦게 이은 준비본은 더 게시되지 않는다.
#[tokio::test]
async fn local_staging_leaves_the_object_alone_until_published() {
    let dir = std::env::temp_dir().join(format!("fvoci-storage-{}", uuid::Uuid::now_v7().simple()));
    let storage = Storage::new(&StorageConfig::Local { dir: dir.clone() });
    let key = format!("test/{}", uuid::Uuid::now_v7().simple());
    let upload = storage.begin(&key).await.unwrap();
    let put = |body: &'static [u8]| {
        let (storage, key, upload) = (storage.clone(), key.clone(), upload.clone());
        async move {
            let etag = storage
                .put_part(&key, &upload, 1, Bytes::from_static(body))
                .await
                .unwrap();
            [Part {
                number: 1,
                etag,
                size: body.len() as u64,
            }]
        }
    };
    let late = storage
        .assemble(&key, &upload, &put(b"old bytes").await)
        .await
        .unwrap();
    // 다른 시도가 조각을 다시 받아 잇는다. 준비본은 서로 섞이지 않는다.
    let current = storage
        .assemble(&key, &upload, &put(b"new bytes").await)
        .await
        .unwrap();
    let sha = |bytes: &[u8]| hex::encode(Sha256::digest(bytes));
    assert_eq!(
        storage.digest_assembled(&key, &late).await.unwrap(),
        (sha(b"old bytes"), 9)
    );
    assert_eq!(
        storage.digest_assembled(&key, &current).await.unwrap(),
        (sha(b"new bytes"), 9)
    );
    assert!(matches!(storage.get(&key).await, Err(AppError::NotFound)));

    storage.publish(&key, &upload, &current).await.unwrap();
    current.discard().await;
    assert_eq!(read_all(&storage, &key).await, b"new bytes");
    assert!(!dir.join("uploads").join(&upload).exists());
    // 늦게 이은 준비본은 조각 폴더와 함께 지워져 게시할 수 없다. 읽기는 열어 둔 파일로 계속된다.
    assert_eq!(
        storage.digest_assembled(&key, &late).await.unwrap(),
        (sha(b"old bytes"), 9)
    );
    assert!(storage.publish(&key, &upload, &late).await.is_err());
    late.discard().await;
    assert_eq!(read_all(&storage, &key).await, b"new bytes");
    assert!(matches!(
        storage.assemble(&key, &upload, &[]).await,
        Err(AppError::NotFound)
    ));
    let _ignored = std::fs::remove_dir_all(&dir);
}

/// #65 리뷰: 잇는 사이 다른 시도가 게시해 조각 폴더를 지우면, 늦은 시도는 조각을 다시 받으라는 실패가 아니라 이미
/// 게시된 것(`NotFound`)으로 본다. 첫 조각 자리에 둔 FIFO에서 읽다가 멈춘 사이 폴더를 지운다.
#[cfg(unix)]
#[tokio::test]
async fn local_staging_sees_parts_removed_midway_as_already_published() {
    let dir = std::env::temp_dir().join(format!("fvoci-storage-{}", uuid::Uuid::now_v7().simple()));
    let storage = Storage::new(&StorageConfig::Local { dir: dir.clone() });
    let key = format!("test/{}", uuid::Uuid::now_v7().simple());
    let upload = storage.begin(&key).await.unwrap();
    let mut parts = Vec::new();
    for number in 1..=2 {
        let etag = storage
            .put_part(&key, &upload, number, Bytes::from_static(b"part"))
            .await
            .unwrap();
        parts.push(Part {
            number,
            etag,
            size: 4,
        });
    }
    let parts_dir = dir.join("uploads").join(&upload);
    let first = parts_dir.join("00001");
    std::fs::remove_file(&first).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(&first)
        .status()
        .unwrap();
    assert!(made.success());
    let staging = {
        let (storage, key, upload) = (storage.clone(), key.clone(), upload.clone());
        tokio::spawn(async move { storage.assemble(&key, &upload, &parts).await })
    };
    let mut writer = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || std::fs::OpenOptions::new().write(true).open(first)),
    )
    .await
    .expect("staging never started reading the parts")
    .unwrap()
    .unwrap();
    std::fs::remove_dir_all(&parts_dir).unwrap();
    {
        use std::io::Write as _;
        writer.write_all(b"part").unwrap();
    }
    drop(writer);
    let staged = tokio::time::timeout(std::time::Duration::from_secs(30), staging)
        .await
        .expect("staging never finished")
        .unwrap();
    assert!(matches!(staged, Err(AppError::NotFound)), "{staged:?}");
    let _ignored = std::fs::remove_dir_all(&dir);
}
