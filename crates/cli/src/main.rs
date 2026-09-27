// SPDX-License-Identifier: GPL-3.0-or-later
//! The headless command line: script the engine with no window (architecture §3.1). `create`,
//! `list`, `rebuild`, `verify`, `rate`, `flag`, `keyword` (WP3) and `source` (WP4: add a folder or
//! a removable volume in place, scan it, confirm new files). `export` and develop follow later.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use auroraw_catalogue::CatalogueError;
use auroraw_engine::{Command, Engine, EngineError, Event, EventReceiver, JobId, Outcome};
use auroraw_format::sidecar::Flag;
use auroraw_types::{KeywordId, PhotoId, SourceId};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage) => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Engine(e)) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "\
usage: auroraw-cli --version
       auroraw-cli create <workspace-dir> <catalogue-file> <name>
       auroraw-cli list <workspace-dir> <catalogue-file> [--min-rating N]
       auroraw-cli rebuild <workspace-dir> <catalogue-file>
       auroraw-cli verify <workspace-dir> <catalogue-file>
       auroraw-cli rate <workspace-dir> <catalogue-file> <photo-id> <0-5>
       auroraw-cli flag <workspace-dir> <catalogue-file> <photo-id> <pick|reject|none>
       auroraw-cli keyword create <workspace-dir> <catalogue-file> <name> [--parent <id>]
       auroraw-cli keyword rename <workspace-dir> <catalogue-file> <keyword-id> <new-name>
       auroraw-cli keyword add <workspace-dir> <catalogue-file> <photo-id> <keyword-id>
       auroraw-cli keyword remove <workspace-dir> <catalogue-file> <photo-id> <keyword-id>
       auroraw-cli source add <workspace-dir> <catalogue-file> <path> <name> [--removable]
       auroraw-cli source list <workspace-dir> <catalogue-file>
       auroraw-cli source scan <workspace-dir> <catalogue-file> <source-id>
       auroraw-cli source add-new <workspace-dir> <catalogue-file> <source-id> --all
       auroraw-cli source add-new <workspace-dir> <catalogue-file> <source-id> <path>...
       auroraw-cli duplicates <workspace-dir> <catalogue-file>";

enum CliError {
    Usage,
    Engine(EngineError),
}

impl From<EngineError> for CliError {
    fn from(e: EngineError) -> Self {
        CliError::Engine(e)
    }
}

impl From<CatalogueError> for CliError {
    fn from(e: CatalogueError) -> Self {
        CliError::Engine(e.into())
    }
}

fn run(args: &[String]) -> Result<(), CliError> {
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("auroraw-cli {}", Engine::version());
            Ok(())
        }
        Some("create") => {
            let (workspace, catalogue, name) = paths3(&args[1..])?;
            let (_engine, _events) = Engine::create(&workspace, &catalogue, &name)?;
            println!(
                "created {} indexed by {}",
                workspace.display(),
                catalogue.display()
            );
            Ok(())
        }
        Some("list") => cmd_list(&args[1..]),
        Some("rebuild") => cmd_rebuild(&args[1..]),
        Some("verify") => cmd_verify(&args[1..]),
        Some("rate") => cmd_rate(&args[1..]),
        Some("flag") => cmd_flag(&args[1..]),
        Some("keyword") => cmd_keyword(&args[1..]),
        Some("source") => cmd_source(&args[1..]),
        Some("duplicates") => cmd_duplicates(&args[1..]),
        _ => Err(CliError::Usage),
    }
}

fn paths2(args: &[String]) -> Result<(PathBuf, PathBuf), CliError> {
    match args {
        [workspace, catalogue] => Ok((PathBuf::from(workspace), PathBuf::from(catalogue))),
        _ => Err(CliError::Usage),
    }
}

