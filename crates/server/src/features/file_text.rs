//! 첨부 본문 글자 뽑기 (D-39 "지원 형식의 첨부 본문 검색"). 색인에만 쓴다.
//!
//! 지원: 글자 형식(text/*, JSON, XML, CSV, Markdown), Office Open XML(DOCX·PPTX·XLSX), HWPX. 앞의 셋과 HWPX는 ZIP 안의
//! XML이므로 이미 쓰는 `zip`으로 읽고(디스크에 풀지 않음) 태그를 걷어 낸다. PDF는 pdf-extract로 읽되 압축 스트림을
//! 한도 안에서 미리 풀어 본 뒤에만 넘긴다(D-73). HWP(이진)·옛 Office 이진 형식은 아직 뽑지 않는다(이름으로만 찾는다).

use std::io::{Cursor, Read};

use crate::features::search_text::MAX_INDEX_CHARS;

/// 이보다 큰 파일은 글자를 뽑지 않는다(색인은 이름으로만).
pub const MAX_EXTRACT_BYTES: u64 = 20 * 1024 * 1024;
/// 한 파일의 글자 뽑기 시간 한도. 넘으면 이름으로만 색인한다.
pub const EXTRACT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// ZIP 안 한 항목에서 읽는 최대 크기(압축 폭탄 방지).
const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
/// PDF 안 압축 스트림을 모두 풀었을 때의 최대 크기(압축 폭탄 방지). 넘으면 글자를 뽑지 않는다.
const MAX_PDF_INFLATED_BYTES: u64 = 64 * 1024 * 1024;

/// PDF를 해석기에 넘기기 전에 압축 스트림을 한도 안에서 미리 풀어 본다. lopdf는 Flate·LZW 스트림을 크기 한도 없이
/// 풀어(메모리 고갈로 프로세스가 죽을 수 있다) 해석기 밖에서 막는다. 스트림 경계(`stream`…`endstream`)마다 zlib과
/// lopdf가 실패 때 쓰는 날 deflate를 모두 시도해 푼 크기를 더한다. 크기를 미리 알 수 없는 LZW와, 암호화되어 미리 풀 수
/// 없는 PDF는 뽑지 않는다.
fn pdf_within_limits(bytes: &[u8]) -> bool {
    if contains(bytes, b"/LZW") || contains(bytes, b"/Encrypt") {
        return false;
    }
    let mut budget = MAX_PDF_INFLATED_BYTES;
    let mut rest = bytes;
    while let Some(at) = find(rest, b"stream") {
        let is_end = at >= 3 && &rest[at - 3..at] == b"end";
        rest = &rest[at + b"stream".len()..];
        if is_end {
            continue;
        }
        let data = rest
            .strip_prefix(b"\r\n")
            .or_else(|| rest.strip_prefix(b"\n"))
            .or_else(|| rest.strip_prefix(b"\r"));
        let Some(data) = data else {
            continue;
        };
        let end = find(data, b"endstream").unwrap_or(data.len());
        let raw = &data[..end];
        let zlib = inflated_len(flate2::read::ZlibDecoder::new(raw), budget);
        let deflate = raw.get(2..).map_or(0, |d| {
            inflated_len(flate2::read::DeflateDecoder::new(d), budget)
        });
        let used = zlib.max(deflate);
        if used > budget {
            return false;
        }
        budget -= used;
        rest = &data[end..];
    }
    true
}

