//! The line breaks a profile file is written with. The writers break lines
//! the way the files' own tools do on Windows; a file some other tool wrote
//! with bare line feeds keeps them when it is saved back, so saving a file
//! that was not edited leaves it exactly as it was.

/// `written` with bare line feeds when the file it replaces, `existing`,
/// has line feeds and no carriage return before any of them; otherwise as
/// it is.
#[must_use]
pub fn match_existing(written: &str, existing: Option<&str>) -> String {
    match existing {
        Some(existing) if existing.contains('\n') && !existing.contains("\r\n") => {
            written.replace("\r\n", "\n")
        }
        _ => written.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_with_bare_line_feeds_keeps_them() {
        assert_eq!(match_existing("a\r\nb\r\n", Some("x\ny\n")), "a\nb\n");
        assert_eq!(match_existing("a\r\nb\r\n", Some("x\r\ny\n")), "a\r\nb\r\n");
        assert_eq!(match_existing("a\r\nb\r\n", Some("one line")), "a\r\nb\r\n");
        assert_eq!(match_existing("a\r\nb\r\n", None), "a\r\nb\r\n");
    }
}
