//! CI가 이번 실행에서 돌 job을 변경 경로로 정한다(#146·#155). `.github/workflows/ci.yml`의 `changes` job이 부르고,
//! 결과를 `$GITHUB_OUTPUT`(job마다 `true`/`false`)과 `$GITHUB_STEP_SUMMARY`(건너뛴 job과 사유)에 쓴다.
//!
//! PR(`pull_request`)만 좁힌다. 체크아웃된 병합 커밋과 그 첫 부모(기준 브랜치)를 비교한다. 그 밖의 이벤트(`push: main`)와
//! 판별할 수 없는 경우는 전체를 돈다. 아래 표에 없는 경로도 전체를 돈다(허용 목록).

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use xshell::{Shell, cmd};

/// 경로가 요구하는 검사 범위. 뒤 영역은 앞 영역이 돌리는 job을 모두 돌린다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Area {
    /// `docs/**`와 루트 Markdown(도구 지침 제외). check의 서식 검사만.
    Docs,
    /// 웹·패키지(`apps/**`·`packages/**`·pnpm·TS·ESLint·Prettier 설정). check·licenses·integration·browser.
    Web,
    /// 서버·SQL(`crates/**`)·계약(`contracts/**`)·Rust 설정·배포(`deploy/**`). 위에 oidc·supabase를 더한다.
    Server,
    /// rhwp WASM 입력(`deploy/rhwp/**`)과 그것을 빌드하는 `deploy/docker/app.Dockerfile`. 위에 rhwp를 더한다.
    Rhwp,
    /// CI·도구 지침(`.github/**`·`xtask/**`·`AGENTS.md`·`.agents/**`)과 표에 없는 경로. 전체.
    Full,
}

impl Area {
    fn of(path: &str) -> Self {
        let file = path.rsplit('/').next().unwrap_or(path);
        if TOOL_FILES.contains(&file) {
            return Self::Full;
        }
        let under = |dirs: &[&str]| dirs.iter().any(|dir| path.starts_with(dir));
        if path.starts_with("docs/") || (!path.contains('/') && path.ends_with(".md")) {
            Self::Docs
        } else if under(&["deploy/rhwp/"]) || path == "deploy/docker/app.Dockerfile" {
            Self::Rhwp
        } else if under(&["crates/", "contracts/", "deploy/", ".cargo/"])
            || SERVER_FILES.contains(&path)
        {
            Self::Server
        } else if under(&["apps/", "packages/"]) || WEB_FILES.contains(&path) {
            Self::Web
        } else {
            Self::Full
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Docs => "문서",
            Self::Web => "웹",
            Self::Server => "서버·SQL·배포",
            Self::Rhwp => "rhwp",
            Self::Full => "CI·도구 지침·표 밖",
        }
    }
}

/// 에이전트 도구가 읽는 지침. 어느 폴더에 있든 Markdown이어도 전체를 돈다.
const TOOL_FILES: [&str; 2] = ["AGENTS.md", "CLAUDE.md"];
const SERVER_FILES: [&str; 6] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "clippy.toml",
    "deny.toml",
];
const WEB_FILES: [&str; 8] = [
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "tsconfig.base.json",
    "eslint.config.mjs",
    ".prettierrc.json",
    ".prettierignore",
    ".npmrc",
];

/// check를 뺀 job. check는 늘 돌고, 문서 전용 PR에서는 서식 검사만 돈다(`cargo xtask check --docs-only`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    Licenses,
    Integration,
    Browser,
    Oidc,
    Supabase,
    Rhwp,
}

impl Job {
    const ALL: [Self; 6] = [
        Self::Licenses,
        Self::Integration,
        Self::Browser,
        Self::Oidc,
        Self::Supabase,
        Self::Rhwp,
    ];

