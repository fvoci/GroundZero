//! 문서 명령이 선언한 최소 runtime 접점을 실제 Rooms에 연결한다.

use super::Rooms;
use fvoci_db::Db;
use fvoci_documents::model::MoveReply;
use fvoci_documents::state::{
    DocumentLimits, DocumentRuntime, MoveRequest, RestoreRequest, RuntimeReply,
};

impl DocumentRuntime for Rooms {
    fn move_document<'a>(
        &'a self,
        db: &'a Db,
        limits: &'a DocumentLimits,
        request: MoveRequest,
    ) -> RuntimeReply<'a, MoveReply> {
        Box::pin(self.move_document(db, limits, request))
    }

    fn restore<'a>(
        &'a self,
        db: &'a Db,
        limits: &'a DocumentLimits,
        request: RestoreRequest,
    ) -> RuntimeReply<'a, String> {
        Box::pin(self.restore(db, limits, request))
    }
}