fn paths3(args: &[String]) -> Result<(PathBuf, PathBuf, String), CliError> {
    match args {
        [workspace, catalogue, name] => Ok((
            PathBuf::from(workspace),
            PathBuf::from(catalogue),
            name.clone(),
        )),
        _ => Err(CliError::Usage),
    }
}

fn open(workspace: &Path, catalogue: &Path) -> Result<(Engine, EventReceiver), CliError> {
    Ok(Engine::open(workspace, catalogue)?)
}

/// Blocks until `job` reports finished or cancelled, printing progress as it goes.
fn wait_for_job(events: &EventReceiver, job: JobId) {
    loop {
        match events.recv_timeout(Duration::from_secs(30)) {
            Some(Event::JobProgress {
                job: j,
                done,
                total,
            }) if j == job => {
                println!("  refreshing... {done}/{total}");
            }
            Some(Event::JobFinished(j)) if j == job => {
                println!("  refresh finished");
                return;
            }
            Some(Event::JobCancelled(j)) if j == job => {
                println!("  refresh cancelled");
                return;
            }
            Some(_) => continue,
            None => {
                eprintln!("  warning: no news from job {job} in 30s, giving up waiting");
                return;
            }
        }
    }
}

fn cmd_rebuild(args: &[String]) -> Result<(), CliError> {
    let (workspace, catalogue) = paths2(args)?;
    let (engine, _events) = open(&workspace, &catalogue)?;
    match engine.submit_and_wait(Command::Rebuild)? {
        Outcome::Applied => println!("rebuilt"),
        other => unreachable!("Rebuild always returns Applied, got {other:?}"),
    }
    Ok(())
}

fn cmd_verify(args: &[String]) -> Result<(), CliError> {
    let (workspace, catalogue) = paths2(args)?;
    let (engine, _events) = open(&workspace, &catalogue)?;
    match engine.submit_and_wait(Command::Reconcile)? {
        Outcome::Applied => println!("verified"),
        other => unreachable!("Reconcile always returns Applied, got {other:?}"),
    }
    Ok(())
}

/// Every photo with more than one confirmed location (D-036, D-108): read-only, like `list`.
fn cmd_duplicates(args: &[String]) -> Result<(), CliError> {
    let (workspace, catalogue) = paths2(args)?;
    let (engine, _events) = open(&workspace, &catalogue)?;
    let duplicates = engine.duplicate_photos()?;
    for photo in &duplicates {
        println!("{}", photo.filename);
        println!("  {} — {}", photo.primary.source_name, photo.primary.path);
        for extra in &photo.extra {
            println!("  {} — {}", extra.source_name, extra.path);
        }
    }
    println!("{} duplicate photo(s)", duplicates.len());
    Ok(())
}

fn cmd_list(args: &[String]) -> Result<(), CliError> {
    let (workspace, catalogue, min_rating) = match args {
        [workspace, catalogue] => (workspace, catalogue, None),
        [workspace, catalogue, flag, value] if flag == "--min-rating" => (
            workspace,
            catalogue,
            Some(value.parse::<u8>().map_err(|_| CliError::Usage)?),
        ),
        _ => return Err(CliError::Usage),
    };
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    let catalogue = engine.read_catalogue()?;
    let rows = match min_rating {
        Some(min) => catalogue.list_by_min_rating(min, None, 10_000)?,
        None => catalogue.list_recent(None, 10_000)?,
    };
    for row in &rows {
        let flag = match row.flag {
            1 => "picked",
            2 => "rejected",
            _ => "-",
        };
        println!(
            "{}  rating={}  flag={}  {}",
            row.id,
            row.effective_rating,
            flag,
            row.title.as_deref().unwrap_or(&row.filename)
        );
    }
    println!("{} photo(s)", rows.len());
    Ok(())
}