    /// ci.yml의 job 이름이자 출력 이름.
    fn name(self) -> &'static str {
        match self {
            Self::Licenses => "licenses",
            Self::Integration => "integration",
            Self::Browser => "browser",
            Self::Oidc => "oidc",
            Self::Supabase => "supabase",
            Self::Rhwp => "rhwp",
        }
    }

    /// 이 job을 돌리는 가장 좁은 영역.
    fn area(self) -> Area {
        match self {
            Self::Licenses | Self::Integration | Self::Browser => Area::Web,
            Self::Oidc | Self::Supabase => Area::Server,
            Self::Rhwp => Area::Rhwp,
        }
    }

    /// 요약·알림에 보일 건너뜀 표기. rhwp는 빌드(출력 키 `rhwp`)만 건너뛰고 권고 검사는 모든 PR에서 돈다.
    fn skipped_label(self) -> &'static str {
        match self {
            Self::Rhwp => "권고 검사(빌드는 캐시 재사용)",
            _ => "건너뜀",
        }
    }

    fn skip_reason(self) -> &'static str {
        match self {
            Self::Licenses | Self::Integration | Self::Browser => {
                "문서 전용 PR(docs/**·루트 Markdown만 바뀜)"
            }
            Self::Oidc | Self::Supabase => {
                "서버(crates/**)·SQL·계약·Rust 설정·배포(deploy/**)·CI·도구 지침 경로가 바뀌지 않음"
            }
            Self::Rhwp => {
                "rhwp 입력(deploy/rhwp/**·deploy/docker/app.Dockerfile)·CI·도구 지침 경로가 바뀌지 않음"
            }
        }
    }
}

/// 이번 실행의 검사 범위와 그 근거.
#[derive(Debug, PartialEq, Eq)]
struct Plan {
    area: Area,
    basis: String,
}

impl Plan {
    fn full(basis: impl Into<String>) -> Self {
        Self {
            area: Area::Full,
            basis: basis.into(),
        }
    }

    /// PR이 바꾼 경로로 정한다. 가장 넓은 영역을 따르고, 그 영역을 만든 경로를 근거에 몇 개 적는다.
    fn for_paths<'p>(paths: impl IntoIterator<Item = &'p str>) -> Self {
        let classified: Vec<(&str, Area)> = paths.into_iter().map(|p| (p, Area::of(p))).collect();
        let Some(area) = classified.iter().map(|(_, area)| *area).max() else {
            return Self::full("PR의 변경 경로가 없어 판별할 수 없음");
        };
        let deciding: Vec<&str> = classified
            .iter()
            .filter(|(_, a)| *a == area)
            .map(|(p, _)| *p)
            .collect();
        let mut basis = format!(
            "변경 경로 {}개 중 가장 넓은 영역 {}: {}",
            classified.len(),
            area.label(),
            deciding
                .iter()
                .take(5)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        );
        if deciding.len() > 5 {
            basis.push_str(&format!(" 외 {}개", deciding.len() - 5));
        }
        Self { area, basis }
    }

    fn docs_only(&self) -> bool {
        self.area == Area::Docs
    }

    fn runs(&self, job: Job) -> bool {
        self.area >= job.area()
    }

    /// `$GITHUB_OUTPUT` 줄. ci.yml은 `'false'`일 때만 건너뛴다.
    fn outputs(&self) -> String {
        let mut out = format!("docs_only={}\n", self.docs_only());
        for job in Job::ALL {
            out.push_str(&format!("{}={}\n", job.name(), self.runs(job)));
        }
        out
    }

    /// 실행 요약(Markdown). 건너뛴 job은 결론이 skipped로만 보이므로 사유는 여기에 남긴다.
    fn summary(&self) -> String {
        let mut out = format!(
            "### CI 실행 범위 (#146·#155)\n\n근거: {}\n\n| job | 실행 | 사유 |\n| --- | --- | --- |\n",
            self.basis
        );
        out.push_str(if self.docs_only() {
            "| check | 서식 검사만 | 문서 전용 PR: `cargo xtask check --docs-only` |\n"
        } else {
            "| check | 실행 | |\n"
        });
        for job in Job::ALL {
            out.push_str(&if self.runs(job) {
                format!("| {} | 실행 | |\n", job.name())
            } else {
                format!(
                    "| {} | {} | {} |\n",
                    job.name(),
                    job.skipped_label(),
                    job.skip_reason()
                )
            });
        }
        out
    }
}

