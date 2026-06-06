/// Resolve a marker title: blank/whitespace becomes `Untitled Song {track}`.
/// The resolved title is used everywhere — MP3 title tag and filename.
pub fn resolve_title(raw: &str, track: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        format!("Untitled Song {track}")
    } else {
        trimmed.to_string()
    }
}

/// Make a title safe as a filename on every OS we ship to: replace
/// `/ \ : * ? " < > |` and control chars with `_`, collapse runs of `_`,
/// trim leading dots and trailing dots/spaces.
pub fn sanitize_title(title: &str) -> String {
    const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let mut out = String::with_capacity(title.len());
    let mut last_was_underscore = false;
    for c in title.chars() {
        if FORBIDDEN.contains(&c) || c.is_control() {
            if !last_was_underscore {
                out.push('_');
                last_was_underscore = true;
            }
        } else {
            out.push(c);
            last_was_underscore = c == '_';
        }
    }
    out.trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_string()
}

/// Build `NN - Title.mp3`. `NN` is zero-padded to max(2, digits(total)).
/// A title that sanitizes to nothing falls back to `Untitled Song {track}`.
pub fn filename_for(track: usize, total: usize, resolved_title: &str) -> String {
    let width = std::cmp::max(2, total.to_string().len());
    let safe = sanitize_title(resolved_title);
    let name = if safe.is_empty() {
        format!("Untitled Song {track}")
    } else {
        safe
    };
    format!("{track:0width$} - {name}.mp3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_titles_resolve_to_untitled() {
        assert_eq!(resolve_title("", 3), "Untitled Song 3");
        assert_eq!(resolve_title("   ", 12), "Untitled Song 12");
        assert_eq!(resolve_title("Slow Blues", 1), "Slow Blues");
    }

    #[test]
    fn sanitizes_forbidden_characters() {
        assert_eq!(sanitize_title("AC/DC Jam"), "AC_DC Jam");
        assert_eq!(sanitize_title("a\\b:c*d?e\"f<g>h|i"), "a_b_c_d_e_f_g_h_i");
        assert_eq!(sanitize_title("tab\there"), "tab_here");
    }

    #[test]
    fn collapses_runs_and_trims_dots_and_spaces() {
        assert_eq!(sanitize_title("a//b"), "a_b");
        assert_eq!(sanitize_title("ends with dots..."), "ends with dots");
        assert_eq!(sanitize_title(".hidden"), "hidden");
        assert_eq!(sanitize_title("trailing space "), "trailing space");
    }

    #[test]
    fn filenames_are_padded_and_numbered() {
        assert_eq!(filename_for(1, 12, "Opening Jam"), "01 - Opening Jam.mp3");
        assert_eq!(filename_for(7, 120, "X"), "007 - X.mp3");
    }

    #[test]
    fn title_that_sanitizes_to_nothing_falls_back_for_filename() {
        assert_eq!(filename_for(2, 9, "..."), "02 - Untitled Song 2.mp3");
    }

    #[test]
    fn identical_titles_still_make_distinct_filenames() {
        // the track prefix guarantees within-run uniqueness by construction
        assert_ne!(filename_for(3, 10, "A_B"), filename_for(7, 10, "A_B"));
    }
}