fn cmd_rate(args: &[String]) -> Result<(), CliError> {
    let [workspace, catalogue, photo_id, rating] = args else {
        return Err(CliError::Usage);
    };
    let photo_id: PhotoId = photo_id.parse().map_err(|_| CliError::Usage)?;
    let rating: u8 = rating.parse().map_err(|_| CliError::Usage)?;
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    engine.submit_and_wait(Command::SetRating { photo_id, rating })?;
    println!("rated {photo_id} {rating}");
    Ok(())
}

fn cmd_flag(args: &[String]) -> Result<(), CliError> {
    let [workspace, catalogue, photo_id, flag] = args else {
        return Err(CliError::Usage);
    };
    let photo_id: PhotoId = photo_id.parse().map_err(|_| CliError::Usage)?;
    let flag = match flag.as_str() {
        "pick" => Some(Flag::Picked),
        "reject" => Some(Flag::Rejected),
        "none" => None,
        _ => return Err(CliError::Usage),
    };
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    engine.submit_and_wait(Command::SetFlag { photo_id, flag })?;
    println!(
        "flagged {photo_id} {}",
        flag.map(|f| format!("{f:?}"))
            .unwrap_or_else(|| "none".into())
    );
    Ok(())
}

fn cmd_keyword(args: &[String]) -> Result<(), CliError> {
    match args.first().map(String::as_str) {
        Some("create") => cmd_keyword_create(&args[1..]),
        Some("rename") => cmd_keyword_rename(&args[1..]),
        Some("add") => cmd_keyword_edit(&args[1..], true),
        Some("remove") => cmd_keyword_edit(&args[1..], false),
        _ => Err(CliError::Usage),
    }
}

fn cmd_keyword_create(args: &[String]) -> Result<(), CliError> {
    let (parent, rest) = take_parent(args)?;
    let [workspace, catalogue, name] = rest else {
        return Err(CliError::Usage);
    };
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    let Outcome::KeywordCreated(id) = engine.submit_and_wait(Command::CreateKeyword {
        name: name.clone(),
        parent,
        id: None,
    })?
    else {
        unreachable!("CreateKeyword always returns KeywordCreated");
    };
    println!("created keyword {id}");
    Ok(())
}

fn take_parent(args: &[String]) -> Result<(Option<KeywordId>, &[String]), CliError> {
    if args.first().map(String::as_str) == Some("--parent") {
        let value = args.get(1).ok_or(CliError::Usage)?;
        let id: KeywordId = value.parse().map_err(|_| CliError::Usage)?;
        Ok((Some(id), &args[2..]))
    } else {
        Ok((None, args))
    }
}

fn cmd_keyword_rename(args: &[String]) -> Result<(), CliError> {
    let [workspace, catalogue, keyword_id, new_name] = args else {
        return Err(CliError::Usage);
    };
    let keyword_id: KeywordId = keyword_id.parse().map_err(|_| CliError::Usage)?;
    let (engine, events) = open(Path::new(workspace), Path::new(catalogue))?;
    let Outcome::RenameStarted { job, affected } =
        engine.submit_and_wait(Command::RenameKeyword {
            keyword_id,
            new_name: new_name.clone(),
        })?
    else {
        unreachable!("RenameKeyword always returns RenameStarted");
    };
    println!("renamed {keyword_id}; refreshing {affected} sidecar(s)");
    wait_for_job(&events, job);
    Ok(())
}

fn cmd_keyword_edit(args: &[String], add: bool) -> Result<(), CliError> {
    let [workspace, catalogue, photo_id, keyword_id] = args else {
        return Err(CliError::Usage);
    };
    let photo_id: PhotoId = photo_id.parse().map_err(|_| CliError::Usage)?;
    let keyword_id: KeywordId = keyword_id.parse().map_err(|_| CliError::Usage)?;
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    let command = if add {
        Command::AddKeyword {
            photo_id,
            keyword_id,
        }
    } else {
        Command::RemoveKeyword {
            photo_id,
            keyword_id,
        }
    };
    engine.submit_and_wait(command)?;
    println!(
        "{} keyword {keyword_id} {} photo {photo_id}",
        if add { "added" } else { "removed" },
        if add { "to" } else { "from" }
    );
    Ok(())
}

