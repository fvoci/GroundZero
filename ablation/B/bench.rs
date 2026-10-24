//! Experimental harness only. Extracted production functions live in sibling files.
use serde_json::{Value, json};
use std::{hint::black_box, time::Instant};

#[derive(Debug)]
pub enum AppError { BadRequest(String) }
pub struct Limits {
    pub max_document_bytes: usize,
    pub max_references_per_document: usize,
}
mod baseline {
    use super::{AppError, Limits};
    use serde_json::Value;
    use uuid::Uuid;
    include!("baseline-helper.rs");
}
mod variant {
    use super::{AppError, Limits};
    use serde_json::Value;
    use uuid::Uuid;
    include!("variant-helper.rs");
}
type Validator = fn(&Value, &Limits) -> Result<(), AppError>;

fn outcome(f: Validator, body: &Value, limits: &Limits) -> Result<(), String> {
    f(body, limits).map_err(|AppError::BadRequest(s)| s)
}

fn sample(f: Validator, body: &Value, limits: &Limits, iterations: usize) -> u128 {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(f(black_box(body), black_box(limits))).ok();
    }
    start.elapsed().as_nanos()
}

fn main() {
    let reference = json!({"type":"reference", "attrs":{
        "kind":"item", "id":"0198f1f2-0000-7000-8000-000000000001"}});
    let mut deep = json!(null);
    for _ in 0..128 { deep = json!([deep]); }
    let fixtures = [
        ("minimal", json!({"type":"doc"}), false, 10000),
        ("small_prose", json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Ordinary document prose. ".repeat(16)}]}]}), false, 2000),
        ("unicode_escapes", json!({"type":"doc","text":"한글🦀é\"\\\n\r\t\u{0000}\u{001f}".repeat(4096)}), false, 100),
        ("many_nodes", json!({"type":"doc", "content":vec![json!({"type":"paragraph","content":[{"type":"text","text":"Node text"}]} );1000]}), false, 100),
        ("large_exact", json!({"type":"doc", "text":"x".repeat(1_999_976)}), false, 32),
        ("large_rejected", json!({"type":"doc", "text":"x".repeat(1_999_977)}), true, 32),
        ("deep", json!({"type":"doc", "content":deep}), false, 2000),
        ("references", json!({"type":"doc", "content":vec![reference;100]}), false, 200),
    ];
    // Fresh process, first validator/serializer invocation; fixture allocation still occurred.
    // This is not a controlled cold CPU or OS page-cache measurement.
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--first-validator") {
        let selected = args.get(2).unwrap();
        let fixture = args.get(3).unwrap();
        let (name, body, rejected, _) = fixtures.iter().find(|(name, ..)| *name == fixture.as_str()).unwrap();
        let limits = Limits { max_document_bytes: if *rejected { 0 } else { usize::MAX }, max_references_per_document: 100 };
        let f = if selected == "baseline" { baseline::validate_body } else { variant::validate_body };
        let elapsed = sample(f, body, &limits, 1);
        println!("first_validator,{name},0,{selected},1,{elapsed},0,0");
        return;
    }
    println!("kind,fixture,pair,variant,iterations,total_ns,serialized_bytes,vec_capacity_bytes");
    for (name, body, rejected, iterations) in fixtures {
        let serialized = serde_json::to_vec(&body).unwrap();
        let size = serialized.len();
        let capacity = serialized.capacity();
        drop(serialized);
        let limits = Limits { max_document_bytes: size - usize::from(rejected), max_references_per_document: 100 };
        for bytes in [0, size - 1, size, size + 1, usize::MAX] {
            for refs in [0, 2, 100] {
                let boundary = Limits { max_document_bytes: bytes, max_references_per_document: refs };
                assert_eq!(outcome(baseline::validate_body, &body, &boundary), outcome(variant::validate_body, &body, &boundary), "{name}/{bytes}/{refs}");
            }
        }
        // First measured calls after fixture construction/parity checking; not OS cold cache.
        for (label, f) in [("baseline", baseline::validate_body as Validator), ("variant", variant::validate_body)] {
            println!("first_call,{name},0,{label},1,{},{size},{capacity}", sample(f, &body, &limits, 1));
        }
        for _ in 0..64 {
            black_box(baseline::validate_body(&body, &limits)).ok();
            black_box(variant::validate_body(&body, &limits)).ok();
        }
        for pair in 0..41 {
            let order: [(&str, Validator);2] = if pair % 2 == 0 {
                [("baseline", baseline::validate_body), ("variant", variant::validate_body)]
            } else {
                [("variant", variant::validate_body), ("baseline", baseline::validate_body)]
            };
            for (label, f) in order {
                let ns = sample(f, &body, &limits, iterations);
                println!("warm,{name},{pair},{label},{iterations},{ns},{size},{capacity}");
            }
        }
    }
}
