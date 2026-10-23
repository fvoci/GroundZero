//! Narrow normalization for the full-line SQL comments edited during consolidation.
//! Executable bytes, inline comments and whitespace remain exact. Quoted SQL data is never stripped.

use anyhow::{Context, Result};

enum Quote {
    None,
    Single { escapes: bool },
    Double,
    Dollar(Vec<u8>),
    Block(usize),
}

fn dollar_tag(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.first() != Some(&b'$') {
        return None;
    }
    let end = bytes[1..].iter().position(|byte| *byte == b'$')? + 1;
    let name = &bytes[1..end];
    if name.is_empty()
        || ((name[0].is_ascii_alphabetic() || name[0] >= 0x80 || name[0] == b'_')
            && name
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte >= 0x80 || *byte == b'_'))
    {
        Some(&bytes[..=end])
    } else {
        None
    }
}

fn body_without_comment_lines(body: &str) -> String {
    let mut quote = Quote::None;
    let mut result = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        if matches!(quote, Quote::None) && line.trim_start().starts_with("--") {
            continue;
        }
        result.push_str(line);
        let bytes = line.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            let rest = &bytes[at..];
            match &mut quote {
                Quote::None => {
                    if rest.starts_with(b"--") {
                        break;
                    } else if rest.starts_with(b"/*") {
                        quote = Quote::Block(1);
                        at += 2;
                    } else if rest[0] == b'\'' {
                        let escapes = at > 0
                            && matches!(bytes[at - 1], b'e' | b'E')
                            && (at == 1
                                || !(bytes[at - 2].is_ascii_alphanumeric()
                                    || bytes[at - 2] >= 0x80
                                    || bytes[at - 2] == b'_'));
                        quote = Quote::Single { escapes };
                        at += 1;
                    } else if rest[0] == b'"' {
                        quote = Quote::Double;
                        at += 1;
                    } else if let Some(tag) = dollar_tag(rest) {
                        quote = Quote::Dollar(tag.to_vec());
                        at += tag.len();
                    } else {
                        at += 1;
                    }
                }
                Quote::Single { escapes } => {
                    if (*escapes && rest[0] == b'\\') || rest.starts_with(b"''") {
                        at += 2;
                    } else {
                        if rest[0] == b'\'' {
                            quote = Quote::None;
                        }
                        at += 1;
                    }
                }
                Quote::Double => {
                    if rest.starts_with(b"\"\"") {
                        at += 2;
                    } else {
                        if rest[0] == b'"' {
                            quote = Quote::None;
                        }
                        at += 1;
                    }
                }
                Quote::Dollar(tag) => {
                    if rest.starts_with(tag) {
                        at += tag.len();
                        quote = Quote::None;
                    } else {
                        at += 1;
                    }
                }
                Quote::Block(depth) => {
                    if rest.starts_with(b"/*") {
                        *depth += 1;
                        at += 2;
                    } else if rest.starts_with(b"*/") {
                        *depth -= 1;
                        at += 2;
                        if *depth == 0 {
                            quote = Quote::None;
                        }
                    } else {
                        at += 1;
                    }
                }
            }
        }
    }
    result
}

/// PostgreSQL's deparsed function header and footer remain byte-identical, including attributes.
pub(super) fn function(definition: &str) -> Result<String> {
    let mut offset = 0;
    for line in definition.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("AS ")
            && let Some(tag) = dollar_tag(rest.as_bytes())
        {
            let start = offset + line.len() - trimmed.len() + 3 + tag.len();
            let tag = std::str::from_utf8(tag)?;
            let end = start
                + definition[start..]
                    .find(tag)
                    .context("unterminated function body")?;
            return Ok(format!(
                "{}{}{}",
                &definition[..start],
                body_without_comment_lines(&definition[start..end]),
                &definition[end..]
            ));
        }
        offset += line.len();
    }
    // SQL-standard BEGIN ATOMIC bodies are outside this consolidation's normalization scope.
    Ok(definition.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_full_comment_lines_outside_sql_literals_are_removed() {
        let before = "\n-- old\nSELECT '-- text', 'first\n-- string data\nlast', E'quote\\\'\n-- escaped string data\nlast', \"quoted\n-- identifier\n\", $inner$\n-- dollar data\n$inner$; -- inline stays\n/* nested /* -- block */\n-- block data\n*/\n  -- last comment\n";
        let after = "\nSELECT '-- text', 'first\n-- string data\nlast', E'quote\\\'\n-- escaped string data\nlast', \"quoted\n-- identifier\n\", $inner$\n-- dollar data\n$inner$; -- inline stays\n/* nested /* -- block */\n-- block data\n*/\n";
        assert_eq!(body_without_comment_lines(before), after);
    }

    #[test]
    fn function_attributes_code_whitespace_and_literal_changes_still_differ() -> Result<()> {
        let sql = "CREATE FUNCTION f() RETURNS text\n LANGUAGE sql SECURITY DEFINER\n SET search_path TO 'fvoci', 'pg_temp'\n AS $function$\n-- rationale\nSELECT 'private';\n$function$\n";
        let changed_comment = sql.replace("-- rationale\n", "  -- current reason\n-- exception\n");
        assert_eq!(function(sql)?, function(&changed_comment)?);
        for changed in [
            sql.replace("SECURITY DEFINER", "SECURITY INVOKER"),
            sql.replace("'private'", "'public'"),
            sql.replace("SELECT ", "SELECT  "),
            sql.replace("'fvoci', 'pg_temp'", "'public'"),
        ] {
            assert_ne!(function(sql)?, function(&changed)?);
        }
        Ok(())
    }

    #[test]
    fn non_ascii_dollar_tags_preserve_comment_shaped_data() {
        let sql = "SELECT $본문$\n-- value A\n$본문$;\n-- remove this comment\n";
        let normalized = body_without_comment_lines(sql);
        assert_eq!(normalized, "SELECT $본문$\n-- value A\n$본문$;\n");
        assert_ne!(
            normalized,
            body_without_comment_lines(&sql.replace("value A", "value B"))
        );
    }
}
