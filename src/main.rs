use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use sdkman_contrib::apply::{RunInfo, apply_queue, void_request};
use sdkman_contrib::change_set::added_request;
use sdkman_contrib::check::{CheckContext, check_request};
use sdkman_contrib::check_digests::CheckDigests;
use sdkman_contrib::fetch::HttpArchiveSource;
use sdkman_contrib::git::Git;
use sdkman_contrib::github::GitHubDigests;
use sdkman_contrib::publish::{publish_release, release_bodies};
use sdkman_contrib::report::{Report, ReportLine, Status};
use sdkman_contrib::request_path::Kind;
use sdkman_contrib::skeleton::{
    registration_skeleton, release_skeleton, skeleton_path, write_skeleton,
};
use sdkman_contrib::state_api::{Credentials, HttpState, resolve_state_url};

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
    /// Apply every pending request to State and push the receipts
    Apply(ApplyArgs),
    /// Write a request skeleton stamped with the current UTC time
    #[command(subcommand)]
    New(NewCommand),
    /// Publish this version's five release archives to State as the vendor of `contrib`
    #[command(hide = true)]
    Publish(PublishArgs),
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

    /// Write the computed sha256 of every downloaded row to this path as JSON
    #[arg(long)]
    digests: Option<PathBuf>,
}

#[derive(Args)]
struct ApplyArgs {
    /// Void this pending request before applying the queue
    #[arg(long, requires = "reason")]
    void: Option<String>,

    /// Why the request is voided
    #[arg(long, requires = "void")]
    reason: Option<String>,
}

#[derive(Args)]
struct PublishArgs {
    /// The URL the archives are downloadable from, without the file name
    #[arg(long)]
    url_base: String,

    /// The directory holding the five `contrib-<version>-<suffix>` archives
    directory: PathBuf,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check(args) => check(args),
        Command::Apply(args) => apply(args),
        Command::New(command) => new(command),
        Command::Publish(args) => publish(args),
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
    let mut digests: Option<CheckDigests> = None;
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
            let outcome = check_request(&context, &repo_path);
            digests = Some(outcome.digests);
            outcome.report
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
    if let (Some(path), Some(digests)) = (args.digests, digests)
        && let Err(error) = write_digests(&path, &digests)
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

fn write_digests(path: &Path, digests: &CheckDigests) -> Result<(), String> {
    let json = serde_json::to_string_pretty(digests).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

fn apply(args: ApplyArgs) -> ExitCode {
    let credentials = match credentials_from_env("COMMUNITY_EMAIL", "COMMUNITY_PASSWORD") {
        Ok(credentials) => credentials,
        Err(code) => return code,
    };
    let (Some(repository), Some(github_token)) = (
        non_empty_env("GITHUB_REPOSITORY"),
        non_empty_env("GITHUB_TOKEN"),
    ) else {
        eprintln!("missing GITHUB_REPOSITORY or GITHUB_TOKEN");
        return ExitCode::from(2);
    };
    let github_api_url =
        non_empty_env("GITHUB_API_URL").unwrap_or_else(|| "https://api.github.com".to_owned());
    let state_url = resolve_state_url(env::var("CONTRIB_STATE_URL").ok());
    eprintln!("applying to {state_url}");

    let state = match login_to_state(&state_url, credentials) {
        Ok(state) => state,
        Err(code) => return code,
    };

    let repo_root = env::current_dir().expect("current directory");
    let git = Git::new(&repo_root);
    let run = RunInfo::from_env(Utc::now());
    if let (Some(path), Some(reason)) = (&args.void, &args.reason)
        && let Err(error) = void_request(&repo_root, &git, &run, &normalise_path(path), reason)
    {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }

    let archives = HttpArchiveSource::new();
    let digests = GitHubDigests::new(&github_api_url, &repository, &github_token);
    match apply_queue(&repo_root, &state, &archives, &digests, &git, &run) {
        Ok(outcome) => {
            for path in &outcome.applied {
                println!("applied {path}");
            }
            for blocked in &outcome.blocked {
                eprintln!("blocked {}: {}", blocked.path, blocked.reason);
            }
            for path in &outcome.skipped {
                println!("skipped {path}");
            }
            if outcome.blocked.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn publish(args: PublishArgs) -> ExitCode {
    let credentials = match credentials_from_env("VENDOR_EMAIL", "VENDOR_PASSWORD") {
        Ok(credentials) => credentials,
        Err(code) => return code,
    };
    let version = env!("CARGO_PKG_VERSION");
    let bodies = match release_bodies(&args.directory, version, &args.url_base) {
        Ok(bodies) => bodies,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let state_url = resolve_state_url(env::var("CONTRIB_STATE_URL").ok());
    eprintln!("publishing to {state_url}");

    let state = match login_to_state(&state_url, credentials) {
        Ok(state) => state,
        Err(code) => return code,
    };
    match publish_release(&state, &bodies) {
        Ok(()) => {
            println!("published contrib {version}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn credentials_from_env(
    email_variable: &str,
    password_variable: &str,
) -> Result<Credentials, ExitCode> {
    match (
        non_empty_env(email_variable),
        non_empty_env(password_variable),
    ) {
        (Some(email), Some(password)) => Ok(Credentials { email, password }),
        _ => {
            eprintln!("missing {email_variable} or {password_variable}");
            Err(ExitCode::from(2))
        }
    }
}

fn login_to_state(state_url: &str, credentials: Credentials) -> Result<HttpState, ExitCode> {
    let state = HttpState::with_credentials(state_url, credentials);
    match state.login() {
        Ok(()) => Ok(state),
        Err(message) => {
            eprintln!("{message}");
            Err(ExitCode::FAILURE)
        }
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
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