fn cmd_source(args: &[String]) -> Result<(), CliError> {
    match args.first().map(String::as_str) {
        Some("add") => cmd_source_add(&args[1..]),
        Some("list") => cmd_source_list(&args[1..]),
        Some("scan") => cmd_source_scan(&args[1..]),
        Some("add-new") => cmd_source_add_new(&args[1..]),
        _ => Err(CliError::Usage),
    }
}

fn cmd_source_add(args: &[String]) -> Result<(), CliError> {
    let (removable, rest) = match args {
        [rest @ .., flag] if flag == "--removable" => (true, rest),
        rest => (false, rest),
    };
    let [workspace, catalogue, path, name] = rest else {
        return Err(CliError::Usage);
    };
    let kind = if removable {
        auroraw_sources::filesystem::REMOVABLE_VOLUME
    } else {
        auroraw_sources::filesystem::LOCAL_FOLDER
    };
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    let Outcome::SourceAdded(id) = engine.submit_and_wait(Command::AddSource {
        name: name.clone(),
        root: PathBuf::from(path),
        kind: kind.into(),
    })?
    else {
        unreachable!("AddSource always returns SourceAdded");
    };
    println!("added source {id} ({kind}) at {path}");
    Ok(())
}

fn cmd_source_list(args: &[String]) -> Result<(), CliError> {
    let (workspace, catalogue) = paths2(args)?;
    let (engine, _events) = open(&workspace, &catalogue)?;
    let sources = engine.read_catalogue()?.list_sources()?;
    for s in &sources {
        println!("{}  {}  {}", s.id, s.kind, s.name);
    }
    println!("{} source(s)", sources.len());
    Ok(())
}

fn cmd_source_scan(args: &[String]) -> Result<(), CliError> {
    let [workspace, catalogue, source_id] = args else {
        return Err(CliError::Usage);
    };
    let source_id: SourceId = source_id.parse().map_err(|_| CliError::Usage)?;
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    print_scan_report(&engine, source_id)
}

fn print_scan_report(engine: &Engine, source_id: SourceId) -> Result<(), CliError> {
    let Outcome::Scanned {
        reachable,
        confirmed,
        changed,
        relinked,
        missing,
        new,
        ambiguous,
        second_locations,
    } = engine.submit_and_wait(Command::ScanSource { source_id })?
    else {
        unreachable!("ScanSource always returns Scanned");
    };
    if !reachable {
        println!("source {source_id} is not reachable right now; nothing scanned");
        return Ok(());
    }
    println!(
        "confirmed={confirmed} changed={changed} relinked={relinked} missing={missing} ambiguous={ambiguous} second_locations={second_locations}"
    );
    for path in &new {
        println!("  new: {path}");
    }
    Ok(())
}

fn cmd_source_add_new(args: &[String]) -> Result<(), CliError> {
    let [workspace, catalogue, source_id, rest @ ..] = args else {
        return Err(CliError::Usage);
    };
    let source_id: SourceId = source_id.parse().map_err(|_| CliError::Usage)?;
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    let paths = match rest {
        [flag] if flag == "--all" => {
            let Outcome::Scanned { new, .. } =
                engine.submit_and_wait(Command::ScanSource { source_id })?
            else {
                unreachable!("ScanSource always returns Scanned");
            };
            new
        }
        [] => return Err(CliError::Usage),
        paths => paths.to_vec(),
    };
    let count = paths.len();
    let Outcome::PhotosAdded(added) =
        engine.submit_and_wait(Command::AddNewPhotos { source_id, paths })?
    else {
        unreachable!("AddNewPhotos always returns PhotosAdded");
    };
    for id in &added {
        println!("added photo {id}");
    }
    println!("{}/{count} added", added.len());
    Ok(())
}
