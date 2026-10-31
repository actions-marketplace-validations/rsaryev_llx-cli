//! Unified hunks of what a patch did to a file, with bounded output. The lines come from the
//! patch's own edits (`patch/apply.rs`), so nothing here works out what changed.

const CONTEXT: usize = 3;
const MAX_LINES: usize = 200;
const MAX_BYTES: usize = 20_000;
const TRUNCATED: &str = "[Diff truncated: remaining lines/changes omitted.]\n";

/// A line of the diff: kept (` `), removed (`-`) or added (`+`), its text and its ending (none for
/// a file's last line without one).
pub(super) type Line<'a> = (char, &'a str, &'a str);

/// `lines` as unified hunks, each change with the lines around it; nothing when none changed.
pub(super) fn render(lines: &[Line<'_>]) -> String {
    let mut hunks: Vec<(usize, usize)> = Vec::new();
    for (i, (sign, _, _)) in lines.iter().enumerate() {
        if *sign == ' ' {
            continue;
        }
        let start = i.saturating_sub(CONTEXT);
        let end = (i + CONTEXT + 1).min(lines.len());
        if let Some(last) = hunks.last_mut().filter(|last| start <= last.1) {
            last.1 = end;
        } else {
            hunks.push((start, end));
        }
    }
    let mut report = String::new();
    let (mut used, mut at, mut old, mut new) = (0, 0, 0, 0);
    for (start, end) in hunks {
        for (sign, _, _) in &lines[at..start] {
            old += usize::from(*sign != '+');
            new += usize::from(*sign != '-');
        }
        let hunk = &lines[start..end];
        let old_count = hunk.iter().filter(|line| line.0 != '+').count();
        let new_count = hunk.iter().filter(|line| line.0 != '-').count();
        let header = format!(
            "@@ -{},{} +{},{} @@\n",
            old + usize::from(old_count != 0),
            old_count,
            new + usize::from(new_count != 0),
            new_count
        );
        if !append(&mut report, &mut used, &[&header], 1) {
            return report;
        }
        for (sign, text, ending) in hunk {
            let unterminated = ending.is_empty();
            let extra = if unterminated {
                "\n\\ No newline at end of file\n"
            } else {
                ""
            };
            let mut sign_bytes = [0; 4];
            let parts: [&str; 4] = [sign.encode_utf8(&mut sign_bytes), text, ending, extra];
            if !append(
                &mut report,
                &mut used,
                &parts,
                1 + usize::from(unterminated),
            ) {
                return report;
            }
        }
        old += old_count;
        new += new_count;
        at = end;
    }
    report
}

/// Add `parts` to `report`, `count` lines. They are measured, not joined first, so an enormous
/// source line is never copied.
fn append(report: &mut String, used: &mut usize, parts: &[&str], count: usize) -> bool {
    let size: usize = parts.iter().map(|part| part.len()).sum();
    // Always reserve room for the notice if later hunks exceed the budget.
    if *used + count >= MAX_LINES || report.len() + size + TRUNCATED.len() > MAX_BYTES {
        report.push_str(TRUNCATED);
        return false;
    }
    report.extend(parts.iter().copied());
    *used += count;
    true
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
