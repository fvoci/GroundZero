//! #240: acceptance is the exact installed name set, including case and prefixes.
use crate::common;
use sqlx::{Connection, PgConnection};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::test]
async fn timezone_validation_matches_the_complete_name_and_rejection_matrix() -> Result<()> {
    let mut admin = PgConnection::connect(&std::env::var("FVOCI_TEST_ADMIN_URL")?).await?;
    let fixture = common::fresh_database(&mut admin, "timezone_names").await;
    let mut conn = PgConnection::connect(&fixture.app_url).await?;
    // Freeze the reference catalogue once. Production validation is independently
    // executed for every input, under the app role, without replacing its source.
    sqlx::raw_sql(
        r"
      CREATE TEMP TABLE timezone_reference_names AS SELECT name FROM pg_timezone_names;
      CREATE TEMP TABLE timezone_reference_inputs AS
      SELECT name FROM timezone_reference_names
      UNION SELECT lower(name) FROM timezone_reference_names
      UNION SELECT upper(name) FROM timezone_reference_names
      UNION SELECT 'posix/'||name FROM timezone_reference_names
      UNION SELECT 'right/'||name FROM timezone_reference_names
      UNION SELECT 'SystemV/'||name FROM timezone_reference_names
      UNION SELECT abbrev FROM pg_timezone_abbrevs
      UNION SELECT unnest(ARRAY[NULL,'','Factory','factory','FACTORY','localtime','LOCALTIME',
        'posixrules','POSIXRULES','EST','est','PST','CST','UTC','utc','GMT','GMT+3','UTC+03',
        '+09','-09','+09:00','GMT+25','GMT-25','EST5EDT','xxx0','UTC0',':Asia/Seoul',':UTC',
        'Asia/Seoul ','Asia/Seoul','Not/AZone','POSIX/Asia/Seoul','RIGHT/Asia/Seoul',
        'systemv/EST5EDT','SystemV/EST5EDT','24:00','252:00']);
    ",
    )
    .execute(&mut conn)
    .await?;
    let inputs: Vec<(Option<String>, bool)> = sqlx::query_as(
        r"
      SELECT i.name, coalesce(i.name IS NOT NULL AND i.name !~ '^(posix|right|SystemV)/'
        AND i.name NOT IN ('Factory','localtime','posixrules')
        AND EXISTS(SELECT 1 FROM timezone_reference_names n WHERE n.name=i.name),false)
      FROM timezone_reference_inputs i ORDER BY i.name NULLS FIRST
    ",
    )
    .fetch_all(&mut conn)
    .await?;
    let mut allowed = 0;
    let mut rejected = 0;
    for (name, expected) in &inputs {
        let result = sqlx::query("SELECT fvoci.app_check_timezone($1)")
            .bind(name)
            .execute(&mut conn)
            .await;
        if *expected {
            result?;
            allowed += 1;
        } else {
            let error = result
                .err()
                .ok_or_else(|| format!("unexpectedly accepted {name:?}"))?;
            assert_eq!(
                error.as_database_error().and_then(|e| e.code()).as_deref(),
                Some("22023"),
                "name={name:?}"
            );
            rejected += 1;
        }
    }
    assert_eq!(allowed + rejected, inputs.len());
    assert!(
        allowed > 0 && rejected > 0,
        "matrix must cover both outcomes"
    );
    println!(
        "timezone.matrix inputs={} allowed={allowed} rejected={rejected} differences=0",
        inputs.len()
    );
    sqlx::raw_sql("DROP TABLE timezone_reference_inputs,timezone_reference_names")
        .execute(&mut conn)
        .await?;
    conn.close().await?;
    sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fixture.name))
        .execute(&mut admin)
        .await?;
    Ok(())
}
