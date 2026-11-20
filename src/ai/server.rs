use std::{
    env, fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use super::{AiError, Result};

const MAX_LINE_BYTES: u64 = 1024 * 1024;
const DISABLE_GRACE: Duration = Duration::from_millis(200);

pub(super) struct Server {
    process: Process,
    write: SyncSender<Outgoing>,
    read: Receiver<Result<Value>>,
    stopping: Arc<AtomicBool>,
}

struct Process {
    child: Child,
    _directory: tempfile::TempDir,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum Outgoing {
    Message(Value),
    Shutdown(Vec<Value>),
}

impl Server {
    pub fn launch(executable: Option<&Path>, home: &Path) -> Result<Self> {
        let executable = match executable {
            Some(path) => fs::canonicalize(path).map_err(launch_error)?,
            None => discover_cli().ok_or(AiError::MissingCli)?,
        };
        fs::create_dir_all(home).map_err(|_| AiError::LocalPreferences)?;
        let home = fs::canonicalize(home).map_err(|_| AiError::LocalPreferences)?;
        let directory = tempfile::tempdir().map_err(|_| AiError::StartFailed)?;
        let mut command = Command::new(executable);
        command
            .args(["app-server", "--listen", "stdio://"])
            .env("CODEX_HOME", home)
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .env_remove("OPENAI_BASE_URL")
            .env_remove("OPENAI_ORG_ID")
            .env_remove("OPENAI_PROJECT_ID")
            .env_remove("CODEX_API_BASE_URL")
            .current_dir(directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        for setting in [
            "forced_login_method=\"chatgpt\"",
            "model_provider=\"openai\"",
            "sandbox_mode=\"read-only\"",
            "approval_policy=\"never\"",
            "web_search=\"disabled\"",
            "features.shell_tool=false",
            "features.shell_snapshot=false",
            "features.unified_exec=false",
            "features.apply_patch_freeform=false",
            "features.multi_agent=false",
            "features.multi_agent_v2=false",
            "features.apps=false",
            "features.plugins=false",
            "features.hooks=false",
            "features.memories=false",
            "features.in_app_browser=false",
        ] {
            command.args(["-c", setting]);
        }
        let mut process = Process {
            child: command.spawn().map_err(launch_error)?,
            _directory: directory,
        };
        let stdin = process.child.stdin.take().ok_or(AiError::StartFailed)?;
        let stdout = process.child.stdout.take().ok_or(AiError::StartFailed)?;
        let (write, outgoing) = mpsc::sync_channel(16);
        let (incoming, read) = mpsc::sync_channel(32);
        let read_errors = incoming.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let writer_stopping = Arc::clone(&stopping);
        thread::Builder::new()
            .name("codex-stdio-read".into())
            .spawn(move || read_messages(stdout, incoming))
            .map_err(|_| AiError::StartFailed)?;
        thread::Builder::new()
            .name("codex-stdio-write".into())
            .spawn(move || {
                let mut stdin = io::BufWriter::new(stdin);
                while let Ok(message) = outgoing.recv() {
                    let result = match message {
                        Outgoing::Message(value) => {
                            if writer_stopping.load(Ordering::Acquire) {
                                continue;
                            }
                            write_message(&mut stdin, &value)
                        }
                        Outgoing::Shutdown(values) => {
                            for value in values {
                                if write_message(&mut stdin, &value).is_err() {
                                    break;
                                }
                            }
                            return;
                        }
                    };
                    if result.is_err() {
                        let _ = read_errors.send(Err(AiError::ProcessEnded));
                        break;
                    }
                }
            })
            .map_err(|_| AiError::StartFailed)?;
        Ok(Self {
            process,
            write,
            read,
            stopping,
        })
    }

    pub fn send(&self, message: Value) -> Result<()> {
        self.write
            .try_send(Outgoing::Message(message))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => AiError::Busy,
                mpsc::TrySendError::Disconnected(_) => AiError::ProcessEnded,
            })
    }

    pub fn receive(&self) -> Result<Option<Value>> {
        match self.read.try_recv() {
            Ok(message) => message.map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(AiError::ProcessEnded),
        }
    }

    pub fn shutdown(self, final_messages: Vec<Value>) {
        self.stopping.store(true, Ordering::Release);
        let _ = self.write.try_send(Outgoing::Shutdown(final_messages));
        let mut process = self.process;
        // Drain the discarded results during the grace period so a busy
        // writer or server can finish delivering the interrupt acknowledgement.
        let read = self.read;
        let _ = thread::Builder::new()
            .name("codex-shutdown".into())
            .spawn(move || {
                let deadline = Instant::now() + DISABLE_GRACE;
                while process
                    .child
                    .try_wait()
                    .is_ok_and(|status| status.is_none())
                {
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    if read.recv_timeout(remaining).is_err() {
                        break;
                    }
                }
            });
    }
}

fn discover_cli() -> Option<PathBuf> {
    let executable = format!("codex{}", env::consts::EXE_SUFFIX);
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(&executable))
        .chain(
            cfg!(target_os = "macos")
                .then(|| PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex")),
        )
        .find(|candidate| candidate.is_file())
}

fn launch_error(error: io::Error) -> AiError {
    match error.kind() {
        io::ErrorKind::NotFound => AiError::MissingCli,
        _ => AiError::StartFailed,
    }
}

fn write_message(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn read_messages(stdout: impl Read, incoming: SyncSender<Result<Value>>) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        line.clear();
        let result = match (&mut reader)
            .take(MAX_LINE_BYTES)
            .read_until(b'\n', &mut line)
        {
            Ok(0) | Err(_) => Err(AiError::ProcessEnded),
            Ok(_) if line.last() != Some(&b'\n') => Err(AiError::Protocol),
            Ok(_) => serde_json::from_slice(&line).map_err(|_| AiError::Protocol),
        };
        let stop = result.is_err();
        if incoming.send(result).is_err() || stop {
            break;
        }
    }
}
