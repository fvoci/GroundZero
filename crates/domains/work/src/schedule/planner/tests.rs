use super::*;
use time::format_description::well_known::Iso8601;

fn d(raw: &str) -> Date {
    Date::parse(raw, &Iso8601::DATE).unwrap()
}

fn span(start: Option<Date>, due: Option<Date>) -> Span {
    Span { start, due }
}

fn days(start: &str, due: &str) -> Span {
    span(Some(d(start)), Some(d(due)))
}

fn ids(n: u128) -> Vec<Uuid> {
    (1..=n).map(Uuid::from_u128).collect()
}

fn fs(p: Uuid, s: Uuid) -> Link {
    link(p, s, LinkKind::FinishToStart, 0)
}

fn link(predecessor: Uuid, successor: Uuid, kind: LinkKind, lag_days: i32) -> Link {
    Link {
        predecessor,
        successor,
        kind,
        lag_days,
    }
}

/// 날짜 옮기기 계획에서 (업무, 새 날짜)만.
fn moves(
    spans: &HashMap<Uuid, Span>,
    links: &[Link],
    fixed: &HashSet<Uuid>,
    task: Uuid,
    next: Span,
) -> Vec<(Uuid, Span)> {
    plan_shifts(spans, links, fixed, Change::Move { task, next })
        .shifts
        .into_iter()
        .map(|p| (p.task, p.span))
        .collect()
}

#[test]
fn pushes_successors_by_the_minimum_and_keeps_their_length() {
    let t = ids(4);
    let spans = HashMap::from([
        (t[0], days("2026-10-01", "2026-10-05")),
        (t[1], days("2026-10-06", "2026-10-08")),
        (t[2], days("2026-10-20", "2026-10-21")),
        (t[3], span(None, Some(d("2026-10-09")))),
    ]);
    // 0 → 1 → 3, 0 → 2 (2는 충분히 뒤라 그대로).
    let links = [fs(t[0], t[1]), fs(t[1], t[3]), fs(t[0], t[2])];
    assert_eq!(
        moves(
            &spans,
            &links,
            &HashSet::new(),
            t[0],
            days("2026-10-01", "2026-10-10")
        ),
        vec![
            (t[1], days("2026-10-11", "2026-10-13")),
            (t[3], span(None, Some(d("2026-10-14")))),
        ]
    );
}

#[test]
fn pulling_earlier_or_undated_tasks_proposes_nothing() {
    let t = ids(3);
    let spans = HashMap::from([
        (t[0], span(None, Some(d("2026-10-05")))),
        (t[1], span(None, None)),
        (t[2], span(Some(d("2026-10-06")), None)),
    ]);
    let links = [fs(t[0], t[1]), fs(t[1], t[2])];
    let earlier = plan_shifts(
        &spans,
        &links,
        &HashSet::new(),
        Change::Move {
            task: t[0],
            next: span(None, Some(d("2026-10-01"))),
        },
    );
    assert_eq!(earlier, Plan::default());
    // 날짜 없는 업무는 옮기지 않고 제약을 전하지 않는다. 더 늦은 날을 요구받으면 알린다(날짜를 지어내지 않는다).
    let later = plan_shifts(
        &spans,
        &links,
        &HashSet::new(),
        Change::Move {
            task: t[0],
            next: span(None, Some(d("2026-12-01"))),
        },
    );
    assert_eq!(
        later,
        Plan {
            shifts: vec![],
            undated: vec![t[1]]
        }
    );
}