/// `event`는 `$GITHUB_EVENT_NAME`. `pull_request`만 변경 경로를 본다.
pub fn run(root: &Path, event: &str) -> Result<()> {
    let plan = if event == "pull_request" {
        let sh = Shell::new()?;
        sh.change_dir(root);
        pull_request_plan(&sh)?
    } else {
        Plan::full(format!("{event} 이벤트는 전체를 돈다"))
    };

    print!("{}", plan.summary());
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        for job in Job::ALL.into_iter().filter(|job| !plan.runs(*job)) {
            println!(
                "::notice title={} {}::{}",
                job.name(),
                job.skipped_label(),
                job.skip_reason()
            );
        }
    }
    append_env_file("GITHUB_OUTPUT", &plan.outputs())?;
    append_env_file("GITHUB_STEP_SUMMARY", &plan.summary())
}

/// 체크아웃은 PR 병합 커밋이다(첫 부모가 기준 브랜치). 병합 커밋이 아니면 비교 기준을 믿을 수 없으므로 전체를 돈다.
fn pull_request_plan(sh: &Shell) -> Result<Plan> {
    let parents = cmd!(sh, "git rev-list --parents -n 1 HEAD").read()?;
    if parents.split_whitespace().count() != 3 {
        return Ok(Plan::full(format!(
            "HEAD가 PR 병합 커밋이 아님(git rev-list --parents: {parents})"
        )));
    }
    // 이름 바꾸기는 옛 경로와 새 경로를 모두 센다.
    let diff = cmd!(sh, "git diff --name-only --no-renames -z HEAD^1 HEAD")
        .read()
        .context("PR 변경 경로")?;
    Ok(Plan::for_paths(diff.split('\0').filter(|p| !p.is_empty())))
}

/// GitHub Actions가 넘긴 파일에 덧붙인다. 로컬처럼 변수가 없으면 쓰지 않는다.
fn append_env_file(var: &str, text: &str) -> Result<()> {
    let Some(path) = std::env::var_os(var) else {
        return Ok(());
    };
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .with_context(|| format!("open ${var}"))?;
    file.write_all(text.as_bytes())
        .with_context(|| format!("write ${var}"))
}

#[cfg(test)]
mod tests {
    use super::{Job, Plan};

