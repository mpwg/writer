use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
};

use tempfile::TempDir;

pub fn mock_executable() -> &'static Path {
    static EXECUTABLE: OnceLock<(TempDir, PathBuf)> = OnceLock::new();
    &EXECUTABLE
        .get_or_init(|| {
            let directory = TempDir::new().unwrap();
            let executable = directory
                .path()
                .join(format!("fixture{}", std::env::consts::EXE_SUFFIX));
            let mut compiler = Command::new(option_env!("RUSTC").unwrap_or("rustc"))
                .args(["--edition=2024", "-", "-o"])
                .arg(&executable)
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            compiler
                .stdin
                .take()
                .unwrap()
                .write_all(FIXTURE.as_bytes())
                .unwrap();
            assert!(compiler.wait().unwrap().success());
            (directory, executable)
        })
        .1
}

// This executable implements the wire boundary. It uses only std so it can
// be compiled and run by the same Cargo test on all desktop targets.
const FIXTURE: &str = r##"
use std::{fs, io::{self, BufRead, Write}, path::PathBuf, thread, time::Duration};

fn event(method: &str, params: &str) {
    println!("{}", r#"{"method":"METHOD","params":PARAMS}"#
        .replace("METHOD", method).replace("PARAMS", params));
}

fn response(id: &str, result: &str) {
    println!("{{\"id\":{id},\"result\":{result}}}");
}

