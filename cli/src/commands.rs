//! One command, one SDK call.
//!
//! The CLI holds no rules of its own. Each command builds the request the SDK
//! already types, hands it to the blocking client, and prints whatever came
//! back. Paging is the SDK's cursor iterator, so no command walks a cursor by
//! hand; validation, scheduling and deduplication stay in the pool.
//!
//! The two commands that are loops rather than single calls are stated as such:
//! `run watch` polls the run detail the contract exposes (there is no run
//! stream route in v1), and `relay follow` iterates the SDK's relay pager.

use std::fmt::Write as _;
use std::io::{BufRead, Read};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use serde::de::DeserializeOwned;
use tidepool_client::blocking::Client;
use tidepool_client::endpoints::{
    DeleteV1HostsIdExecution, GetV1ActivityRunsRun, GetV1Chatrooms, GetV1Hosts,
    GetV1ProjectsIdConfiguration, GetV1SetupStatus, PostV1ChatroomsRoomMessages, PostV1Hosts,
    PostV1HostsEnroll, PostV1HostsEnrollmentTokens, PostV1HostsIdExecution, PostV1Runs,
    PostV1TriggersInstall, PostV1TriggersValidate,
};
use tidepool_client::paging::{PageOptions, relay_events};
use tidepool_client::types::{
    ActivityRunDetail, CreateHostRequest, EnrollHostRequest, HostsResponse, JsonValue,
    ManualDispatchRequest, ManualDispatchResponse, MessageView, MintEnrollmentTokenRequest,
    RevisionView, RoomView, SendMessageBody, SetupStatusResponse, TriggerDocument,
    TriggerInstallRequest, TriggerInstallResponse, TriggerValidateResponse,
};

use crate::args::{Command, Invocation, USAGE};
use crate::credentials::{self, CredentialStore};
use crate::error::{CliError, Result};
use crate::output::Printer;

/// Statuses after which a run will not change again.
const TERMINAL_RUN_STATUSES: [&str; 3] = ["succeeded", "failed", "interrupted"];

/// Run one parsed command.
pub fn execute(
    invocation: &Invocation,
    store: &mut CredentialStore,
    stdin: &mut dyn Read,
    out: &mut dyn std::io::Write,
) -> Result<()> {
    let mut printer = Printer::new(invocation.json, out);
    match &invocation.command {
        Command::Help => printer.line(USAGE),
        Command::Login {
            email,
            password_stdin,
        } => login(
            invocation,
            store,
            email.as_deref(),
            *password_stdin,
            stdin,
            &mut printer,
        ),
        Command::PoolStatus => {
            let client = anonymous_client(invocation, store)?;
            let status = client.call(&GetV1SetupStatus::new())?;
            printer.emit(&status, render_pool_status)
        }
        Command::HostList => {
            let client = signed_in_client(invocation, store)?;
            let hosts = client.call(&GetV1Hosts::new())?;
            printer.emit(&hosts, render_hosts)
        }
        Command::HostEnroll {
            hostname,
            display_name,
            host,
        } => {
            let client = signed_in_client(invocation, store)?;
            host_enroll(
                &client,
                store,
                hostname.as_deref(),
                display_name.clone(),
                host.as_deref(),
                &mut printer,
            )
        }
        Command::HostGrant { host, revoke } => {
            let client = signed_in_client(invocation, store)?;
            let updated = if *revoke {
                client.call(&DeleteV1HostsIdExecution::new(host.clone()))?
            } else {
                client.call(&PostV1HostsIdExecution::new(host.clone(), JsonValue::Null))?
            };
            let verb = if *revoke { "revoked" } else { "granted" };
            printer.emit(&updated, |updated| {
                format!(
                    "{verb} host.execute for {} ({}); status: {}, execution_enabled: {}",
                    updated.slug,
                    updated.id,
                    updated.status,
                    yes_no(updated.execution_enabled)
                )
            })
        }
        Command::TriggerList { project } => {
            let client = signed_in_client(invocation, store)?;
            let revision = client.call(&GetV1ProjectsIdConfiguration::new(project.clone()))?;
            printer.emit(&revision, render_revision)
        }
        Command::TriggerValidate { file } => {
            let client = signed_in_client(invocation, store)?;
            let document: TriggerDocument = read_json_file(Path::new(file))?;
            let response = client.call(&PostV1TriggersValidate::new(document))?;
            printer.emit(&response, render_validation)
        }
        Command::TriggerInstall { project, file } => {
            let client = signed_in_client(invocation, store)?;
            let document: TriggerDocument = read_json_file(Path::new(file))?;
            let request = TriggerInstallRequest {
                event: document.event,
                filters: document.filters,
                name: document.name,
                project: project.clone(),
                run: document.run,
            };
            let response = client.call(&PostV1TriggersInstall::new(request))?;
            printer.emit(&response, render_install)
        }
        Command::RunDispatch {
            project,
            trigger,
            actor,
            delivery_key,
            input,
            revision,
        } => {
            let client = signed_in_client(invocation, store)?;
            let actor = actor
                .clone()
                .or_else(|| store.file().email.clone())
                .ok_or_else(|| {
                    CliError::Usage(
                        "`run dispatch` needs `--actor` or a signed-in member".to_owned(),
                    )
                })?;
            let delivery_key = match delivery_key {
                Some(key) => key.clone(),
                None => format!("cli-{}", generate_secret(16)?),
            };
            let request = ManualDispatchRequest {
                actor,
                delivery_key,
                input: input.clone(),
                project: project.clone(),
                revision: revision.clone(),
                trigger: trigger.clone(),
            };
            let dispatched = client.call(&PostV1Runs::new(request))?;
            printer.emit(&dispatched, render_dispatch)
        }
        Command::RunWatch {
            run,
            interval_ms,
            max_polls,
        } => {
            let client = signed_in_client(invocation, store)?;
            run_watch(&client, run, *interval_ms, *max_polls, &mut printer)
        }
        Command::RoomList => {
            let client = signed_in_client(invocation, store)?;
            let rooms = client.call(&GetV1Chatrooms::new())?;
            printer.emit(&rooms, |rooms| render_rooms(rooms))
        }
        Command::RoomSend {
            room,
            body,
            reply_to,
        } => {
            let client = signed_in_client(invocation, store)?;
            let sent = client.call(&PostV1ChatroomsRoomMessages::new(
                room.clone(),
                SendMessageBody {
                    body: body.clone(),
                    reply_to: reply_to.clone(),
                },
            ))?;
            printer.emit(&sent, render_message)
        }
        Command::RelayFollow {
            session,
            after,
            limit,
            max_pages,
        } => {
            let client = signed_in_client(invocation, store)?;
            relay_follow(&client, session, *after, *limit, *max_pages, &mut printer)
        }
    }
}

