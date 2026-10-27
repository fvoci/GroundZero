//! `sqlx::migrate!`는 마이그레이션 폴더에 새 파일이 생겨도 다시 빌드하지 않으므로 폴더 변경을 알린다.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
