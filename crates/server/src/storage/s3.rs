//! S3 호환 저장소(Cloudflare R2, 테스트는 SeaweedFS). 기존 `reqwest`로 요청하고 AWS 서명 v4만 직접 만든다.
//! 경로 방식 주소(`{endpoint}/{bucket}/{key}`)를 쓴다. 조각은 본문의 SHA-256으로 서명해 전송 중 변조·손상을
//! 저장소가 거부하게 한다.

use bytes::Bytes;
use futures_util::StreamExt;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use url::Url;

use super::{ByteStream, Part};
use crate::config::{S3Config, Secret};
use crate::error::AppError;

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Debug, Clone)]
pub struct S3Store {
    endpoint: Url,
    region: String,
    bucket: String,
    access_key: String,
    secret_key: Secret,
    http: reqwest::Client,
}

fn http_error(err: reqwest::Error) -> AppError {
    AppError::Storage(format!("s3: {err}"))
}

/// AWS 규칙의 URI 인코딩. 경로에서는 `/`를 그대로 둔다.
fn encode(value: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b'/' if keep_slash => out.push('/'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    // HMAC는 어떤 길이의 키도 받으므로 실패하지 않는다.
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap_or_else(|_| unreachable!());
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn amz_date(now: OffsetDateTime) -> (String, String) {
    let date = format!(
        "{:04}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    );
    let stamp = format!(
        "{date}T{:02}{:02}{:02}Z",
        now.hour(),
        now.minute(),
        now.second()
    );
    (date, stamp)
}

/// XML에서 첫 `<name>…</name>`의 내용.
fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(&xml[start..end])
}

/// XML에서 모든 `<name>…</name>`의 내용.
fn blocks<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let start = i + open.len();
        let Some(len) = rest[start..].find(&close) else {
            break;
        };
        out.push(&rest[start..start + len]);
        rest = &rest[start + len + close.len()..];
    }
    out
}

fn unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#x22;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

impl S3Store {
    #[must_use]
    pub fn new(config: &S3Config) -> Self {
        Self {
            endpoint: config.endpoint.clone(),
            region: config.region.clone(),
            bucket: config.bucket.clone(),
            access_key: config.access_key.clone(),
            secret_key: config.secret_key.clone(),
            http: reqwest::Client::builder()
                // 큰 파일을 흘려보내는 동안 끊지 않도록 전체 시간 대신 연결·읽기 대기 시간만 제한한다.
                .connect_timeout(std::time::Duration::from_secs(10))
                .read_timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
        }
    }