fn login(
    invocation: &Invocation,
    store: &mut CredentialStore,
    email: Option<&str>,
    password_stdin: bool,
    stdin: &mut dyn Read,
    printer: &mut Printer<'_>,
) -> Result<()> {
    let email = email
        .ok_or_else(|| CliError::Usage("`login` needs `--email`".to_owned()))?
        .to_owned();
    if !password_stdin {
        return Err(CliError::Usage(
            "`login` reads the password from stdin: pass `--password-stdin` and pipe it".to_owned(),
        ));
    }
    let password = read_password(stdin)?;
    if password.is_empty() {
        return Err(CliError::Usage(
            "the password read from stdin was empty".to_owned(),
        ));
    }

    let client = anonymous_client(invocation, store)?;
    let response = client.login(email, password)?;
    let base_url = base_url(invocation, store)?;

    // The store is written before anything is printed, so a failed save does
    // not leave a person believing they are signed in.
    store.file_mut().pool = Some(base_url);
    store.file_mut().email = Some(response.user.email.clone());
    store.file_mut().token = Some(response.token.clone());
    store.save()?;

    let mut report = response;
    report.token = credentials::redact(&report.token);
    let store_path = store.path().display().to_string();
    printer.emit(&report, |report| {
        format!(
            "signed in as {} ({}){}; token {} stored in {store_path}",
            report.user.email,
            report.user.role,
            if report.must_change_password {
                " — the pool requires a password change next"
            } else {
                ""
            },
            report.token
        )
    })
}

