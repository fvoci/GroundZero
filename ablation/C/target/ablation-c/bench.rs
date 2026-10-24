use sha2::{Digest, Sha256};
use std::{hint::black_box, time::Instant};

fn baseline(digest: &[u8]) -> String {
    let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

fn candidate(digest: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut etag = String::with_capacity(34);
    etag.push('"');
    for byte in &digest[..16] {
        etag.push(char::from(HEX[usize::from(byte >> 4)]));
        etag.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    etag.push('"');
    etag
}

fn hash(user: &[u8], body: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"fvoci-representation-1\0");
    hasher.update(user);
    hasher.update(b"\0");
    hasher.update(body);
    hasher.finalize().into()
}

// Original production function, with only its name changed.
fn baseline_etag(user: &[u8], body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"fvoci-representation-1\0");
    hasher.update(user);
    hasher.update(b"\0");
    hasher.update(body);
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

fn candidate_etag(user: &[u8], body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"fvoci-representation-1\0");
    hasher.update(user);
    hasher.update(b"\0");
    hasher.update(body);
    let digest = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut etag = String::with_capacity(34);
    etag.push('"');
    for byte in &digest[..16] {
        etag.push(char::from(HEX[usize::from(byte >> 4)]));
        etag.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    etag.push('"');
    etag
}

fn sample(f: &dyn Fn(usize) -> String, n: usize) -> f64 {
    let start = Instant::now();
    for i in 0..n { black_box(f(black_box(i))); }
    start.elapsed().as_nanos() as f64 / n as f64
}

fn verify() {
    let users = [b"user-000000000001", b"user-000000000002", b"user-000000000003"];
    let vectors: &[(&[u8], &[u8])] = &[(b"", b""), (b"u1", b"{}"), (b"u2", b"{}"), (b"u1", b"{ }")];
    for (user, body) in vectors {
        let digest = hash(user, body);
        assert_eq!(baseline(&digest), candidate(&digest));
        assert_eq!(baseline_etag(user, body), candidate_etag(user, body));
        eprintln!("vector {user:?} {body:?}: {}", baseline(&digest));
    }
    // Deterministic binary corpus exercises every byte value without new dependencies.
    let mut seed = 0x12345678u32;
    for len in 0..1024 {
        let body: Vec<u8> = (0..len).map(|_| { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; seed as u8 }).collect();
        for user in users {
            let digest = hash(user, &body);
            assert_eq!(baseline(&digest), candidate(&digest));
            assert_eq!(baseline_etag(user, &body), candidate_etag(user, &body));
        }
    }
    eprintln!("PASS: 4 fixed vectors and 3072 deterministic binary/user pairs");
}

fn main() {
    if std::env::args().any(|arg| arg == "--verify-only") {
        verify();
        return;
    }
    let users = [b"user-000000000001", b"user-000000000002", b"user-000000000003"];
    println!("case,round,order,iterations,baseline_ns,candidate_ns");
    for (case, size, n) in [("hex", 0, 20000), ("empty", 0, 20000), ("small", 256, 10000), ("large", 1048576, 100)] {
        let body: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let digests: Vec<_> = users.iter().map(|u| hash(*u, &body)).collect();
        let a = |i: usize| if case == "hex" { baseline(black_box(&digests[i % 3])) } else { baseline_etag(black_box(users[i % 3]), black_box(&body)) };
        let b = |i: usize| if case == "hex" { candidate(black_box(&digests[i % 3])) } else { candidate_etag(black_box(users[i % 3]), black_box(&body)) };
        let cold_a = sample(&a, 1);
        let cold_b = sample(&b, 1);
        println!("{case},-1,AB,1,{cold_a:.3},{cold_b:.3}");
        for _ in 0..3 { sample(&a, n); sample(&b, n); }
        for round in 0..31 {
            let (an, bn, order) = if round % 2 == 0 { let an = sample(&a,n); (an,sample(&b,n),"AB") } else { let bn = sample(&b,n); (sample(&a,n),bn,"BA") };
            println!("{case},{round},{order},{n},{an:.3},{bn:.3}");
        }
    }
}
