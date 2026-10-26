//! Hyphenation pattern files installed by the user: LibreOffice/Hyphen `hyph_<locale>.dic` files
//! (`hyph_sr_RS.dic`, `hyph_sh_RS.dic`, `hyph_de_DE.dic`) in a folder or its subfolders (one per
//! locale, as InDesign's Hunspell folder keeps them), read once at start-up. Their patterns come
//! before the bundled ones for text in that language
//! (see [`designcraft_compose::hyphen::install_patterns`]).

use std::path::{Path, PathBuf};

use designcraft_compose::hyphen;

/// Pattern files larger than this are skipped (the largest LibreOffice ones are under 1 MB).
const MAX_FILE_BYTES: u64 = 8 << 20;
/// At most this many files are read from the folder.
const MAX_FILES: usize = 64;

/// The locale of a pattern file name: `hyph_sr_RS.dic` → `sr_RS`.
fn locale_of(name: &str) -> Option<&str> {
    let locale = name.strip_prefix("hyph_")?.strip_suffix(".dic")?;
    (!locale.is_empty()).then_some(locale)
}

/// The files in `dir` and in its subfolders (one level), by name then path. A missing folder has
/// none. Names that aren't UTF-8 are left out.
pub(crate) fn dictionary_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let list = |d: &Path| -> Vec<(String, PathBuf, bool)> {
        let Ok(entries) = std::fs::read_dir(d) else { return Vec::new() };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path(), e.file_type().is_ok_and(|t| t.is_dir()))))
            .collect()
    };
    let mut files = Vec::new();
    for (name, path, is_dir) in list(dir) {
        if is_dir {
            files.extend(list(&path).into_iter().filter(|f| !f.2).map(|(n, p, _)| (n, p)));
        } else {
            files.push((name, path));
        }
    }
    files.sort();
    files
}

/// Install the `hyph_*.dic` files in `dir` and its subfolders, in name order. A missing folder
/// installs nothing. Returns one line per file for the log: what was installed, or why not.
pub fn install_dir(dir: &Path) -> Vec<String> {
    let mut files: Vec<(String, PathBuf)> = dictionary_files(dir).into_iter().filter(|(name, _)| locale_of(name).is_some()).collect();
    let mut report = Vec::new();
    if files.len() > MAX_FILES {
        report.push(format!("hyphenation: only the first {MAX_FILES} of {} pattern files are read", files.len()));
        files.truncate(MAX_FILES);
    }
    for (name, path) in files {
        let Some(locale) = locale_of(&name) else { continue };
        let line = match std::fs::metadata(&path) {
            Ok(m) if !m.is_file() => continue,
            Ok(m) if m.len() > MAX_FILE_BYTES => format!("hyphenation: {name} skipped: larger than {} MB", MAX_FILE_BYTES >> 20),
            Err(e) => format!("hyphenation: {name}: {e}"),
            Ok(_) => match std::fs::read(&path) {
                Err(e) => format!("hyphenation: {name}: {e}"),
                Ok(bytes) => match String::from_utf8(bytes) {
                    Err(_) => format!("hyphenation: {name} skipped: not UTF-8"),
                    Ok(text) => match hyphen::install_patterns(locale, &text) {
                        Ok((n, 0)) => format!("hyphenation: {name}: {n} patterns for {locale}"),
                        Ok((n, skipped)) => format!("hyphenation: {name}: {n} patterns for {locale} ({skipped} not supported)"),
                        Err(e) => format!("hyphenation: {name} skipped: {e}"),
                    },
                },
            },
        };
        report.push(line);
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_file_names() {
        assert_eq!(locale_of("hyph_sr_RS.dic"), Some("sr_RS"));
        assert_eq!(locale_of("hyph_sh_RS.dic"), Some("sh_RS"));
        assert_eq!(locale_of("hyph_.dic"), None);
        assert_eq!(locale_of("sr_RS.dic"), None, "a spelling dictionary");
        assert_eq!(locale_of("hyph_sr_RS.dic.bak"), None);
    }

    #[test]
    fn installs_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        // A language no other test installs or hyphenates: installing is process-wide.
        // A locale folder, as InDesign's Hunspell folder has them.
        std::fs::create_dir(dir.path().join("tpi")).unwrap();
        std::fs::write(dir.path().join("tpi/hyph_tpi.dic"), "UTF-8\nLEFTHYPHENMIN 2\n1pa\n1ta\n").unwrap();
        std::fs::write(dir.path().join("hyph_tpi_PG.dic"), "ISO8859-1\n1pa\n").unwrap();
        std::fs::write(dir.path().join("readme.txt"), "not a pattern file").unwrap();
        let report = install_dir(dir.path());
        assert_eq!(report.len(), 2, "{report:?}");
        assert!(report[0].contains("2 patterns for tpi"), "{report:?}");
        assert!(report[1].contains("skipped"), "{report:?}");
        let lang = hyphen::Lang::for_language("tpi").unwrap();
        assert_eq!(hyphen::hyphen_points_in("papatapa", &hyphen::Limits::default(), lang), vec![2, 4, 6]);
        assert!(install_dir(&dir.path().join("missing")).is_empty());
    }
}