/// Register a host when needed, then complete the two-step exchange: mint a
/// single-use token, then hand it back with a host-generated credential.
fn host_enroll(
    client: &Client,
    store: &mut CredentialStore,
    hostname: Option<&str>,
    display_name: Option<String>,
    host: Option<&str>,
    printer: &mut Printer<'_>,
) -> Result<()> {
    let host_id = match host {
        Some(host_id) => host_id.to_owned(),
        None => {
            let hostname = hostname.ok_or_else(|| {
                CliError::Usage(
                    "`host enroll` needs `--hostname` (or `--host <id>` to enrol an existing host)"
                        .to_owned(),
                )
            })?;
            let registered = client.call(&PostV1Hosts::new(CreateHostRequest {
                display_name,
                hostname: hostname.to_owned(),
            }))?;
            registered.id
        }
    };

    // Step one: the pool mints a ten-minute single-use token.
    let minted = client.call(&PostV1HostsEnrollmentTokens::new(
        MintEnrollmentTokenRequest {
            host_id: host_id.clone(),
        },
    ))?;

    // Step two: the host generates its own relationship secret and exchanges
    // the token for it. The pool stores only a hash of the credential.
    let credential = generate_secret(32)?;
    let enrolled = client.call(&PostV1HostsEnroll::new(EnrollHostRequest {
        token: minted.token.clone(),
        credential: credential.clone(),
    }))?;

    store
        .file_mut()
        .host_credentials
        .insert(enrolled.id.clone(), credential.clone());
    store.file_mut().active_host = Some(enrolled.id.clone());
    store.save()?;

    let note = format!(
        "credential {} stored in {}",
        credentials::redact(&credential),
        store.path().display()
    );
    printer.emit(&enrolled, |enrolled| {
        format!(
            "enrolled {} ({})\nstatus: {}\n{note}",
            enrolled.slug, enrolled.id, enrolled.status
        )
    })
}

fn run_watch(
    client: &Client,
    run: &str,
    interval_ms: u64,
    max_polls: Option<u64>,
    printer: &mut Printer<'_>,
) -> Result<()> {
    let mut polls: u64 = 0;
    loop {
        let detail = client.call(&GetV1ActivityRunsRun::new(run.to_owned()))?;
        polls += 1;
        let finished = TERMINAL_RUN_STATUSES.contains(&detail.run.status.as_str());
        if finished || max_polls.is_some_and(|max| polls >= max) {
            return printer.emit(&detail, render_run);
        }
        if interval_ms > 0 {
            sleep(Duration::from_millis(interval_ms));
        }
    }
}

fn relay_follow(
    client: &Client,
    session: &str,
    after: Option<u64>,
    limit: Option<u64>,
    max_pages: Option<usize>,
    printer: &mut Printer<'_>,
) -> Result<()> {
    let options = PageOptions {
        after,
        limit,
        max_pages,
    };
    for event in relay_events(client, session.to_owned(), options) {
        let event = event?;
        if printer.is_json() {
            printer.json_line(&event)?;
        } else {
            let payload = serde_json::to_string(&event.payload)?;
            printer.line(format!("{} {} {}", event.sequence, event.kind, payload))?;
        }
    }
    Ok(())
}

fn base_url(invocation: &Invocation, store: &CredentialStore) -> Result<String> {
    invocation
        .pool
        .clone()
        .or_else(|| store.file().pool.clone())
        .ok_or(CliError::NoPool)
}

fn anonymous_client(invocation: &Invocation, store: &CredentialStore) -> Result<Client> {
    Ok(Client::from_base_url(base_url(invocation, store)?)?)
}

fn signed_in_client(invocation: &Invocation, store: &CredentialStore) -> Result<Client> {
    // The pool is checked first: "no pool configured" is the fixable step a
    // fresh install hits, and a token from a different pool would not help.
    let base_url = base_url(invocation, store)?;
    let token = store.file().token.clone().ok_or(CliError::NotSignedIn)?;
    Ok(Client::from_base_url(base_url)?.with_token(token))
}

fn read_password(stdin: &mut dyn Read) -> Result<String> {
    let mut reader = std::io::BufReader::new(stdin);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(CliError::Input)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = std::fs::read(path).map_err(|source| CliError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| CliError::DecodeFile {
        path: path.to_path_buf(),
        source,
    })
}

/// A hex secret drawn from the operating system, or a hashed fallback.
///
/// The credential is at least as long as the host protocol requires; the
/// fallback exists so a platform without `/dev/urandom` still produces one
/// rather than refusing to enrol.
pub fn generate_secret(bytes: usize) -> Result<String> {
    let mut buffer = vec![0_u8; bytes];
    if let Ok(mut random) = std::fs::File::open("/dev/urandom") {
        if random.read_exact(&mut buffer).is_ok() {
            return Ok(hex(&buffer));
        }
    }
    Ok(hex(&fallback_bytes(bytes)))
}

