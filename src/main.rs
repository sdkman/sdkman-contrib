use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use sdkman_contrib::change_set::added_request;
use sdkman_contrib::check::{CheckContext, run_check};
use sdkman_contrib::fetch::HttpArchiveSource;
use sdkman_contrib::report::{Report, ReportLine, Status};
use sdkman_contrib::request_path::Kind;
use sdkman_contrib::skeleton::{
    registration_skeleton, release_skeleton, skeleton_path, write_skeleton,
};
use sdkman_contrib::state_api::{HttpState, resolve_state_url};

#[derive(Parser)]
#[command(name = "contrib", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check one request against the contribution rules and State's validation
    Check(CheckArgs),
    /// Write a request skeleton stamped with the current UTC time
    #[command(subcommand)]
    New(NewCommand),
}

#[derive(Subcommand)]
enum NewCommand {
    /// Write a registration skeleton
    Register { candidate: String },
    /// Write a release skeleton with one URL placeholder per platform
    Release {
        candidate: String,
        version: String,
        /// Comma-separated platform ids, such as LINUX_X64,MAC_ARM64
        #[arg(long, required = true, value_delimiter = ',')]
        platforms: Vec<String>,
    },
}

#[derive(Args)]
struct CheckArgs {
    /// The request file, as the shell writes it
    #[arg(required_unless_present = "base")]
    file: Option<String>,

    /// Take the request from the files this checkout adds against this revision
    #[arg(long)]
    base: Option<String>,

    /// Write the markdown report to this path
    #[arg(long)]
    report: Option<PathBuf>,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check(args) => check(args),
        Command::New(command) => new(command),
    }
}

fn new(command: NewCommand) -> ExitCode {
    let (candidate, kind, slug_source, text) = match command {
        NewCommand::Register { candidate } => {
            let text = registration_skeleton(&candidate);
            (candidate.clone(), Kind::Register, candidate, text)
        }
        NewCommand::Release {
            candidate,
            version,
            platforms,
        } => {
            let text = release_skeleton(&candidate, &version, &platforms);
            (candidate, Kind::Release, version, text)
        }
    };
    let repo_root = env::current_dir().expect("current directory");
    let written = skeleton_path(&candidate, kind, &slug_source, Utc::now())
        .map_err(|error| error.to_string())
        .and_then(|path| {
            write_skeleton(&repo_root, &path, &text)
                .map(|()| path.clone())
                .map_err(|error| format!("cannot write {path}: {error}"))
        });
    match written {
        Ok(path) => {
            println!("wrote {path}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn check(args: CheckArgs) -> ExitCode {
    let repo_root = env::current_dir().expect("current directory");
    let state_url = resolve_state_url(env::var("CONTRIB_STATE_URL").ok());
    eprintln!("validating against {state_url}");

    let file = args.file.as_deref().map(normalise_path);
    let report = match select_file(&repo_root, file, args.base.as_deref()) {
        Ok(repo_path) => {
            let state = HttpState::new(&state_url);
            let archives = HttpArchiveSource::new();
            let context = CheckContext {
                repo_root: &repo_root,
                now: Utc::now(),
                validator: &state,
                archives: &archives,
            };
            run_check(&context, &repo_path)
        }
        Err(message) => {
            let mut report = Report::default();
            report.push(ReportLine {
                status: Status::Fail,
                subject: "change set".to_owned(),
                detail: message,
            });
            report
        }
    };

    print!("{}", report.render_text());
    if let Some(path) = args.report
        && let Err(error) = fs::write(&path, report.render_markdown())
    {
        eprintln!("cannot write {}: {error}", path.display());
        return ExitCode::FAILURE;
    }
    if report.has_failures() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn normalise_path(file: &str) -> String {
    let slashed = file.replace('\\', "/");
    slashed.strip_prefix("./").unwrap_or(&slashed).to_owned()
}

fn select_file(
    repo_root: &Path,
    file: Option<String>,
    base: Option<&str>,
) -> Result<String, String> {
    match (file, base) {
        (file, Some(base)) => {
            let added = added_request(repo_root, base)?;
            match file {
                Some(file) if file != added => Err(format!(
                    "{file} is not the request this change adds ({added})"
                )),
                _ => Ok(added),
            }
        }
        (Some(file), None) => Ok(file),
        (None, None) => Err("name a request file or pass --base".to_owned()),
    }
}