#[test]
fn existing_overlaps_and_done_tasks_are_left_alone() {
    let t = ids(4);
    let spans = HashMap::from([
        (t[0], span(None, Some(d("2026-10-01")))),
        // 1은 0의 후속인데 전부터 0과 겹친다(0의 마감 10-01, 1의 시작 10-01).
        (t[1], days("2026-10-01", "2026-10-02")),
        (t[2], days("2026-10-03", "2026-10-03")),
        (t[3], days("2026-10-02", "2026-10-02")),
    ]);
    let links = [fs(t[0], t[1]), fs(t[1], t[2]), fs(t[0], t[3])];
    // 0을 앞당기면 아무것도 제안하지 않는다(전부터 겹친 1도 그대로).
    assert!(
        moves(
            &spans,
            &links,
            &HashSet::new(),
            t[0],
            span(None, Some(d("2026-09-20")))
        )
        .is_empty()
    );
    // 0을 이틀 미루면 1은 늘어난 이틀만큼, 3은 완료라 그대로.
    assert_eq!(
        moves(
            &spans,
            &links,
            &HashSet::from([t[3]]),
            t[0],
            span(None, Some(d("2026-10-03")))
        ),
        vec![
            (t[1], days("2026-10-03", "2026-10-04")),
            (t[2], days("2026-10-05", "2026-10-05")),
        ]
    );
}

#[test]
fn a_task_with_two_moved_predecessors_takes_the_later_one() {
    let t = ids(3);
    let spans = HashMap::from([
        (t[0], span(None, Some(d("2026-10-01")))),
        (t[1], span(None, Some(d("2026-10-02")))),
        (t[2], days("2026-10-03", "2026-10-03")),
    ]);
    // 0 → 1 → 2, 0 → 2.
    let links = [fs(t[0], t[1]), fs(t[1], t[2]), fs(t[0], t[2])];
    let plan = plan_shifts(
        &spans,
        &links,
        &HashSet::new(),
        Change::Move {
            task: t[0],
            next: span(None, Some(d("2026-10-04"))),
        },
    );
    assert_eq!(
        plan.shifts
            .iter()
            .map(|p| (p.task, p.span))
            .collect::<Vec<_>>(),
        vec![
            (t[1], span(None, Some(d("2026-10-05")))),
            (t[2], days("2026-10-06", "2026-10-06")),
        ]
    );
    // 2를 민 것은 1 → 2(1이 10-05에 끝나 10-06 시작). 0 → 2는 10-05만 요구해 원인이 아니다.
    assert_eq!(plan.shifts[1].causes, vec![fs(t[1], t[2])]);
}

/// 선행 A(10-05 ~ 10-09), 후속 B(10-01 ~ 10-03, 3일)를 종류·지연일마다 이어 만들 때 B의 새 날짜.
fn linked(kind: LinkKind, lag: i32) -> Vec<(Uuid, Span)> {
    let t = ids(2);
    let spans = HashMap::from([
        (t[0], days("2026-10-05", "2026-10-09")),
        (t[1], days("2026-10-01", "2026-10-03")),
    ]);
    let new = link(t[0], t[1], kind, lag);
    let plan = plan_shifts(
        &spans,
        &[],
        &HashSet::new(),
        Change::Link { old: None, new },
    );
    assert!(plan.shifts.iter().all(|p| p.causes == vec![new]));
    plan.shifts.into_iter().map(|p| (p.task, p.span)).collect()
}

#[test]
fn each_kind_anchors_the_right_ends_with_and_without_lag() {
    let b = ids(2)[1];
    // FS: B 시작 ≥ A 마감 + 1 + 지연.
    assert_eq!(
        linked(LinkKind::FinishToStart, 0),
        vec![(b, days("2026-10-10", "2026-10-12"))]
    );
    assert_eq!(
        linked(LinkKind::FinishToStart, 2),
        vec![(b, days("2026-10-12", "2026-10-14"))]
    );
    // SS: B 시작 ≥ A 시작 + 지연.
    assert_eq!(
        linked(LinkKind::StartToStart, 0),
        vec![(b, days("2026-10-05", "2026-10-07"))]
    );
    assert_eq!(
        linked(LinkKind::StartToStart, 3),
        vec![(b, days("2026-10-08", "2026-10-10"))]
    );
    // FF: B 마감 ≥ A 마감 + 지연.
    assert_eq!(
        linked(LinkKind::FinishToFinish, 0),
        vec![(b, days("2026-10-07", "2026-10-09"))]
    );
    assert_eq!(
        linked(LinkKind::FinishToFinish, 1),
        vec![(b, days("2026-10-08", "2026-10-10"))]
    );
    // SF: B 마감 ≥ A 시작 - 1 + 지연. 지연 0이면 A 시작 전날(10-04)에 끝나도 된다.
    assert_eq!(
        linked(LinkKind::StartToFinish, 0),
        vec![(b, days("2026-10-02", "2026-10-04"))]
    );
    assert_eq!(
        linked(LinkKind::StartToFinish, 1),
        vec![(b, days("2026-10-03", "2026-10-05"))]
    );
}