fn complete_turn(turn: &str, reply: &str) {
    event("turn/completed", &format!(r#"{{"threadId":"thread-valid","turn":{{"id":"{turn}","status":"completed","items":[{{"type":"agentMessage","id":"fixture-answer","text":{reply:?},"phase":"final_answer"}}],"error":null}}}}"#));
}

fn reply_from_file(codex_home: &std::path::Path, kind: &str, turn: &str, request: &str) {
    let path = codex_home.join(format!("{kind}-request"));
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, request).unwrap();
    fs::rename(temporary, path).unwrap();
    let answer = codex_home.join(format!("{kind}-response"));
    let start = std::time::Instant::now();
    while !answer.exists() {
        assert!(start.elapsed() < Duration::from_secs(10), "missing {kind} fixture response");
        thread::sleep(Duration::from_millis(5));
    }
    let reply = fs::read_to_string(answer).unwrap();
    complete_turn(turn, &reply);
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    for argument in ["app-server", "stdio://", "features.shell_tool=false",
                     "features.plugins=false", "features.hooks=false", "web_search=\"disabled\""] {
        assert!(args.iter().any(|arg| arg == argument), "missing {argument}");
    }
    let codex_home = PathBuf::from(std::env::var_os("CODEX_HOME").unwrap());
    let mut initialized = false;
    let mut consistency_count = 0;
    let mut signed_in = false;
    let mut login_count = 0;
    let mut turn_count = 0;
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let method = line.split("\"method\":\"").nth(1).and_then(|tail| tail.split('"').next());
        let id = line.split("\"id\":").nth(1).and_then(|tail| tail.split([',', '}']).next()).unwrap_or("");
        match method {
            Some("initialize") => {
                response("999", "null");
                let mut out = io::stdout().lock();
                write!(out, "{{\"id\":{id},\"res").unwrap();
                out.flush().unwrap();
                thread::sleep(Duration::from_millis(5));
                writeln!(out, "{}", r#"ult":{"userAgent":"fixture","codexHome":"/fixture","platformFamily":"unix","platformOs":"linux"}}"#).unwrap();
            }
            Some("initialized") => initialized = true,
            Some("account/read") => {
                assert!(initialized);
                if signed_in {
                    response(id, r#"{"account":{"type":"chatgpt","email":null,"planType":"pro"},"requiresOpenaiAuth":true}"#);
                } else {
                    response(id, r#"{"account":null,"requiresOpenaiAuth":true}"#);
                }
            }
            Some("account/login/start") => {
                assert!(line.contains("\"type\":\"chatgpt\""));
                login_count += 1;
                response(id, r#"{"type":"chatgpt","loginId":"login-valid","authUrl":"https://auth.openai.com/oauth/authorize?test=yes"}"#);
                event("account/login/completed", r#"{"loginId":"wrong-login","success":false,"error":"wrong response"}"#);
                // A second request inside the same receive loop lets the caller
                // drive cancellation. Other cases inject completion via a flag.
                if !codex_home.join("cancel").exists() {
                    signed_in = true;
                    event("account/login/completed", r#"{"loginId":"login-valid","success":true,"error":null}"#);
                }
            }
            Some("account/login/cancel") => {
                assert_eq!(login_count, 1);
                assert!(line.contains("\"loginId\":\"login-valid\""));
                response(id, r#"{"status":"canceled"}"#);
                event("account/login/completed", r#"{"loginId":"login-valid","success":true,"error":null}"#);
            }
            Some("thread/start" | "thread/resume") => {
                assert!(signed_in);
                assert!(line.contains("\"approvalPolicy\":\"never\""));
                assert!(line.contains("\"sandbox\":\"read-only\""));
                assert!(line.contains("\"modelProvider\":\"openai\""));
                if method == Some("thread/resume") && line.contains("\"threadId\":\"missing\"") {
                    println!("{{\"id\":{id},\"error\":{{\"code\":-32600,\"message\":\"thread not found\"}}}}");
                    continue;
                }
                if method == Some("thread/resume") {
                    assert!(line.contains("\"threadId\":\"thread-valid\""));
                    assert!(line.contains("\"excludeTurns\":true"));
                }
                response(id, r#"{"thread":{"id":"thread-valid"},"sandbox":{"type":"readOnly","networkAccess":false},"approvalPolicy":"never","modelProvider":"openai"}"#);
            }
            Some("turn/start") => {
                turn_count += 1;
                assert!(line.contains("\"threadId\":\"thread-valid\""));
                assert!(line.contains("\"approvalPolicy\":\"never\""));
                assert!(line.contains("\"networkAccess\":false"));
                assert!(line.contains("\"type\":\"readOnly\""));
                if line.contains("Prozess beenden") { return; }
                let consistency = line.contains("Prüfe die nummerierten Aussagen auf inhaltliche Widersprüche");
                let empty_details = line.contains("Finde belegte Ergänzungen für bestehende Zielnotizen")
                    && codex_home.join("empty-detail-replies").exists();
                let numbered_turn = format!("turn-{turn_count}");
                let turn_id = if consistency || empty_details {
                    &numbered_turn
                } else if turn_count == 1 {
                    "turn-valid"
                } else {
                    "turn-second"
                };
                response(id, &format!(r#"{{"turn":{{"id":"{turn_id}","status":"inProgress","items":[],"error":null}}}}"#));
                if consistency {
                    consistency_count += 1;
                    reply_from_file(&codex_home, &format!("consistency-{consistency_count}"), turn_id, &line);
                    continue;
                }
                if empty_details {
                    complete_turn(turn_id, "[]");
                    continue;
                }
                if line.contains("Finde belegte Ergänzungen für bestehende Zielnotizen") {
                    assert_eq!(turn_count, 1, "repeated detail request for an unchanged scene");
                    reply_from_file(&codex_home, "detail", turn_id, &line);
                    continue;
                }
                if line.contains("Formuliere die Auswahl als Prosa aus") {
                    reply_from_file(&codex_home, "draft", turn_id, &line);
                    continue;
                }
                if line.contains("Beantworte die Buchfrage ausschließlich") {
                    reply_from_file(&codex_home, "research", turn_id, &line);
                    continue;
                }
                if line.contains("Fasse die geprüften Textquellen") {
                    reply_from_file(&codex_home, "research-summary", turn_id, &line);
                    continue;
                }
                if line.contains("Späte Antwort") {
                    event("item/agentMessage/delta", r#"{"threadId":"thread-valid","turnId":"turn-valid","itemId":"item","delta":"Beginn"}"#);
                    continue;
                }
                if turn_count == 1 {
                    assert!(line.contains("Was sieht die Figur?"));
                    assert!(line.contains("Der Leuchtturm ist dunkel."));
                    event("item/agentMessage/delta", r#"{"threadId":"wrong-thread","turnId":"turn-valid","itemId":"item","delta":"falsch"}"#);
                    event("item/agentMessage/delta", r#"{"threadId":"thread-valid","turnId":"wrong-turn","itemId":"item","delta":"falsch"}"#);
                    event("item/agentMessage/delta", r#"{"threadId":"thread-valid","turnId":"turn-valid","itemId":"item","delta":"Die Küste."}"#);
                    event("turn/completed", r#"{"threadId":"thread-valid","turn":{"id":"wrong-turn","status":"completed","items":[],"error":null}}"#);
                    event("turn/completed", r#"{"threadId":"thread-valid","turn":{"id":"turn-valid","status":"completed","items":[{"type":"agentMessage","id":"item","text":"Die Küste.","phase":"final_answer"}],"error":null}}"#);
                } else {
                    event("turn/completed", r#"{"threadId":"thread-valid","turn":{"id":"turn-second","status":"failed","items":[],"error":{"codexErrorInfo":"usageLimitExceeded","message":"quota","additionalDetails":null}}}"#);
                }
            }
            Some("turn/interrupt") => {
                assert!(line.contains("\"threadId\":\"thread-valid\""));
                assert!(line.contains("\"turnId\":\"turn-valid\""));
                fs::write(codex_home.join("interrupted"), "yes").unwrap();
                response(id, "{}");
                event("turn/completed", r#"{"threadId":"thread-valid","turn":{"id":"turn-valid","status":"completed","items":[{"type":"agentMessage","id":"item","text":"Spät"}],"error":null}}"#);
            }
            _ => panic!("unexpected stdio message: {line}"),
        }
    }
}
"##;
