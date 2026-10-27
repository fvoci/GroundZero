//! 권한 판정과 같은 DB snapshot에서 정확한 이동 트랜잭션이 보이는지 구분한다.

use crate::db::AnonTx;
use crate::error::AppError;

/// 성공적으로 커밋된 이동의 top-level xid8. 롤백된 이동에는 만들지 않는다.
#[derive(Debug, Clone, Copy)]
pub struct CommittedMove(u64);

impl CommittedMove {
    pub(crate) fn new(xid: u64) -> Self {
        Self(xid)
    }
}

/// pg_snapshot의 xid8 상한과 진행 중 top-level 트랜잭션 목록.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    upper: u64,
    in_progress: Vec<u64>,
}

impl Snapshot {
    pub(super) async fn read(tx: &mut AnonTx) -> Result<Self, AppError> {
        let (upper, in_progress): (String, Vec<String>) = sqlx::query_as(
            "SELECT pg_snapshot_xmax(s)::text, ARRAY(SELECT x::text FROM pg_snapshot_xip(s) x) FROM (SELECT pg_current_snapshot() s) snapshot",
        )
        .fetch_one(tx.conn())
        .await?;
        let parse = |value: String| {
            value
                .parse::<u64>()
                .map_err(|err| AppError::Database(sqlx::Error::Decode(Box::new(err))))
        };
        Ok(Self {
            upper: parse(upper)?,
            in_progress: in_progress
                .into_iter()
                .map(parse)
                .collect::<Result<_, _>>()?,
        })
    }

    /// 성공이 확인된 top-level xid에만 적용한다. 진행 중/미래 xid는 보이지 않는다.
    pub(super) fn sees(&self, moved: CommittedMove) -> bool {
        moved.0 < self.upper && !self.in_progress.contains(&moved.0)
    }

    #[cfg(test)]
    pub(super) fn source() -> Self {
        Self {
            upper: 100,
            in_progress: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn destination() -> Self {
        Self {
            upper: 101,
            in_progress: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_moves_distinguish_in_progress_future_and_previous_xid_epochs() {
        let epoch = 1_u64 << 32;
        let snapshot = Snapshot {
            upper: epoch + 20,
            in_progress: vec![epoch + 10, epoch + 14],
        };
        for (xid, visible) in [
            (epoch - 1, true),
            (epoch + 9, true),
            (epoch + 10, false),
            (epoch + 13, true),
            (epoch + 14, false),
            (epoch + 20, false),
            (epoch + 21, false),
        ] {
            assert_eq!(
                snapshot.sees(CommittedMove::new(xid)),
                visible,
                "top-level xid8={xid}"
            );
        }
    }
}
