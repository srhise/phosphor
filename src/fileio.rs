//! Reading and writing documents.
//!
//! Two normalizations happen on load and are deliberately lossy: line
//! endings collapse to LF (the original style is remembered and restored
//! on save), and tabs expand to spaces. Expanding tabs at the boundary is
//! what lets every column calculation downstream be exact.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const TAB_STOP: usize = 8;

pub struct Loaded {
    pub text: String,
    /// Whether the file used CRLF, so saving can put it back.
    pub crlf: bool,
}

/// Replace tabs with spaces up to the next multiple of `stop`, counting
/// from the start of each line.
pub fn expand_tabs(s: &str, stop: usize) -> String {
    let stop = stop.max(1);
    let mut out = String::with_capacity(s.len());
    let mut col = 0usize;
    for c in s.chars() {
        match c {
            '\t' => {
                let spaces = stop - (col % stop);
                out.extend(std::iter::repeat_n(' ', spaces));
                col += spaces;
            }
            '\n' => {
                out.push('\n');
                col = 0;
            }
            other => {
                out.push(other);
                col += 1;
            }
        }
    }
    out
}

pub fn load(path: &Path) -> io::Result<Loaded> {
    let bytes = fs::read(path)?;
    // Damaged encoding should still open; a writer would rather see the
    // file with a few replacement characters than be refused entry.
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    let crlf = raw.contains("\r\n");
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    Ok(Loaded { text: expand_tabs(&normalized, TAB_STOP), crlf })
}

/// Write atomically: a temporary file in the destination directory, then
/// a rename over the target. A failed write never destroys the previous
/// version.
pub fn save(path: &Path, text: &str, crlf: bool) -> io::Result<()> {
    let body = if crlf { text.replace('\n', "\r\n") } else { text.to_string() };

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = temp_sibling(path);

    {
        let mut f = File::create(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    // Fsync the directory so the rename itself is durable.
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// A temporary path beside the target, so the rename stays on one volume.
fn temp_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".to_string());
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!(".{name}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn expands_tabs_to_the_next_stop() {
        assert_eq!(expand_tabs("\tx", 8), "        x");
        assert_eq!(expand_tabs("a\tb", 8), "a       b");
        assert_eq!(expand_tabs("abcdefg\th", 8), "abcdefg h");
        assert_eq!(expand_tabs("abcdefgh\ti", 8), "abcdefgh        i");
    }

    #[test]
    fn tab_stops_restart_on_each_line() {
        assert_eq!(expand_tabs("abc\n\tx", 8), "abc\n        x");
    }

    #[test]
    fn text_without_tabs_is_unchanged() {
        assert_eq!(expand_tabs("plain text", 8), "plain text");
    }

    #[test]
    fn loads_a_plain_file() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("a.txt");
        fs::write(&p, "hello\nworld").expect("write");
        let loaded = load(&p).expect("load");
        assert_eq!(loaded.text, "hello\nworld");
        assert!(!loaded.crlf);
    }

    #[test]
    fn normalizes_crlf_and_remembers_it() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("dos.txt");
        fs::write(&p, "a\r\nb\r\n").expect("write");
        let loaded = load(&p).expect("load");
        assert_eq!(loaded.text, "a\nb\n", "the buffer only ever holds LF");
        assert!(loaded.crlf, "but we remember to write CRLF back");
    }

    #[test]
    fn normalizes_lone_carriage_returns() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("mac.txt");
        fs::write(&p, "a\rb").expect("write");
        assert_eq!(load(&p).expect("load").text, "a\nb");
    }

    #[test]
    fn expands_tabs_on_load() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("t.txt");
        fs::write(&p, "a\tb").expect("write");
        assert_eq!(load(&p).expect("load").text, "a       b");
    }

    #[test]
    fn invalid_utf8_is_replaced_rather_than_refused() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("bad.txt");
        fs::write(&p, [0x61, 0xFF, 0x62]).expect("write");
        let loaded = load(&p).expect("a damaged file still opens");
        assert!(loaded.text.starts_with('a'));
        assert!(loaded.text.ends_with('b'));
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let dir = tempdir().expect("tempdir");
        assert!(load(&dir.path().join("nope.txt")).is_err());
    }

    #[test]
    fn saves_and_round_trips() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("out.txt");
        save(&p, "hello\nthere", false).expect("save");
        assert_eq!(fs::read_to_string(&p).expect("read"), "hello\nthere");
        assert_eq!(load(&p).expect("load").text, "hello\nthere");
    }

    #[test]
    fn restores_crlf_line_endings_on_save() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("dos.txt");
        save(&p, "a\nb", true).expect("save");
        assert_eq!(fs::read_to_string(&p).expect("read"), "a\r\nb");
    }

    #[test]
    fn saving_leaves_no_temporary_files_behind() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("out.txt");
        save(&p, "x", false).expect("save");
        let names: Vec<_> = fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["out.txt".to_string()]);
    }

    #[test]
    fn overwriting_replaces_the_previous_contents_entirely() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("out.txt");
        save(&p, "a longer first version", false).expect("save");
        save(&p, "short", false).expect("save");
        assert_eq!(fs::read_to_string(&p).expect("read"), "short");
    }

    #[test]
    fn saving_into_a_missing_directory_is_an_error_not_a_panic() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("no_such_dir").join("out.txt");
        assert!(save(&p, "x", false).is_err());
    }
}
