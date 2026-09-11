pub fn normalize_document_path(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return None;
    }
    if s == "~" || s.starts_with("~/") {
        let home = std::env::var("HOME").ok()?;
        if s == "~" {
            s = home;
        } else {
            s = format!("{home}{}", &s[1..]);
        }
    }
    if s.to_ascii_lowercase().starts_with("file:") {
        s = file_url_to_path(&s)?;
    }
    if !s.starts_with('/') {
        return None;
    }
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    Some(s)
}

fn file_url_to_path(url: &str) -> Option<String> {
    let rest = url.split_once(':')?.1;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let path_part = if rest.starts_with('/') {
        rest
    } else {
        let slash = rest.find('/')?;
        let host = &rest[..slash];
        if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
            return None;
        }
        &rest[slash..]
    };
    percent_decode(path_part)
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let h = hex_val(bytes[i + 1])?;
            let l = hex_val(bytes[i + 2])?;
            out.push((h << 4) | l);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expands_home_before_absolute_check() {
        let home = std::env::var("HOME").expect("HOME");
        assert_eq!(
            normalize_document_path("~/Projects/HDP"),
            Some(format!("{home}/Projects/HDP"))
        );
    }

    #[test]
    fn file_url_percent_decodes() {
        assert_eq!(
            normalize_document_path("file:///Users/me/My%20Project/a.py"),
            Some("/Users/me/My Project/a.py".into())
        );
    }

    #[test]
    fn file_url_strips_trailing_slash() {
        assert_eq!(
            normalize_document_path("file:///Users/me/HDP/"),
            Some("/Users/me/HDP".into())
        );
    }

    #[test]
    fn window_title_is_not_a_path() {
        assert_eq!(normalize_document_path("train.py — HDP"), None);
    }

    #[test]
    fn relative_rejected() {
        assert_eq!(normalize_document_path("train.py"), None);
    }
}