fn fallback_bytes(len: usize) -> Vec<u8> {
    use std::hash::{BuildHasher as _, Hasher as _};
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let mut output = Vec::with_capacity(len);
    let mut counter: u64 = 0;
    while output.len() < len {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(seed);
        hasher.write_u32(std::process::id());
        hasher.write_u64(counter);
        output.extend_from_slice(&hasher.finish().to_le_bytes());
        counter += 1;
    }
    output.truncate(len);
    output
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing into a String cannot fail.
        let _ = write!(
            text,
            "{}{}",
            DIGITS[usize::from(byte >> 4)] as char,
            DIGITS[usize::from(byte & 0x0f)] as char
        );
    }
    text
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn render_pool_status(status: &SetupStatusResponse) -> String {
    format!(
        "needs_setup: {}\nbootstrap_configured: {}",
        yes_no(status.needs_setup),
        yes_no(status.bootstrap_configured)
    )
}

fn render_hosts(response: &HostsResponse) -> String {
    if response.hosts.is_empty() {
        return "no hosts enrolled".to_owned();
    }
    let mut lines = vec!["SLUG\tSTATUS\tEXECUTION\tDISPLAY NAME\tID".to_owned()];
    lines.extend(response.hosts.iter().map(|host| {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            host.slug,
            host.status,
            yes_no(host.execution_enabled),
            host.display_name,
            host.id
        )
    }));
    lines.join("\n")
}

fn render_revision(revision: &RevisionView) -> String {
    let mut lines = vec![format!(
        "revision {} version {} status {} active {}",
        revision.id,
        revision.version,
        revision.status,
        yes_no(revision.active)
    )];
    if revision.files.is_empty() {
        lines.push("no authored files".to_owned());
    } else {
        lines.extend(
            revision
                .files
                .iter()
                .map(|file| format!("{}\t{} bytes", file.path, file.content.len())),
        );
    }
    lines.extend(
        revision
            .errors
            .iter()
            .map(|issue| format!("error {}: {}", issue.field, issue.message)),
    );
    lines.join("\n")
}

fn render_validation(response: &TriggerValidateResponse) -> String {
    if response.valid {
        return format!("trigger {} is valid", response.name);
    }
    let mut lines = vec![format!("trigger {} is invalid", response.name)];
    lines.extend(
        response
            .errors
            .iter()
            .map(|issue| format!("{}: {}", issue.field, issue.message)),
    );
    lines.join("\n")
}

fn render_install(response: &TriggerInstallResponse) -> String {
    format!(
        "installed {} id={} revision={} version={} active={} replaced={}",
        response.name,
        response.id,
        response.revision,
        response.version,
        yes_no(response.active),
        yes_no(response.replaced)
    )
}

fn render_dispatch(response: &ManualDispatchResponse) -> String {
    format!(
        "run {} status={} deduplicated={}",
        response.run.id,
        response.run.status,
        yes_no(response.deduplicated)
    )
}

fn render_run(detail: &ActivityRunDetail) -> String {
    let run = &detail.run;
    let mut lines = vec![format!(
        "run {} status={} project={} trigger={}",
        run.id, run.status, run.project, run.trigger_name
    )];
    lines.extend(run.steps.iter().map(|step| {
        format!(
            "step {} {} execution={} {}",
            step.position,
            step.step,
            step.execution_id.as_deref().unwrap_or("-"),
            step.error.as_deref().unwrap_or(step.status.as_str())
        )
    }));
    lines.join("\n")
}

fn render_rooms(rooms: &[RoomView]) -> String {
    if rooms.is_empty() {
        return "no rooms visible".to_owned();
    }
    let mut lines = vec!["ID\tVISIBILITY\tNAME".to_owned()];
    lines.extend(
        rooms
            .iter()
            .map(|room| format!("{}\t{}\t{}", room.id, room.visibility, room.name)),
    );
    lines.join("\n")
}

fn render_message(message: &MessageView) -> String {
    format!(
        "sent {} sequence={} at {}",
        message.id, message.sequence, message.created_at
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_secrets_are_long_and_hex() {
        let secret = generate_secret(32).expect("secret");
        assert_eq!(secret.len(), 64);
        assert!(
            secret
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
        let other = generate_secret(32).expect("secret");
        assert_ne!(secret, other);
    }

    #[test]
    fn the_fallback_still_produces_a_full_length_secret() {
        let bytes = fallback_bytes(32);
        assert_eq!(bytes.len(), 32);
        assert_ne!(bytes, fallback_bytes(32));
    }

    #[test]
    fn a_short_password_line_is_trimmed_of_its_newline() {
        let mut input = std::io::Cursor::new(b"hunter2\r\n".to_vec());
        assert_eq!(read_password(&mut input).expect("password"), "hunter2");
    }

    #[test]
    fn terminal_statuses_are_the_contracts_three() {
        for status in TERMINAL_RUN_STATUSES {
            assert!(["succeeded", "failed", "interrupted"].contains(&status));
        }
    }
}