#[test]
fn a_link_that_is_already_satisfied_moves_nothing() {
    let t = ids(2);
    let spans = HashMap::from([
        (t[0], days("2026-10-01", "2026-10-03")),
        (t[1], days("2026-10-10", "2026-10-12")),
    ]);
    for kind in [
        LinkKind::FinishToStart,
        LinkKind::StartToStart,
        LinkKind::FinishToFinish,
        LinkKind::StartToFinish,
    ] {
        let new = link(t[0], t[1], kind, 2);
        assert_eq!(
            plan_shifts(
                &spans,
                &[],
                &HashSet::new(),
                Change::Link { old: None, new }
            ),
            Plan::default(),
            "{kind:?}"
        );
    }
}

#[test]
fn changing_a_link_pushes_only_what_the_change_adds() {
    let t = ids(3);
    let spans = HashMap::from([
        (t[0], days("2026-10-01", "2026-10-05")),
        (t[1], days("2026-10-06", "2026-10-07")),
        (t[2], days("2026-10-08", "2026-10-08")),
    ]);
    let old = fs(t[0], t[1]);
    let links = [old, fs(t[1], t[2])];
    // FS·0 → FS·3: 1은 사흘, 2는 그 뒤로 사흘.
    let lag3 = link(t[0], t[1], LinkKind::FinishToStart, 3);
    let plan = plan_shifts(
        &spans,
        &links,
        &HashSet::new(),
        Change::Link {
            old: Some(old),
            new: lag3,
        },
    );
    assert_eq!(
        plan.shifts
            .iter()
            .map(|p| (p.task, p.span, p.causes.clone()))
            .collect::<Vec<_>>(),
        vec![
            (t[1], days("2026-10-09", "2026-10-10"), vec![lag3]),
            (t[2], days("2026-10-11", "2026-10-11"), vec![fs(t[1], t[2])]),
        ]
    );
    // FS → SS는 요구가 줄어 아무것도 옮기지 않는다.
    let ss = link(t[0], t[1], LinkKind::StartToStart, 0);
    assert_eq!(
        plan_shifts(
            &spans,
            &links,
            &HashSet::new(),
            Change::Link {
                old: Some(old),
                new: ss
            }
        ),
        Plan::default()
    );
}

#[test]
fn a_join_waits_for_every_predecessor_across_kinds() {
    // 0 →(FS) 2, 1 →(FF·2) 2. 0을 미루면 2는 두 관계를 모두 채워야 한다.
    let t = ids(3);
    let spans = HashMap::from([
        (t[0], days("2026-10-01", "2026-10-02")),
        (t[1], days("2026-10-01", "2026-10-10")),
        (t[2], days("2026-10-11", "2026-10-12")),
    ]);
    let ff = link(t[1], t[2], LinkKind::FinishToFinish, 2);
    let links = [fs(t[0], t[2]), ff];
    // 0이 10-12까지면 FS가 2를 10-13 시작으로 민다. FF·2(10-12 마감)는 이미 채운다.
    let plan = plan_shifts(
        &spans,
        &links,
        &HashSet::new(),
        Change::Move {
            task: t[0],
            next: days("2026-10-11", "2026-10-12"),
        },
    );
    assert_eq!(
        plan.shifts
            .iter()
            .map(|p| (p.task, p.span))
            .collect::<Vec<_>>(),
        vec![(t[2], days("2026-10-13", "2026-10-14"))]
    );
    // 1을 미루면 FF·2만 닿는다: 1이 10-15에 끝나면 2는 10-17에 끝나야 한다.
    let plan = plan_shifts(
        &spans,
        &links,
        &HashSet::new(),
        Change::Move {
            task: t[1],
            next: days("2026-10-06", "2026-10-15"),
        },
    );
    assert_eq!(
        plan.shifts
            .iter()
            .map(|p| (p.task, p.span, p.causes.clone()))
            .collect::<Vec<_>>(),
        vec![(t[2], days("2026-10-16", "2026-10-17"), vec![ff])]
    );
}

