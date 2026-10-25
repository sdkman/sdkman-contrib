use std::fmt;

use crate::verify::RowCheck;

const STATUS_WIDTH: usize = 6;
const SUBJECT_GAP: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Status::Ok => "OK",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportLine {
    pub status: Status,
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    lines: Vec<ReportLine>,
}

impl Report {
    pub fn push(&mut self, line: ReportLine) {
        self.lines.push(line);
    }

    pub fn has_failures(&self) -> bool {
        self.lines.iter().any(|line| line.status == Status::Fail)
    }

    pub fn render_text(&self) -> String {
        let subject_width = self
            .lines
            .iter()
            .map(|line| line.subject.len())
            .max()
            .unwrap_or(0)
            + SUBJECT_GAP;
        self.lines
            .iter()
            .map(|line| {
                let row = format!(
                    "{:<STATUS_WIDTH$}{:<subject_width$}{}",
                    line.status.to_string(),
                    line.subject,
                    line.detail
                );
                format!("{}\n", row.trim_end())
            })
            .collect()
    }

    pub fn render_markdown(&self) -> String {
        let verdict = if self.has_failures() { "FAIL" } else { "PASS" };
        format!(
            "## contrib check: {verdict}\n\n```\n{}```\n",
            self.render_text()
        )
    }
}

pub fn row_subject(candidate: &str, version: &str, platform: &str) -> String {
    format!("{candidate} {version} {platform}")
}

pub fn row_line(candidate: &str, row: &RowCheck, state_failures: &[String]) -> ReportLine {
    let status = if !row.failures.is_empty() || !state_failures.is_empty() {
        Status::Fail
    } else if !row.warnings.is_empty() {
        Status::Warn
    } else {
        Status::Ok
    };
    let checksums = row.checksums.iter().map(|outcome| {
        let verdict = if outcome.matched {
            "matched"
        } else {
            "not matched"
        };
        format!("{} declared, {verdict}", outcome.algorithm)
    });
    let detail = row
        .failures
        .iter()
        .chain(state_failures)
        .chain(&row.warnings)
        .cloned()
        .chain(
            row.digests
                .iter()
                .map(|digests| format!("sha256 {}", digests.sha256)),
        )
        .chain(checksums)
        .collect::<Vec<_>>()
        .join("; ");
    ReportLine {
        status,
        subject: row_subject(candidate, &row.version, &row.platform),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::{ChecksumOutcome, Digests};

    fn row(platform: &str) -> RowCheck {
        RowCheck {
            version: "1.2.0".to_owned(),
            platform: platform.to_owned(),
            url: format!("https://example.com/jpx-1.2.0-{platform}.zip"),
            digests: None,
            checksums: Vec::new(),
            failures: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn digests(sha256: &str) -> Digests {
        Digests {
            md5: "m".to_owned(),
            sha256: sha256.to_owned(),
            sha512: "s".to_owned(),
        }
    }

    fn outcome(algorithm: &'static str, matched: bool) -> ChecksumOutcome {
        ChecksumOutcome {
            algorithm,
            declared: "d".to_owned(),
            matched,
        }
    }

    fn spec_example_report() -> Report {
        let mut failing = row("LINUX_X64");
        failing.failures.push("no bin/ under jpx-1.2.0/".to_owned());
        let mut warning = row("MAC_ARM64");
        warning
            .warnings
            .push("base directory is 'jpx' (expected 'jpx-1.2.0')".to_owned());
        let mut passing = row("WINDOWS_X64");
        passing.digests = Some(digests("9c1e..."));
        let mut report = Report::default();
        for row in [&failing, &warning, &passing] {
            report.push(row_line("jpx", row, &[]));
        }
        report
    }

    const SPEC_EXAMPLE: &str = "\
FAIL  jpx 1.2.0 LINUX_X64    no bin/ under jpx-1.2.0/
WARN  jpx 1.2.0 MAC_ARM64    base directory is 'jpx' (expected 'jpx-1.2.0')
OK    jpx 1.2.0 WINDOWS_X64  sha256 9c1e...
";

    #[test]
    fn renders_spec_example() {
        assert_eq!(spec_example_report().render_text(), SPEC_EXAMPLE);
    }

    #[test]
    fn status_displays_upper_case() {
        let shown: Vec<String> = [Status::Ok, Status::Warn, Status::Fail]
            .iter()
            .map(Status::to_string)
            .collect();
        assert_eq!(shown, ["OK", "WARN", "FAIL"]);
    }

    #[test]
    fn markdown_fences_the_text_under_a_verdict_heading() {
        let markdown = spec_example_report().render_markdown();
        assert_eq!(
            markdown,
            format!("## contrib check: FAIL\n\n```\n{SPEC_EXAMPLE}```\n")
        );
    }

    #[test]
    fn markdown_passes_without_failures() {
        let mut report = Report::default();
        report.push(row_line("jpx", &row("LINUX_X64"), &[]));
        assert!(!report.has_failures());
        assert!(
            report
                .render_markdown()
                .starts_with("## contrib check: PASS\n")
        );
    }

    #[test]
    fn warning_alone_does_not_fail_the_report() {
        let mut warning = row("LINUX_X64");
        warning.warnings.push("odd".to_owned());
        let mut report = Report::default();
        report.push(row_line("jpx", &warning, &[]));
        assert!(!report.has_failures());
    }

    #[test]
    fn state_failure_fails_an_otherwise_clean_row() {
        let line = row_line("jpx", &row("LINUX_X64"), &["State answered 400".to_owned()]);
        assert_eq!(line.status, Status::Fail);
        assert_eq!(line.detail, "State answered 400");
    }

    #[test]
    fn detail_joins_failures_state_warnings_digest_and_checksums() {
        let mut checked = row("LINUX_X64");
        checked.failures.push("bad layout".to_owned());
        checked.warnings.push("odd base".to_owned());
        checked.digests = Some(digests("abc"));
        checked.checksums = vec![outcome("md5", true), outcome("sha512", false)];
        let line = row_line("jpx", &checked, &["State refused".to_owned()]);
        assert_eq!(
            line.detail,
            "bad layout; State refused; odd base; sha256 abc; \
             md5 declared, matched; sha512 declared, not matched"
        );
    }

    #[test]
    fn empty_report_renders_no_text() {
        assert_eq!(Report::default().render_text(), "");
    }
}
