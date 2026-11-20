pub mod brainstorm;
pub mod consistency;
pub mod details;
pub mod draft;
pub mod research;
mod server;
mod session;

use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::Id;

pub use session::AiSession;

pub type Result<T> = std::result::Result<T, AiError>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    #[error("KI ist für dieses Projekt ausgeschaltet.")]
    Disabled,
    #[error("Codex ist noch nicht mit ChatGPT verbunden.")]
    NotReady,
    #[error("Die Codex-CLI wurde nicht gefunden. Wähle die ausführbare Codex-Datei.")]
    MissingCli,
    #[error("Der lokale Codex-Prozess konnte nicht gestartet werden.")]
    StartFailed,
    #[error("Der lokale Codex-Prozess wurde beendet.")]
    ProcessEnded,
    #[error("Die Codex-CLI hat unerwartet geantwortet. Prüfe die installierte Version.")]
    Protocol,
    #[error("Codex hat nicht rechtzeitig geantwortet.")]
    Timeout,
    #[error("Es läuft bereits eine Anfrage.")]
    Busy,
    #[error("Die Anfrage enthält keine Frage.")]
    EmptyPrompt,
    #[error("Die Anfrage enthält zu viel Text. Kürze die Frage oder den ausgewählten Kontext.")]
    ContextTooLarge,
    #[error("Die Antwort von Codex ist zu lang.")]
    ResponseTooLarge,
    #[error("Die lokale KI-Freigabe ist nicht verfügbar.")]
    LocalPreferences,
    #[error("Die Anmeldung wurde abgebrochen.")]
    LoginCancelled,
    #[error("Die Anmeldung bei ChatGPT ist fehlgeschlagen.")]
    LoginFailed,
    #[error("Das Codex-Kontingent ist momentan ausgeschöpft.")]
    QuotaExceeded,
    #[error("Die Anfrage wurde unterbrochen.")]
    Interrupted,
    #[error("Der bisherige Codex-Thread konnte nicht wieder geöffnet werden.")]
    ThreadUnavailable,
    #[error("Codex konnte die Anfrage nicht abschließen.")]
    RequestFailed,
    #[error("Codex hat keinen Antworttext geliefert.")]
    EmptyResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiStatus {
    Disabled,
    Connecting,
    SignInRequired,
    SigningIn,
    Ready,
    Unavailable(AiError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiEvent {
    LoginUrl(String),
    ConnectionError(AiError),
    Delta {
        request_id: RequestId,
        text: String,
    },
    Completed {
        request_id: RequestId,
        text: String,
        thread_id: Option<String>,
    },
    Failed {
        request_id: RequestId,
        error: AiError,
    },
}

enum History {
    Ephemeral,
    Keep,
    Resume(String),
}

pub struct AiRequest {
    prompt: String,
    context: Option<String>,
    history: History,
}

impl AiRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            context: None,
            history: History::Ephemeral,
        }
    }

    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Retain the Codex thread in local Codex history so it can be resumed.
    #[must_use]
    pub fn with_thread_history(mut self) -> Self {
        self.history = History::Keep;
        self
    }

    /// Continue a thread returned for this conversation by an earlier request.
    #[must_use]
    pub fn resume(mut self, thread_id: impl Into<String>) -> Self {
        self.history = History::Resume(thread_id.into());
        self
    }
}

pub struct AiPreferences {
    directory: PathBuf,
}

impl AiPreferences {
    pub fn load() -> Result<Self> {
        let home = |key| {
            env::var_os(key)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let base = if cfg!(windows) {
            home("LOCALAPPDATA")
        } else if cfg!(target_os = "macos") {
            home("HOME").map(|path| path.join("Library").join("Application Support"))
        } else {
            home("XDG_CONFIG_HOME").or_else(|| home("HOME").map(|path| path.join(".config")))
        };
        base.map(|path| Self::at(path.join("Schreibwerkstatt")))
            .ok_or(AiError::LocalPreferences)
    }

    pub fn at(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn is_enabled(&self, root: &Path, id: Option<Id>) -> Result<bool> {
        let Some(id) = id else { return Ok(false) };
        let root = canonical_root(root)?;
        Ok(read_roots(&self.consent_path(id))?.contains(&root))
    }

    pub fn set_enabled(&self, root: &Path, id: Option<Id>, enabled: bool) -> Result<()> {
        let Some(id) = id else { return Ok(()) };
        static CONSENT_UPDATE: Mutex<()> = Mutex::new(());
        let _update = CONSENT_UPDATE
            .lock()
            .map_err(|_| AiError::LocalPreferences)?;

        let root = canonical_root(root)?;
        let directory = self.directory.join("ai-consent");
        let path = self.consent_path(id);
        let mut roots = read_roots(&path)?;
        let changed = match roots.iter().position(|entry| *entry == root) {
            Some(index) if !enabled => {
                roots.swap_remove(index);
                true
            }
            None if enabled => {
                roots.push(root);
                true
            }
            _ => false,
        };
        if !changed {
            return Ok(());
        }

        fs::create_dir_all(&directory).map_err(|_| AiError::LocalPreferences)?;
        let mut file =
            tempfile::NamedTempFile::new_in(directory).map_err(|_| AiError::LocalPreferences)?;
        serde_json::to_writer(&mut file, &roots).map_err(|_| AiError::LocalPreferences)?;
        file.flush().map_err(|_| AiError::LocalPreferences)?;
        file.persist(path).map_err(|_| AiError::LocalPreferences)?;
        Ok(())
    }

    fn consent_path(&self, id: Id) -> PathBuf {
        self.directory.join("ai-consent").join(format!("{id}.json"))
    }

    fn codex_home(&self) -> PathBuf {
        self.directory.join("codex")
    }
}

fn read_roots(path: &Path) -> Result<Vec<PathBuf>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(AiError::LocalPreferences),
    };
    serde_json::from_reader(file).map_err(|_| AiError::LocalPreferences)
}

fn canonical_root(root: &Path) -> Result<PathBuf> {
    fs::canonicalize(root).map_err(|_| AiError::LocalPreferences)
}