#[test]
fn forks_and_mixed_chains_follow_each_kind() {
    // 0 →(SS·1) 1 →(SF) 2, 0 →(FF) 3. 0을 사흘 미룬다.
    let t = ids(4);
    let spans = HashMap::from([
        (t[0], days("2026-10-01", "2026-10-04")),
        (t[1], days("2026-10-02", "2026-10-03")),
        (t[2], days("2026-10-01", "2026-10-01")),
        (t[3], days("2026-10-03", "2026-10-04")),
    ]);
    let links = [
        link(t[0], t[1], LinkKind::StartToStart, 1),
        link(t[1], t[2], LinkKind::StartToFinish, 0),
        link(t[0], t[3], LinkKind::FinishToFinish, 0),
    ];
    assert_eq!(
        moves(
            &spans,
            &links,
            &HashSet::new(),
            t[0],
            days("2026-10-04", "2026-10-07")
        ),
        // 위상 순서: 0 다음 1·3(id 순), 1 다음 2.
        vec![
            // SS·1: 0이 10-04 시작 → 1은 10-05 시작.
            (t[1], days("2026-10-05", "2026-10-06")),
            // FF: 0이 10-07 마감 → 3은 10-07 마감.
            (t[3], days("2026-10-06", "2026-10-07")),
            // SF: 1이 10-05 시작 → 2는 10-04 이후 마감. 전에는 10-01 요구(이미 채움)라 사흘.
            (t[2], days("2026-10-04", "2026-10-04")),
        ]
    );
}

#[test]
fn milestones_and_one_sided_dates_use_the_date_they_have() {
    // 마일스톤(마감만) → 시작만 있는 업무.
    let t = ids(2);
    let spans = HashMap::from([
        (t[0], span(None, Some(d("2026-10-05")))),
        (t[1], span(Some(d("2026-10-03")), None)),
    ]);
    let new = link(t[0], t[1], LinkKind::StartToStart, 0);
    assert_eq!(
        plan_shifts(
            &spans,
            &[],
            &HashSet::new(),
            Change::Link { old: None, new }
        )
        .shifts
        .into_iter()
        .map(|p| (p.task, p.span))
        .collect::<Vec<_>>(),
        vec![(t[1], span(Some(d("2026-10-05")), None))]
    );
}

#[test]
fn a_new_link_to_an_undated_successor_is_reported_not_invented() {
    let t = ids(3);
    let spans = HashMap::from([
        (t[0], days("2026-10-01", "2026-10-05")),
        (t[1], span(None, None)),
        (t[2], days("2026-10-02", "2026-10-02")),
    ]);
    let links = [fs(t[1], t[2])];
    let plan = plan_shifts(
        &spans,
        &links,
        &HashSet::new(),
        Change::Link {
            old: None,
            new: fs(t[0], t[1]),
        },
    );
    assert_eq!(
        plan,
        Plan {
            shifts: vec![],
            undated: vec![t[1]]
        }
    );
}