/// 한도+1 바이트까지만 풀어 본 크기(오류가 나면 그때까지 푼 크기).
fn inflated_len(reader: impl Read, budget: u64) -> u64 {
    let mut limited = reader.take(budget.saturating_add(1));
    let mut sink = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        match limited.read(&mut sink) {
            Ok(0) | Err(_) => return total,
            Ok(n) => total += n as u64,
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Docx,
    Pptx,
    Xlsx,
    Hwpx,
    Pdf,
}

fn format(content_type: &str, name: &str) -> Option<Format> {
    let ty = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let ext = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match (ty.as_str(), ext.as_str()) {
        (t, _) if t.starts_with("text/") => Some(Format::Text),
        ("application/json" | "application/xml" | "application/x-yaml", _) => Some(Format::Text),
        ("application/vnd.openxmlformats-officedocument.wordprocessingml.document", _)
        | (_, "docx") => Some(Format::Docx),
        ("application/vnd.openxmlformats-officedocument.presentationml.presentation", _)
        | (_, "pptx") => Some(Format::Pptx),
        ("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", _) | (_, "xlsx") => {
            Some(Format::Xlsx)
        }
        ("application/hwp+zip" | "application/haansofthwpx", _) | (_, "hwpx") => Some(Format::Hwpx),
        ("application/pdf", _) | (_, "pdf") => Some(Format::Pdf),
        (
            _,
            "txt" | "md" | "markdown" | "csv" | "tsv" | "json" | "xml" | "yaml" | "yml" | "log",
        ) => Some(Format::Text),
        _ => None,
    }
}

/// 이 형식에서 글자를 뽑을 수 있는지.
#[must_use]
pub fn supported(content_type: &str, name: &str) -> bool {
    format(content_type, name).is_some()
}

fn clip(mut text: String) -> String {
    if let Some((cut, _)) = text.char_indices().nth(MAX_INDEX_CHARS) {
        text.truncate(cut);
    }
    text
}

/// XML에서 태그를 걷어 내고 글자만 남긴다. 문단·줄·칸 끝은 공백·줄바꿈으로 둔다.
fn xml_text(xml: &str, out: &mut String) {
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        push_unescaped(&rest[..start], out);
        let Some(end) = rest[start..].find('>') else {
            return;
        };
        let tag = &rest[start + 1..start + end];
        let name = tag
            .trim_start_matches('/')
            .split([' ', '/'])
            .next()
            .unwrap_or_default();
        let local = name.rsplit(':').next().unwrap_or(name);
        if tag.starts_with('/') && matches!(local, "p" | "tr" | "si" | "row") {
            out.push('\n');
        } else if matches!(local, "tab" | "br" | "c" | "tc") {
            out.push(' ');
        }
        rest = &rest[start + end + 1..];
    }
    push_unescaped(rest, out);
}

fn push_unescaped(text: &str, out: &mut String) {
    if !text.contains('&') {
        out.push_str(text);
        return;
    }
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let Some(semi) = rest[amp..].find(';') else {
            out.push_str(&rest[amp..]);
            return;
        };
        let entity = &rest[amp + 1..amp + semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16)
                .ok()
                .and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&rest[amp..=amp + semi]),
        }
        rest = &rest[amp + semi + 1..];
    }
    out.push_str(rest);
}

/// ZIP 안에서 이름이 `wanted`에 맞는 XML 항목들의 글자(이름 순).
fn zip_xml_text(bytes: &[u8], wanted: impl Fn(&str) -> bool) -> Option<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut names: Vec<String> = archive
        .file_names()
        .filter_map(Result::ok)
        .map(std::borrow::Cow::into_owned)
        .filter(|n| wanted(n))
        .collect();
    // slide2가 slide10보다 앞서도록 숫자 길이로 먼저 줄 세운다.
    names.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    let mut out = String::new();
    for name in names {
        let Ok(entry) = archive.by_name(&name) else {
            continue;
        };
        let mut xml = String::new();
        if entry
            .take(MAX_ENTRY_BYTES)
            .read_to_string(&mut xml)
            .is_err()
        {
            continue;
        }
        xml_text(&xml, &mut out);
        out.push('\n');
        if out.len() > MAX_INDEX_CHARS * 4 {
            break;
        }
    }
    Some(out)
}