    /// 서명한 요청을 만든다. `query`는 인코딩 전 값.
    fn signed(
        &self,
        method: reqwest::Method,
        key: &str,
        query: &[(&str, String)],
        payload_sha256: &str,
    ) -> reqwest::RequestBuilder {
        let path = format!(
            "{}/{}/{}",
            self.endpoint.path().trim_end_matches('/'),
            encode(&self.bucket, false),
            encode(key, true)
        );
        let mut pairs: Vec<(String, String)> = query
            .iter()
            .map(|(k, v)| (encode(k, false), encode(v, false)))
            .collect();
        pairs.sort();
        let canonical_query = pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let host = match self.endpoint.port() {
            Some(port) => format!("{}:{port}", self.endpoint.host_str().unwrap_or_default()),
            None => self.endpoint.host_str().unwrap_or_default().to_owned(),
        };
        let (date, stamp) = amz_date(OffsetDateTime::now_utc());
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "{method}\n{path}\n{canonical_query}\nhost:{host}\nx-amz-content-sha256:{payload_sha256}\nx-amz-date:{stamp}\n\n{signed_headers}\n{payload_sha256}"
        );
        let scope = format!("{date}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical_request.as_bytes()))
        );
        let mut signing_key = hmac(
            format!("AWS4{}", self.secret_key.expose()).as_bytes(),
            &date,
        );
        for part in [self.region.as_str(), "s3", "aws4_request"] {
            signing_key = hmac(&signing_key, part);
        }
        let signature = hex::encode(hmac(&signing_key, &string_to_sign));
        let mut url = self.endpoint.clone();
        url.set_path(&path);
        url.set_query((!canonical_query.is_empty()).then_some(canonical_query.as_str()));
        self.http
            .request(method, url)
            .header("x-amz-date", stamp)
            .header("x-amz-content-sha256", payload_sha256)
            .header(
                "authorization",
                format!(
                    "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
                    self.access_key
                ),
            )
    }

    /// 보내고 성공이 아니면 S3 오류 코드를 담아 실패한다.
    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, AppError> {
        let response = request.send().await.map_err(http_error)?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::NOT_FOUND
            && matches!(
                tag(&body, "Code"),
                Some("NoSuchKey" | "NoSuchUpload") | None
            )
        {
            return Err(AppError::NotFound);
        }
        Err(AppError::Storage(format!(
            "s3 {status}: {}",
            tag(&body, "Code").unwrap_or("unknown")
        )))
    }

    pub(super) async fn begin(&self, key: &str) -> Result<String, AppError> {
        let response = self
            .send(self.signed(
                reqwest::Method::POST,
                key,
                &[("uploads", String::new())],
                EMPTY_SHA256,
            ))
            .await?;
        let body = response.text().await.map_err(http_error)?;
        tag(&body, "UploadId")
            .map(unescape)
            .ok_or_else(|| AppError::Storage("s3: no upload id".into()))
    }

    pub(super) async fn put_part(
        &self,
        key: &str,
        upload_id: &str,
        number: u32,
        body: Bytes,
    ) -> Result<String, AppError> {
        let hash = hex::encode(Sha256::digest(&body));
        let query = [
            ("partNumber", number.to_string()),
            ("uploadId", upload_id.to_owned()),
        ];
        let response = self
            .send(
                self.signed(reqwest::Method::PUT, key, &query, &hash)
                    .header("content-length", body.len())
                    .body(body),
            )
            .await?;
        response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| AppError::Storage("s3: no etag".into()))
    }

    pub(super) async fn list_parts(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<Vec<Part>, AppError> {
        let mut parts = Vec::new();
        let mut marker: Option<String> = None;
        loop {
            let mut query = vec![("uploadId", upload_id.to_owned())];
            if let Some(m) = &marker {
                query.push(("part-number-marker", m.clone()));
            }
            let response = self
                .send(self.signed(reqwest::Method::GET, key, &query, EMPTY_SHA256))
                .await?;
            let body = response.text().await.map_err(http_error)?;
            for block in blocks(&body, "Part") {
                let number = tag(block, "PartNumber").and_then(|n| n.parse().ok());
                let size = tag(block, "Size").and_then(|n| n.parse().ok());
                if let (Some(number), Some(size), Some(etag)) = (number, size, tag(block, "ETag")) {
                    parts.push(Part {
                        number,
                        etag: unescape(etag),
                        size,
                    });
                }
            }
            if tag(&body, "IsTruncated") != Some("true") {
                break;
            }
            match tag(&body, "NextPartNumberMarker") {
                Some(next) if Some(next) != marker.as_deref() => marker = Some(next.to_owned()),
                _ => break,
            }
        }
        // 같은 번호를 다시 올리면 저장소에 따라(SeaweedFS) 두 줄이 나온다. 번호마다 하나만 남긴다. 마치기에는
        // 이 목록이 아니라 조각을 넣을 때 받은 표시를 쓴다.
        parts.sort_by_key(|p| p.number);
        parts.dedup_by_key(|p| p.number);
        Ok(parts)
    }

    pub(super) async fn complete(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[Part],
    ) -> Result<(), AppError> {
        let mut xml = String::from("<CompleteMultipartUpload>");
        for part in parts {
            xml.push_str(&format!(
                "<Part><PartNumber>{}</PartNumber><ETag>{}</ETag></Part>",
                part.number,
                part.etag.replace('&', "&amp;").replace('<', "&lt;")
            ));
        }
        xml.push_str("</CompleteMultipartUpload>");
        let hash = hex::encode(Sha256::digest(xml.as_bytes()));
        let response = self
            .send(
                self.signed(
                    reqwest::Method::POST,
                    key,
                    &[("uploadId", upload_id.to_owned())],
                    &hash,
                )
                .header("content-type", "application/xml")
                .body(xml),
            )
            .await?;
        // 성공 응답 안에 오류가 올 수 있다(S3 규칙).
        let body = response.text().await.map_err(http_error)?;
        if body.contains("<Error>") {
            return Err(AppError::Storage(format!(
                "s3 complete: {}",
                tag(&body, "Code").unwrap_or("unknown")
            )));
        }
        Ok(())
    }

    pub(super) async fn abort(&self, key: &str, upload_id: &str) -> Result<(), AppError> {
        match self
            .send(self.signed(
                reqwest::Method::DELETE,
                key,
                &[("uploadId", upload_id.to_owned())],
                EMPTY_SHA256,
            ))
            .await
        {
            Ok(_) | Err(AppError::NotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub(super) async fn get(&self, key: &str) -> Result<ByteStream, AppError> {
        let response = self
            .send(self.signed(reqwest::Method::GET, key, &[], EMPTY_SHA256))
            .await?;
        Ok(Box::pin(
            response
                .bytes_stream()
                .map(|chunk| chunk.map_err(std::io::Error::other)),
        ))
    }

    pub(super) async fn delete(&self, key: &str) -> Result<(), AppError> {
        match self
            .send(self.signed(reqwest::Method::DELETE, key, &[], EMPTY_SHA256))
            .await
        {
            Ok(_) | Err(AppError::NotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_aws() {
        assert_eq!(encode("a b/c~d", true), "a%20b/c~d");
        assert_eq!(encode("a/b", false), "a%2Fb");
        assert_eq!(encode("한", false), "%ED%95%9C");
    }

    #[test]
    fn reads_parts_from_xml() {
        let xml = "<ListPartsResult><IsTruncated>false</IsTruncated>\
            <Part><PartNumber>2</PartNumber><ETag>&quot;b&quot;</ETag><Size>3</Size></Part>\
            <Part><PartNumber>1</PartNumber><ETag>&quot;a&quot;</ETag><Size>5</Size></Part></ListPartsResult>";
        let found: Vec<_> = blocks(xml, "Part")
            .into_iter()
            .map(|b| (tag(b, "PartNumber"), tag(b, "ETag").map(unescape)))
            .collect();
        assert_eq!(found[0], (Some("2"), Some("\"b\"".to_owned())));
        assert_eq!(found.len(), 2);
    }

    /// AWS 문서의 서명 키 예시(GET 요청 예, 2013-05-24, us-east-1)와 같은 값.
    #[test]
    fn derives_the_documented_signing_key() {
        let mut key = hmac(b"AWS4wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY", "20130524");
        for part in ["us-east-1", "s3", "aws4_request"] {
            key = hmac(&key, part);
        }
        let canonical = "GET\n/test.txt\n\nhost:examplebucket.s3.amazonaws.com\nrange:bytes=0-9\nx-amz-content-sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nx-amz-date:20130524T000000Z\n\nhost;range;x-amz-content-sha256;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let sts = format!(
            "AWS4-HMAC-SHA256\n20130524T000000Z\n20130524/us-east-1/s3/aws4_request\n{}",
            hex::encode(Sha256::digest(canonical.as_bytes()))
        );
        assert_eq!(
            hex::encode(hmac(&key, &sts)),
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }
}