    /// 경로 묶음에서 실제로 도는 job 이름(check 제외).
    fn running(paths: &[&str]) -> Vec<&'static str> {
        let plan = Plan::for_paths(paths.iter().copied());
        Job::ALL
            .into_iter()
            .filter(|job| plan.runs(*job))
            .map(Job::name)
            .collect()
    }

    const BASIC: [&str; 3] = ["licenses", "integration", "browser"];
    const SERVER: [&str; 5] = ["licenses", "integration", "browser", "oidc", "supabase"];
    const ALL: [&str; 6] = [
        "licenses",
        "integration",
        "browser",
        "oidc",
        "supabase",
        "rhwp",
    ];

    #[test]
    fn docs_only_pr_runs_format_check_only() {
        let paths = [
            "docs/decisions/delivery.md",
            "docs/decisions.md",
            "README.md",
            "docs/images/flow.png",
        ];
        assert!(Plan::for_paths(paths).docs_only());
        assert!(running(&paths).is_empty());
    }

    #[test]
    fn web_only_pr_runs_check_licenses_integration_browser() {
        for paths in [
            &["apps/web/src/lib/features/files/hwp.ts"][..],
            &["packages/api-client/src/index.ts", "pnpm-lock.yaml"],
            &["apps/web/src/lib/ui/README.md"],
            &["eslint.config.mjs", ".prettierrc.json"],
        ] {
            assert!(!Plan::for_paths(paths.iter().copied()).docs_only());
            assert_eq!(running(paths), BASIC, "{paths:?}");
        }
    }

    #[test]
    fn server_sql_deploy_pr_adds_oidc_and_supabase() {
        for path in [
            "crates/server/src/main.rs",
            "crates/server/migrations/202610250001_baseline.sql",
            "deploy/compose.test.yaml",
            "deploy/supabase/test.env",
            "contracts/openapi.json",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
        ] {
            assert_eq!(running(&[path]), SERVER, "{path}");
        }
    }

    #[test]
    fn rhwp_inputs_add_rhwp() {
        for path in [
            "deploy/rhwp/inputs.env",
            "deploy/rhwp/THIRD_PARTY_NOTICES.txt",
            "deploy/docker/app.Dockerfile",
        ] {
            assert_eq!(running(&[path]), ALL, "{path}");
        }
        assert_eq!(running(&["deploy/docker/dev.Dockerfile"]), SERVER);
    }

    #[test]
    fn ci_and_tool_files_run_everything() {
        for path in [
            ".github/workflows/ci.yml",
            ".github/workflows/release-image.yml",
            "xtask/src/check.rs",
            "AGENTS.md",
            "CLAUDE.md",
            "docs/AGENTS.md",
            ".agents/skills/web/SKILL.md",
        ] {
            assert_eq!(running(&[path]), ALL, "{path}");
        }
    }

    #[test]
    fn unlisted_paths_run_everything() {
        for path in [
            ".coderabbit.yaml",
            ".gitignore",
            "THIRD_PARTY_NOTICES.txt",
            "README.MD",
        ] {
            assert_eq!(running(&[path]), ALL, "{path}");
        }
        let empty = Plan::for_paths([]);
        assert_eq!(empty, Plan::full("PR의 변경 경로가 없어 판별할 수 없음"));
    }

    #[test]
    fn mixed_pr_follows_widest_area() {
        assert_eq!(
            running(&["docs/decisions.md", "apps/web/package.json"]),
            BASIC
        );
        assert_eq!(running(&["docs/decisions.md", "AGENTS.md"]), ALL);
        assert_eq!(running(&["README.md", ".agents/skills/sql/SKILL.md"]), ALL);
        assert_eq!(
            running(&["apps/web/src/app.html", "crates/server/src/lib.rs"]),
            SERVER
        );
        assert_eq!(
            running(&["crates/server/src/lib.rs", "deploy/rhwp/build.sh"]),
            ALL
        );
        assert!(!Plan::for_paths(["docs/decisions.md", "apps/web/package.json"]).docs_only());
    }

    #[test]
    fn non_pull_request_events_run_everything() {
        let plan = Plan::full("push 이벤트는 전체를 돈다");
        assert!(!plan.docs_only());
        assert!(Job::ALL.into_iter().all(|job| plan.runs(job)));
    }

    #[test]
    fn rhwp_not_built_shows_audit_only_in_summary() {
        for paths in [
            &["docs/decisions.md"][..],
            &["apps/web/src/app.html"],
            &["crates/server/src/lib.rs"],
        ] {
            let plan = Plan::for_paths(paths.iter().copied());
            assert!(!plan.runs(Job::Rhwp), "{paths:?}");
            assert!(plan.outputs().ends_with("rhwp=false\n"), "{paths:?}");
            let summary = plan.summary();
            assert!(
                summary.contains("| rhwp | 권고 검사(빌드는 캐시 재사용) | rhwp 입력"),
                "{summary}"
            );
            assert!(!summary.contains("| rhwp | 건너뜀 |"), "{summary}");
        }

        let built = Plan::for_paths(["deploy/rhwp/build.sh"]);
        assert!(built.summary().contains("| rhwp | 실행 | |"));
        assert!(built.outputs().ends_with("rhwp=true\n"));
    }

    #[test]
    fn outputs_and_summary_name_every_skipped_job_with_reason() {
        let plan = Plan::for_paths(["apps/web/src/app.html"]);
        assert_eq!(
            plan.outputs(),
            "docs_only=false\nlicenses=true\nintegration=true\nbrowser=true\n\
             oidc=false\nsupabase=false\nrhwp=false\n"
        );
        let summary = plan.summary();
        for job in [Job::Oidc, Job::Supabase, Job::Rhwp] {
            let row = format!(
                "| {} | {} | {} |",
                job.name(),
                job.skipped_label(),
                job.skip_reason()
            );
            assert!(summary.contains(&row), "{summary}");
        }

        let docs = Plan::for_paths(["docs/decisions.md"]);
        assert!(docs.outputs().starts_with("docs_only=true\n"));
        assert!(docs.summary().contains("| check | 서식 검사만 |"));
        assert!(
            docs.summary()
                .contains("| licenses | 건너뜀 | 문서 전용 PR")
        );
    }
}