/// 파일 내용에서 색인할 글자를 뽑는다. 지원하지 않거나 읽지 못하면 `None`.
#[must_use]
pub fn extract(bytes: &[u8], content_type: &str, name: &str) -> Option<String> {
    let text = match format(content_type, name)? {
        Format::Text => String::from_utf8_lossy(bytes).into_owned(),
        Format::Docx => zip_xml_text(bytes, |n| {
            n == "word/document.xml" || n.starts_with("word/header") || n.starts_with("word/footer")
        })?,
        Format::Pptx => zip_xml_text(bytes, |n| {
            n.starts_with("ppt/slides/slide") && n.ends_with(".xml")
        })?,
        Format::Xlsx => zip_xml_text(bytes, |n| {
            n == "xl/sharedStrings.xml"
                || (n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        })?,
        Format::Hwpx => zip_xml_text(bytes, |n| {
            n.starts_with("Contents/section") && n.ends_with(".xml")
        })?,
        // D-73: pdf-extract(MIT) → lopdf. lopdf는 압축 스트림을 크기 한도 없이 풀므로 먼저 한도 안인지 본다.
        // 읽지 못하거나 한도를 넘는 PDF는 이름으로만 찾는다.
        Format::Pdf => {
            if !pdf_within_limits(bytes) {
                return None;
            }
            pdf_extract::extract_text_from_mem(bytes).ok()?
        }
    };
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(clip(compact))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn zip_with(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut out);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, body) in entries {
            zip.start_file(*name, options).ok();
            zip.write_all(body.as_bytes()).ok();
        }
        zip.finish().ok();
        out.into_inner()
    }

    #[test]
    fn reads_plain_text_and_office_xml() {
        assert_eq!(
            extract("가 나\n다".as_bytes(), "text/plain", "a.txt").as_deref(),
            Some("가 나 다")
        );
        let docx = zip_with(&[(
            "word/document.xml",
            "<w:document><w:body><w:p><w:r><w:t>회의</w:t></w:r><w:r><w:t xml:space=\"preserve\"> 결과 &amp; 결정</w:t></w:r></w:p><w:p><w:r><w:t>둘째</w:t></w:r></w:p></w:body></w:document>",
        )]);
        assert_eq!(
            extract(&docx, "application/octet-stream", "보고서.docx").as_deref(),
            Some("회의 결과 & 결정 둘째")
        );
        let pptx = zip_with(&[
            ("ppt/slides/slide10.xml", "<p:sld><a:t>열번째</a:t></p:sld>"),
            ("ppt/slides/slide2.xml", "<p:sld><a:t>두번째</a:t></p:sld>"),
        ]);
        assert_eq!(
            extract(&pptx, "", "발표.pptx").as_deref(),
            Some("두번째 열번째")
        );
        let hwpx = zip_with(&[(
            "Contents/section0.xml",
            "<hs:sec><hp:p><hp:t>한글 문서 &#xD55C;</hp:t></hp:p></hs:sec>",
        )]);
        assert_eq!(
            extract(&hwpx, "application/hwp+zip", "x.hwpx").as_deref(),
            Some("한글 문서 한")
        );
    }

    /// 표준 글꼴(Helvetica) 한 쪽짜리 PDF. 상호 참조 표의 위치를 직접 계산한다.
    fn tiny_pdf(text: &str) -> Vec<u8> {
        pdf_with(
            "",
            format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET").as_bytes(),
        )
    }

    /// 한 쪽짜리 PDF. 본문 스트림의 사전에 `extra`(예: 압축 방식)를 더한다. 상호 참조 표의 위치를 직접 계산한다.
    fn pdf_with(extra: &str, content: &[u8]) -> Vec<u8> {
        let mut stream = format!("<< /Length {}{extra} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content);
        stream.extend_from_slice(b"\nendstream");
        let objects: [Vec<u8>; 5] = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
            stream,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    /// 압축 폭탄(D-73 리뷰): 작은 파일이 풀면 한도를 넘는 PDF는 해석기에 넘기지 않는다. 압축된 보통 PDF는 그대로 읽는다.
    #[test]
    fn pdf_flate_bombs_are_not_parsed() {
        let normal = pdf_with(
            " /Filter /FlateDecode",
            &zlib(b"BT /F1 12 Tf 72 720 Td (Compressed body text) Tj ET"),
        );
        assert_eq!(
            extract(&normal, "application/pdf", "a.pdf").as_deref(),
            Some("Compressed body text")
        );
        let zeros = vec![0u8; usize::try_from(MAX_PDF_INFLATED_BYTES).unwrap() + 4096];
        let bomb = pdf_with(" /Filter /FlateDecode", &zlib(&zeros));
        assert!(bomb.len() < 1024 * 1024, "{}", bomb.len());
        assert!(!pdf_within_limits(&bomb));
        assert!(extract(&bomb, "application/pdf", "bomb.pdf").is_none());
        // 크기를 미리 알 수 없는 LZW, 미리 풀 수 없는 암호화 PDF도 뽑지 않는다.
        assert!(!pdf_within_limits(&pdf_with(" /Filter /LZWDecode", b"x")));
        assert!(!pdf_within_limits(&pdf_with(" /Encrypt 9 0 R", b"x")));
    }

    #[test]
    fn reads_pdf_text() {
        let pdf = tiny_pdf("Rainbow outcome was significant");
        assert_eq!(
            extract(&pdf, "application/pdf", "report.pdf").as_deref(),
            Some("Rainbow outcome was significant")
        );
        assert!(supported("", "paper.PDF"));
    }

    #[test]
    fn skips_unsupported_and_broken_files() {
        assert!(extract(b"%PDF-1.4", "application/pdf", "a.pdf").is_none());
        assert!(extract(b"not a zip", "", "a.docx").is_none());
        assert!(!supported("image/png", "a.png"));
        assert!(supported("", "notes.md"));
    }
}