/// 0에서 갈라지는 후속 `n`개를 모두 하루 미는 계획.
fn fork_plan(n: u128) -> Plan {
    let t = ids(n + 1);
    let mut spans = HashMap::from([(t[0], days("2026-10-01", "2026-10-05"))]);
    let links: Vec<Link> = t[1..]
        .iter()
        .map(|&s| {
            spans.insert(s, days("2026-10-06", "2026-10-06"));
            fs(t[0], s)
        })
        .collect();
    plan_shifts(
        &spans,
        &links,
        &HashSet::new(),
        Change::Move {
            task: t[0],
            next: days("2026-10-01", "2026-10-06"),
        },
    )
}

#[test]
fn the_shift_limit_matches_approval_at_499_500_and_501() {
    assert_eq!(fork_plan(499).shifts.len(), 499);
    assert_eq!(plan_block(&fork_plan(499)), None);
    assert_eq!(plan_block(&fork_plan(500)), None);
    let over = fork_plan(501);
    assert_eq!(over.shifts.len(), 501, "the plan is not cut short");
    assert_eq!(
        plan_block(&over),
        Some((PlanBlockReason::TooManyShifts, None))
    );
}

#[test]
fn proposed_dates_outside_the_saveable_years_block_the_plan() {
    let t = ids(2);
    let spans = HashMap::from([
        (t[0], days("2200-12-20", "2200-12-29")),
        (t[1], days("2200-12-30", "2200-12-31")),
    ]);
    let plan = plan_shifts(
        &spans,
        &[fs(t[0], t[1])],
        &HashSet::new(),
        Change::Move {
            task: t[0],
            next: days("2200-12-20", "2200-12-30"),
        },
    );
    // 기간을 자르지 않고 그대로 제안하되 저장할 수 없다고 알린다.
    assert_eq!(plan.shifts[0].span, days("2200-12-31", "2201-01-01"));
    assert_eq!(
        plan_block(&plan),
        Some((
            PlanBlockReason::DateOutOfRange,
            Some((t[1], d("2201-01-01")))
        ))
    );
    // 범위 끝(2200-12-31)에 딱 맞게 밀리면 저장할 수 있다.
    let spans = HashMap::from([
        (t[0], days("2200-12-20", "2200-12-28")),
        (t[1], days("2200-12-29", "2200-12-30")),
    ]);
    let edge = plan_shifts(
        &spans,
        &[fs(t[0], t[1])],
        &HashSet::new(),
        Change::Move {
            task: t[0],
            next: days("2200-12-20", "2200-12-29"),
        },
    );
    assert_eq!(edge.shifts[0].span, days("2200-12-30", "2200-12-31"));
    assert_eq!(plan_block(&edge), None);
}

#[test]
fn the_approved_list_must_match_the_plan_exactly() {
    let t = ids(2);
    let plan = Plan {
        shifts: vec![Planned {
            task: t[0],
            span: days("2026-10-02", "2026-10-03"),
            causes: vec![],
        }],
        undated: vec![],
    };
    assert!(same_shifts(
        &plan,
        &[(t[0], days("2026-10-02", "2026-10-03"))]
    ));
    assert!(!same_shifts(
        &plan,
        &[(t[0], days("2026-10-03", "2026-10-04"))]
    ));
    assert!(!same_shifts(&plan, &[]));
    assert!(!same_shifts(
        &plan,
        &[
            (t[0], days("2026-10-02", "2026-10-03")),
            (t[1], days("2026-10-02", "2026-10-03"))
        ]
    ));
}

#[test]
fn link_kinds_round_trip_their_codes() {
    for kind in [
        LinkKind::FinishToStart,
        LinkKind::StartToStart,
        LinkKind::FinishToFinish,
        LinkKind::StartToFinish,
    ] {
        assert_eq!(LinkKind::parse(kind.as_str()), Some(kind));
        assert_eq!(
            serde_json::to_value(kind).unwrap(),
            serde_json::json!(kind.as_str())
        );
    }
    assert_eq!(LinkKind::parse("fs"), None);
}
