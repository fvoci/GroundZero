mod baseline;
mod variant;

use serde_json::{Value, json};
use std::hint::black_box;
use std::time::Instant;

fn text(value: &str) -> Value { json!({"type":"text", "text":value}) }
fn doc(nodes: Vec<Value>) -> Value { json!({"type":"doc", "content":nodes}) }
fn block(kind: &str, nodes: Vec<Value>) -> Value { json!({"type":kind,"content":nodes}) }

fn fixtures() -> Vec<(String, Value)> {
    let mut out = vec![
        ("null".into(), Value::Null),
        ("array".into(), json!([text("ignored")])),
        ("malformed".into(), doc(vec![json!(3), json!({"type":4}), json!({"type":"text","text":3}), json!({"content":"bad"})])),
        ("unknown_children".into(), block("unknown",vec![text("seen")])),
        ("unicode".into(), doc(vec![text("  가😀e\u{301}\t"),json!({"type":"hardBreak"}),text("한글 👩‍💻  ")])),
        ("references".into(),doc(vec![json!({"type":"reference"}),json!({"type":"reference","attrs":{"kind":"document","id":"가😀","label":"must not appear"}})])),
        ("checklists".into(),doc(vec![json!({"type":"taskItem","attrs":{"checked":true},"content":[text("yes")]}),json!({"type":"taskItem","attrs":{"checked":false},"content":[text("no")]}),json!({"type":"taskItem","attrs":{"checked":"bad"}})])),
        ("whitespace".into(),doc(vec![text(" \t\r\n\u{2003} "),block("paragraph",vec![])])),
        ("text_no_children".into(),json!({"type":"text","content":[text("ignored")]})),
    ];
    for kind in ["paragraph","heading","blockquote","codeBlock","listItem","taskItem","horizontalRule","table","tableRow"] {
        out.push((format!("block_{kind}"),doc(vec![block(kind,vec![text("가😀e\u{301}")]),text("tail")])));
    }
    for n in [99_999,100_000,100_001] {
        out.push((format!("cap_{n}"), doc(vec![text(&"가".repeat(n)),text("TAIL")])));
    }
    out.push(("huge_leading_whitespace".into(),doc(vec![block("blockquote",vec![block("paragraph",vec![text(&format!("{}{}  "," ".repeat(100_002),"😀".repeat(100_005)))])]),text("TAIL")])));
    out.push(("ancestor_newline_at_cap".into(),doc(vec![block("blockquote",vec![block("paragraph",vec![text(&"x".repeat(99_999))])]),text("TAIL")])));
    out.push(("marker_crosses_cap".into(),doc(vec![text(&"x".repeat(99_998)),json!({"type":"taskItem","content":[text("TAIL")]}),text("TAIL")])));
    out.push(("reference_crosses_cap".into(),doc(vec![text(&"x".repeat(99_998)),json!({"type":"reference","attrs":{"kind":"user","id":"x"}}),text("TAIL")])));
    out.push(("break_crosses_cap".into(),doc(vec![text(&"x".repeat(99_999)),json!({"type":"hardBreak"}),text("TAIL")])));
    out
}

fn hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |a,b| (a ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

fn measure_case(name: &str) -> (Value, usize) {
    match name {
        "tiny" => (doc(vec![block("paragraph",vec![text("가😀e\u{301} hello")])]), 30_000),
        "single_huge" => (doc(vec![text(&"가".repeat(100_010))]), 300),
        "many_ascii" => (doc((0..10_000).map(|_|text("abcdefghij")).collect()), 4),
        "many_unicode" => (doc((0..10_000).map(|_|text("가😀e\u{301}한글abcd")).collect()), 4),
        "many_blocks" => (doc((0..5_000).map(|_|block("paragraph",vec![text("가😀abcdef")])).collect()), 4),
        _ => panic!("unknown case"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(feature = "variant")]
    let compute = variant::plain_text;
    #[cfg(not(feature = "variant"))]
    let compute = baseline::plain_text;
    if args.get(1).map(String::as_str) == Some("verify") {
        let mut count = 0;
        for (name, body) in fixtures() {
            assert_eq!(baseline::plain_text(&body), variant::plain_text(&body), "{name}");
            count += 1;
        }
        // Deterministic mixed trees exercise count boundaries between all append kinds.
        for seed in 0..300usize {
            let mut nodes = Vec::new();
            for i in 0..(seed % 37 + 1) {
                let leaf = match (seed + i) % 7 {
                    0 => text("  가😀e\u{301}  "),
                    1 => json!({"type":"hardBreak"}),
                    2 => json!({"type":"reference", "attrs":{"kind":"user","id":"α","label":"hidden"}}),
                    3 => Value::Null,
                    4 => block("taskItem",vec![text("☕👩‍💻")]),
                    5 => block("unknown",vec![text("tail")]),
                    _ => text("\t\n"),
                };
                nodes.push(block(["paragraph","blockquote","tableRow"][i % 3],vec![leaf]));
            }
            if seed % 5 == 0 { nodes.insert(0,text(&"가".repeat(99_950 + seed % 53))); }
            let body = doc(nodes);
            assert_eq!(baseline::plain_text(&body), variant::plain_text(&body), "seed {seed}");
            count += 1;
        }
        println!("PASS exact baseline/variant output equality: {count} fixtures");
        return;
    }
    if args.get(1).map(String::as_str) == Some("golden") {
        for (name, body) in fixtures() {
            let output = compute(&body);
            println!("{name}\t{}\t{}\t{:016x}",output.len(),output.chars().count(),hash(&output));
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("measure") {
        let mode = &args[2];
        let case = &args[3];
        let (body, iterations) = measure_case(case);
        // Process-cold means no explicit warmup; fixture construction is outside timing.
        if mode == "warm" { for _ in 0..5 { black_box(compute(black_box(&body))); } }
        let started = Instant::now();
        let mut checksum = 0u64;
        for _ in 0..iterations {
            let output = black_box(compute(black_box(&body)));
            checksum = checksum.wrapping_add(black_box(output.len() as u64));
        }
        println!("{mode}\t{case}\t{iterations}\t{}\t{checksum}",started.elapsed().as_nanos());
        return;
    }
    panic!("use golden or measure warm|cold case");
}
