//! 파일 첨부·업로드·버전 도메인. 공동 편집 저장은 조립 crate가 주입한다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod cleanup;
pub mod commands;
pub mod completion;
pub mod content;
pub mod model;
pub mod read;
pub mod resource;
pub mod router;
pub mod state;
pub mod uploads;
pub mod validation;

pub use router::router;
pub use state::{FileLimits, FilesState, PersistWorkspace};

use utoipa::OpenApi;
/// 파일 도메인의 API 계약.
#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        uploads::create_upload,
        uploads::get_upload,
        uploads::put_part,
        completion::complete_upload,
        uploads::abort_upload,
        read::document_files,
        read::item_files,
        read::get_file,
        read::versions,
        commands::rename_file,
        commands::trash_file,
        content::content,
        commands::delete_version
    ),
    components(schemas(
        model::UploadTarget,
        model::CreateUpload,
        model::UploadView,
        model::VersionView,
        model::FileView,
        model::RenameFile,
        model::FileTrashed
    ))
)]
pub struct FilesApi;
