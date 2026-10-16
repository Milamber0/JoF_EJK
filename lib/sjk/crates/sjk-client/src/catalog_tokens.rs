//! Tokenization shared by the legacy catalogue adapters.

pub(crate) fn tokenize(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            index += 2;
            while index + 1 < bytes.len() && !bytes[index..].starts_with(b"*/") {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        if matches!(bytes[index], b'{' | b'}') {
            tokens.push(char::from(bytes[index]).to_string());
            index += 1;
            continue;
        }
        if bytes[index] == b'"' {
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index] != b'"' {
                index += 1;
            }
            tokens.push(String::from_utf8_lossy(&bytes[start..index]).into_owned());
            index += usize::from(index < bytes.len());
            continue;
        }
        let start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && !matches!(bytes[index], b'{' | b'}')
            && !bytes[index..].starts_with(b"//")
            && !bytes[index..].starts_with(b"/*")
        {
            index += 1;
        }
        if start != index {
            tokens.push(String::from_utf8_lossy(&bytes[start..index]).into_owned());
        }
    }
    tokens
}
