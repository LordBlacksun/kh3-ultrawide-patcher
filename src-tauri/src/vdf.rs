//! Minimal, dependency-free Valve KeyValues (VDF) reader — just enough to pull
//! library paths out of `libraryfolders.vdf` and `installdir` out of an
//! `appmanifest_*.acf`. Handles modern nested and legacy flat formats, escaped
//! backslashes, CRLF, and a leading BOM.

/// Flatten a VDF document into its ordered list of quoted strings (keys and
/// values interleaved; unquoted braces/whitespace are ignored).
pub fn tokenize(s: &str) -> Vec<String> {
    let mut toks = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == '"' {
            chars.next(); // opening quote
            let mut val = String::new();
            while let Some(c2) = chars.next() {
                if c2 == '\\' {
                    if let Some(n) = chars.next() {
                        match n {
                            '\\' => val.push('\\'),
                            '"' => val.push('"'),
                            'n' => val.push('\n'),
                            't' => val.push('\t'),
                            other => val.push(other),
                        }
                    }
                } else if c2 == '"' {
                    break;
                } else {
                    val.push(c2);
                }
            }
            toks.push(val);
        } else {
            chars.next();
        }
    }
    toks
}

/// All library paths from a `libraryfolders.vdf` (modern `"path" "X"` blocks and
/// legacy `"1" "X"` entries).
pub fn parse_library_paths(vdf: &str) -> Vec<String> {
    let toks = tokenize(vdf);
    let mut paths = Vec::new();
    let mut i = 0;
    while i + 1 < toks.len() {
        let key = &toks[i];
        let val = &toks[i + 1];
        if key.eq_ignore_ascii_case("path") {
            paths.push(val.clone());
            i += 2;
            continue;
        }
        // Legacy: numeric key whose value looks like a filesystem path.
        let looks_like_path = val.contains(':') || val.contains('\\') || val.contains('/');
        if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) && looks_like_path {
            paths.push(val.clone());
            i += 2;
            continue;
        }
        i += 1;
    }
    paths.sort();
    paths.dedup();
    paths
}

/// Read the `installdir` value from an `appmanifest_*.acf`.
pub fn read_installdir(acf: &str) -> Option<String> {
    let toks = tokenize(acf);
    let mut i = 0;
    while i + 1 < toks.len() {
        if toks[i].eq_ignore_ascii_case("installdir") {
            return Some(toks[i + 1].clone());
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_libraryfolders() {
        let vdf = r#"
"libraryfolders"
{
    "0"
    {
        "path"        "C:\\Program Files (x86)\\Steam"
        "apps"        { "228980" "123" }
    }
    "1"
    {
        "path"        "D:\\Games\\SteamLibrary"
        "apps"        { "2552450" "456" }
    }
}
"#;
        let paths = parse_library_paths(vdf);
        assert!(paths.iter().any(|p| p == r"C:\Program Files (x86)\Steam"));
        assert!(paths.iter().any(|p| p == r"D:\Games\SteamLibrary"));
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn legacy_libraryfolders() {
        let vdf = r#"
"LibraryFolders"
{
    "TimeNextStatsReport"  "1700000000"
    "ContentStatsID"       "1234567890"
    "1"  "D:\\Games\\SteamLibrary"
    "2"  "F:\\Steam"
}
"#;
        let paths = parse_library_paths(vdf);
        assert!(paths.iter().any(|p| p == r"D:\Games\SteamLibrary"));
        assert!(paths.iter().any(|p| p == r"F:\Steam"));
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn installdir_from_acf() {
        let acf = r#"
"AppState"
{
    "appid"      "2552450"
    "name"       "KINGDOM HEARTS III"
    "installdir" "KINGDOM HEARTS III"
}
"#;
        assert_eq!(read_installdir(acf).as_deref(), Some("KINGDOM HEARTS III"));
    }
}
