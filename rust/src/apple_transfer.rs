//! Foreground-only, paired LAN image transfer. Workers own filesystem/import
//! state only, never an SDL host or ares session. No general file-serving API.
use crate::{frontend::catalog, import_job::ImportControl, profile};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_UPLOAD: u64 = 1024 * 1024 * 1024;
const MAX_CLIENTS: usize = 4;
const PAGE: &str = include_str!("apple_transfer.html");
struct State {
    listener: Option<TcpListener>,
    generation: u64,
    token: String,
    code: String,
    paired: bool,
    pairing_expires: Option<Instant>,
    pairing_failures: u32,
    pairing_locked: Option<Instant>,
    closed: bool,
    foreground: bool,
    job: Option<ImportControl>,
    message: String,
    received: u64,
    total: u64,
    revision: u64,
    profiles: Vec<Value>,
    limited: bool,
    launch: Option<Value>,
}
struct Shared {
    root: PathBuf,
    state: Mutex<State>,
    clients: AtomicUsize,
}
pub(crate) struct Transfer(Arc<Shared>);

fn real_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Transfer root must be absolute".into());
    }
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| !m.is_dir() || m.file_type().is_symlink()) {
            return Err("Transfer root contains a link or non-directory".into());
        }
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())
}
fn bounded_json(path: &Path) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 32 * 1024 * 1024
    {
        return Err("Invalid profile JSON".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("Profile JSON too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn profile_at(root: &Path, name: &str) -> Result<profile::Profile, String> {
    if name.len() > 80 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("Invalid profile identity".into());
    }
    let directory = root.join("profiles").join(name);
    let result = profile::Profile {
        manifest: bounded_json(&directory.join("profile.json"))?,
        directory,
    };
    if !profile::valid_profile(&result) {
        return Err("Profile is missing or damaged; upload its image again".into());
    }
    Ok(result)
}
fn games(profile: &profile::Profile) -> Result<Vec<catalog::Game>, String> {
    catalog::decode(&bounded_json(&profile.assets().join("catalog.json"))?)
}
fn display(text: &str, characters: usize) -> String {
    text.chars().take(characters).collect()
}
fn scan(root: &Path) -> (Vec<Value>, bool) {
    let Ok(entries) = fs::read_dir(root.join("profiles")) else {
        return (vec![], false);
    };
    let mut result = Vec::new();
    let mut limited = false;
    let mut bytes = 0;
    // The app owns this directory; bound restart validation to 64 entries.
    for (index, entry) in entries.take(65).enumerate() {
        if index == 64 {
            limited = true;
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(profile) = profile_at(root, &name) else {
            continue;
        };
        let Ok(games) = games(&profile) else {
            continue;
        };
        if games.len() > 64 {
            limited = true;
            continue;
        }
        let rows:Vec<_>=games.iter().map(|g|json!({"id":g.id,"title":display(&g.title,80),
            "playable":!g.rom.is_empty() && g.import_error.is_empty() && g.firmware_required.is_empty(),
            "reason":display(if !g.firmware_required.is_empty() {"Requires external coprocessor firmware"} else {&g.import_error},160)})).collect();
        let row = json!({"id":name,"label":display(&profile.label(),160),"system":if profile.manifest["system"]=="nes" {"nes"}else{"snes"},"games":rows});
        let size = row.to_string().len();
        if bytes + size > 192 * 1024 {
            limited = true;
            continue;
        }
        bytes += size;
        result.push(row);
    }
    result.sort_by_key(|p| p["label"].as_str().unwrap_or_default().to_owned());
    (result, limited)
}
impl Transfer {
    pub(crate) fn new(root: PathBuf) -> Result<Self, String> {
        real_directory(&root)?;
        real_directory(&root.join("profiles"))?;
        let shared = Arc::new(Shared {
            root,
            state: Mutex::new(State {
                listener: None,
                generation: 0,
                token: String::new(),
                code: String::new(),
                paired: false,
                pairing_expires: None,
                pairing_failures: 0,
                pairing_locked: None,
                closed: false,
                foreground: false,
                job: Some(ImportControl::new()),
                message: "Checking imported profiles…".into(),
                received: 0,
                total: 0,
                revision: 1,
                profiles: vec![],
                limited: false,
                launch: None,
            }),
            clients: AtomicUsize::new(0),
        });
        let worker = shared.clone();
        thread::spawn(move || {
            let (profiles, limited) = scan(&worker.root);
            {
                let mut state = worker.state.lock().unwrap();
                state.profiles = profiles;
                state.limited = limited;
                state.job = None;
                state.message = "Ready to upload a Classic .hsqs image".into();
                state.revision += 1;
            }
            loop {
                let connection = {
                    let state = worker.state.lock().unwrap();
                    if state.closed {
                        break;
                    }
                    state
                        .listener
                        .as_ref()
                        .and_then(|l| l.accept().ok())
                        .map(|(stream, _)| (stream, state.generation))
                };
                if let Some((stream, generation)) = connection {
                    if worker.clients.fetch_add(1, Ordering::SeqCst) >= MAX_CLIENTS {
                        worker.clients.fetch_sub(1, Ordering::SeqCst);
                        drop(stream);
                        continue;
                    }
                    let client = worker.clone();
                    thread::spawn(move || {
                        serve(&client, stream, generation);
                        client.clients.fetch_sub(1, Ordering::SeqCst);
                    });
                } else {
                    thread::sleep(Duration::from_millis(30));
                }
            }
        });
        Ok(Self(shared))
    }
    pub(crate) fn enable(&self, token: &str, code: &str) -> Result<u16, String> {
        self.listen(token, code, false)
    }
    pub(crate) fn renew(&self, token: &str, code: &str) -> Result<u16, String> {
        self.listen(token, code, true)
    }
    fn listen(&self, token: &str, code: &str, renew: bool) -> Result<u16, String> {
        if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid pairing token".into());
        }
        if code.len() != 6 || !code.bytes().all(|c| c.is_ascii_digit()) {
            return Err("Pairing needs six digits".into());
        }
        let mut s = self.0.state.lock().unwrap();
        if s.closed {
            return Err("Transfer service closed".into());
        }
        if renew && (!s.foreground || s.job.is_some()) {
            return Err(
                "Wait for the current import to finish before requesting a new code".into(),
            );
        }
        s.foreground = true;
        if !renew && let Some(listener) = &s.listener {
            return Ok(listener.local_addr().map_err(|e| e.to_string())?.port());
        }
        // Refresh credentials without changing the browser's address. The old
        // page can then enter the new PIN instead of needing a new URL/tab.
        let port = if let Some(listener) = &s.listener {
            listener.local_addr().map_err(|e| e.to_string())?.port()
        } else {
            let listener =
                TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|e| e.to_string())?;
            listener.set_nonblocking(true).map_err(|e| e.to_string())?;
            let port = listener.local_addr().map_err(|e| e.to_string())?.port();
            s.listener = Some(listener);
            port
        };
        s.token = token.to_owned();
        s.code = code.to_owned();
        s.paired = false;
        s.pairing_expires = Some(Instant::now() + Duration::from_secs(600));
        s.pairing_failures = 0;
        s.pairing_locked = None;
        if renew {
            s.launch = None;
        }
        s.generation += 1;
        Ok(port)
    }
    pub(crate) fn disable(&self) {
        let mut s = self.0.state.lock().unwrap();
        s.listener = None;
        s.foreground = false;
        s.token.clear();
        s.code.clear();
        s.paired = false;
        s.pairing_expires = None;
        s.pairing_locked = None;
        s.pairing_failures = 0;
        s.generation += 1;
        s.launch = None;
        if let Some(job) = &s.job {
            job.cancel();
        }
    }
    pub(crate) fn snapshot(&self) -> Value {
        snapshot(&self.0)
    }
    pub(crate) fn cancel(&self) {
        if let Some(job) = &self.0.state.lock().unwrap().job {
            job.cancel();
        }
    }
    pub(crate) fn take_launch(&self) -> Option<Value> {
        self.0.state.lock().unwrap().launch.take()
    }
    pub(crate) fn prepare(&self, profile: String, game: String) -> Result<(), String> {
        let control = begin(&self.0, None)?;
        self.0.state.lock().unwrap().message = "Checking selected game…".into();
        let shared = self.0.clone();
        thread::spawn(move || {
            let result = (|| {
                let p = profile_at(&shared.root, &profile)?;
                let games = games(&p)?;
                let game = games
                    .iter()
                    .find(|g| g.id == game)
                    .ok_or("Game is no longer in the profile")?;
                if game.rom.is_empty()
                    || !game.import_error.is_empty()
                    || !game.firmware_required.is_empty()
                {
                    return Err("Game is unavailable or requires external firmware".into());
                }
                let expected = format!(
                    "roms/{}.{}",
                    game.id,
                    if p.manifest["system"] == "nes" {
                        "nes"
                    } else {
                        "sfc"
                    }
                );
                if game.rom != expected {
                    return Err("Unsafe catalog ROM path".into());
                }
                if p.manifest["files"]
                    .get(format!("assets/{}", game.rom))
                    .and_then(Value::as_str)
                    .is_none()
                {
                    return Err("Selected ROM is not in the verified asset manifest".into());
                }
                let rom = p.assets().join(&game.rom);
                let saves = p.data().join("saves").join(&game.id);
                if control.state() != crate::frontend::import_lifecycle::State::Created {
                    return Err("Open cancelled".into());
                }
                real_directory(&saves)?;
                Ok(
                    json!({"action":"load-profile","rom":rom,"saves":saves,"firmware":p.assets().join("firmware")}),
                )
            })();
            let refreshed = result.as_ref().err().map(|_| scan(&shared.root));
            let mut s = shared.state.lock().unwrap();
            if let Some((profiles, limited)) = refreshed {
                s.profiles = profiles;
                s.limited = limited;
            }
            if s.foreground
                && !matches!(
                    control.state(),
                    crate::frontend::import_lifecycle::State::Cancelled
                        | crate::frontend::import_lifecycle::State::CancelRequested
                )
            {
                match result {
                    Ok(command) => {
                        s.launch = Some(command);
                        s.message = "Selected Classic game ready".into();
                    }
                    Err(error) => {
                        s.launch = Some(json!({"action":"classic-error","error":error}));
                        s.message = error;
                    }
                }
            }
            s.job = None;
            s.revision += 1;
        });
        Ok(())
    }
    pub(crate) fn prepare_collection(&self, identity: String) -> Result<(), String> {
        let control = begin(&self.0, None)?;
        let shared = self.0.clone();
        shared.state.lock().unwrap().message = "Checking Classic collection…".into();
        thread::spawn(move || {
            let result=profile_at(&shared.root,&identity).map(|profile|json!({"action":"open-classic","profile":identity,
                "assets":profile.assets(),"data":profile.data(),"cache":shared.root.join("profiles"),"home":shared.root}));
            let mut s = shared.state.lock().unwrap();
            if s.foreground && control.state() == crate::frontend::import_lifecycle::State::Created
            {
                match result {
                    Ok(command) => {
                        s.launch = Some(command);
                        s.message = "Classic collection ready".into();
                    }
                    Err(error) => {
                        s.launch = Some(json!({"action":"classic-error","error":error}));
                        s.message = error;
                    }
                }
            }
            s.job = None;
            s.revision += 1;
        });
        Ok(())
    }
}
impl Drop for Transfer {
    fn drop(&mut self) {
        self.disable();
        self.0.state.lock().unwrap().closed = true;
    }
}
fn snapshot(shared: &Shared) -> Value {
    let s = shared.state.lock().unwrap();
    json!({"enabled":s.listener.is_some(),"busy":s.job.is_some(),"message":s.message,
        "received":s.received,"total":s.total,"revision":s.revision,"profiles":s.profiles,"limited":s.limited,
        "paired":s.paired,"pairing_seconds_remaining":seconds_left(s.pairing_expires),"pairing_locked_seconds":seconds_left(s.pairing_locked)})
}
fn seconds_left(until: Option<Instant>) -> u64 {
    until
        .map(|t| t.saturating_duration_since(Instant::now()).as_secs())
        .unwrap_or(0)
}
fn pair(shared: &Shared, generation: u64, code: &[u8]) -> Result<String, (u16, &'static str)> {
    let mut s = shared.state.lock().unwrap();
    let now = Instant::now();
    if s.listener.is_none() || s.closed || s.generation != generation {
        return Err((403, "Pairing session ended. Use the current TV code."));
    }
    if s.pairing_expires.is_none_or(|t| t <= now) {
        return Err((410, "Code expired. Select New code on the TV."));
    }
    if s.pairing_locked.is_some_and(|t| t > now) {
        return Err((
            429,
            "Too many attempts. Wait one minute or select New code on the TV.",
        ));
    }
    // A global budget cannot be bypassed by changing IP addresses. Successful
    // pairing exchanges the short PIN for a separate 128-bit bearer credential.
    if s.pairing_locked.take().is_some() {
        s.pairing_failures = 0;
    }
    if code != s.code.as_bytes() {
        s.pairing_failures += 1;
        if s.pairing_failures >= 5 {
            s.pairing_locked = Some(now + Duration::from_secs(60));
            return Err((
                429,
                "Too many attempts. Wait one minute or select New code on the TV.",
            ));
        }
        return Err((403, "Incorrect code. Enter the six digits shown on the TV."));
    }
    s.pairing_failures = 0;
    s.paired = true;
    Ok(s.token.clone())
}
fn begin(shared: &Shared, generation: Option<u64>) -> Result<ImportControl, String> {
    let mut s = shared.state.lock().unwrap();
    if !s.foreground || s.closed || (generation.is_some() && s.listener.is_none()) {
        return Err("Transfers are disabled".into());
    }
    if generation.is_some_and(|g| g != s.generation) {
        return Err("Pairing session expired".into());
    }
    if s.job.is_some() {
        return Err("Another transfer or validation is still finishing".into());
    }
    let job = ImportControl::new();
    s.job = Some(job.clone());
    s.received = 0;
    s.total = 0;
    s.launch = None;
    s.message = "Receiving image…".into();
    s.revision += 1;
    Ok(job)
}
struct Request {
    method: String,
    path: String,
    host: String,
    token: String,
    origin: Option<String>,
    length: u64,
}
fn request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut bytes = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 8192 || Instant::now() > deadline {
            return Err("Header limit".into());
        }
        let mut byte = [0];
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        bytes.push(byte[0]);
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let mut lines = text.split("\r\n");
    let first: Vec<_> = lines.next().unwrap_or_default().split(' ').collect();
    if first.len() != 3 || first[2] != "HTTP/1.1" {
        return Err("HTTP/1.1 required".into());
    }
    let mut headers = std::collections::BTreeMap::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (name, value) = line.split_once(':').ok_or("Malformed header")?;
        let name = name.to_ascii_lowercase();
        if headers.insert(name, value.trim().to_owned()).is_some() {
            return Err("Duplicate header".into());
        }
    }
    if headers.contains_key("transfer-encoding") || headers.contains_key("expect") {
        return Err("Chunked/Expect uploads are unsupported".into());
    }
    let length = match headers.get("content-length") {
        Some(v) if !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) => {
            v.parse().map_err(|_| "Invalid length")?
        }
        None => 0,
        _ => return Err("Invalid length".into()),
    };
    Ok(Request {
        method: first[0].into(),
        path: first[1].into(),
        host: headers.remove("host").ok_or("Host required")?,
        token: headers.remove("authorization").unwrap_or_default(),
        origin: headers.remove("origin"),
        length,
    })
}
fn respond(stream: &mut TcpStream, code: u16, kind: &str, body: &str) {
    let header = format!(
        "HTTP/1.1 {code} Response\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; form-action 'none'; frame-ancestors 'none'\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}
fn reject(stream: &mut TcpStream, req: &Request, code: u16, message: &str) {
    // Consume only a small rejected body. Closing a Darwin socket with unread
    // request bytes can reset it before the browser receives the error reply.
    // Never drain an unauthenticated image-sized request.
    if req.length <= 8192 {
        let mut body = [0; 8192];
        let _ = stream.read_exact(&mut body[..req.length as usize]);
    }
    respond(stream, code, "text/plain", message);
}
fn allowed_host(host: &str, port: u16) -> bool {
    let Some((ip, p)) = host.rsplit_once(':') else {
        return false;
    };
    let Ok(ip) = ip.parse::<Ipv4Addr>() else {
        return false;
    };
    p.parse::<u16>() == Ok(port) && (ip.is_private() || ip.is_loopback() || ip.is_link_local())
}
fn live(shared: &Shared, generation: u64) -> bool {
    let s = shared.state.lock().unwrap();
    s.listener.is_some() && s.generation == generation && !s.closed
}
fn serve(shared: &Arc<Shared>, mut stream: TcpStream, generation: u64) {
    // Darwin inherits O_NONBLOCK from the listener on accepted sockets.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let Ok(req) = request(&mut stream) else {
        respond(
            &mut stream,
            400,
            "text/plain",
            "Malformed or unsupported HTTP request",
        );
        return;
    };
    let port = stream.local_addr().map(|a| a.port()).unwrap_or(0);
    if !live(shared, generation) {
        respond(&mut stream, 503, "text/plain", "Transfers are disabled");
        return;
    }
    if !allowed_host(&req.host, port)
        || req
            .origin
            .as_ref()
            .is_some_and(|o| *o != format!("http://{}", req.host))
    {
        reject(
            &mut stream,
            &req,
            403,
            "Use the local IP address shown on the TV",
        );
        return;
    }
    if req.method == "GET" && req.path == "/" && req.length == 0 {
        respond(&mut stream, 200, "text/html; charset=utf-8", PAGE);
        return;
    }
    if req.method == "POST" && req.path == "/pair" {
        if req.length != 6 {
            respond(&mut stream, 400, "text/plain", "Enter exactly six digits");
            return;
        }
        let mut code = [0; 6];
        if stream.read_exact(&mut code).is_err() {
            respond(&mut stream, 400, "text/plain", "Incomplete code");
            return;
        }
        match pair(shared, generation, &code) {
            Ok(token) => respond(
                &mut stream,
                200,
                "application/json",
                &json!({"token":token}).to_string(),
            ),
            Err((status, message)) => respond(&mut stream, status, "text/plain", message),
        }
        return;
    }
    let authorized = {
        let s = shared.state.lock().unwrap();
        s.paired
            && s.generation == generation
            && s.listener.is_some()
            && req.token == format!("Bearer {}", s.token)
    };
    if !authorized {
        reject(&mut stream, &req, 403, "Pair with the code shown on the TV");
        return;
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/status") if req.length == 0 => respond(
            &mut stream,
            200,
            "application/json",
            &snapshot(shared).to_string(),
        ),
        ("POST", "/cancel") if req.length == 0 => {
            let s = shared.state.lock().unwrap();
            if s.generation == generation
                && let Some(job) = &s.job
            {
                job.cancel();
            }
            drop(s);
            respond(&mut stream, 200, "text/plain", "Cancellation requested");
        }
        ("POST", "/upload") if req.length > 0 && req.length <= MAX_UPLOAD => {
            let control = match begin(shared, Some(generation)) {
                Ok(c) => c,
                Err(e) => {
                    respond(&mut stream, 409, "text/plain", &e);
                    return;
                }
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                receive(shared, &mut stream, Some(generation), req.length, &control)
            }))
            .unwrap_or_else(|_| Err("Image import failed unexpectedly".into()));
            let mut s = shared.state.lock().unwrap();
            match &result {
                Ok(()) => s.message = "Profile ready — select a collection in Clover".into(),
                Err(e) => s.message = e.clone(),
            }
            s.job = None;
            s.revision += 1;
            drop(s);
            respond(
                &mut stream,
                if result.is_ok() { 200 } else { 422 },
                "text/plain",
                result
                    .as_ref()
                    .err()
                    .map(String::as_str)
                    .unwrap_or("Profile imported"),
            );
        }
        ("POST", "/upload") => respond(
            &mut stream,
            413,
            "text/plain",
            "Upload must be between 1 byte and 1 GiB",
        ),
        _ => respond(&mut stream, 404, "text/plain", "Not found"),
    }
}
fn receive(
    shared: &Shared,
    stream: &mut impl Read,
    generation: Option<u64>,
    length: u64,
    control: &ImportControl,
) -> Result<(), String> {
    let serial = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = shared.root.join(format!(
        ".upload-{}-{}-{serial}.hsqs",
        generation.unwrap_or(0),
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    struct Owned(PathBuf);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _owned = Owned(path.clone());
    shared.state.lock().unwrap().total = length;
    (|| {
        let mut remaining = length;
        let mut buffer = [0; 65536];
        let deadline = Instant::now() + Duration::from_secs(15 * 60);
        while remaining > 0 {
            let active = if let Some(generation) = generation {
                live(shared, generation)
            } else {
                let state = shared.state.lock().unwrap();
                state.foreground && !state.closed
            };
            if !active
                || control.state() != crate::frontend::import_lifecycle::State::Created
                || Instant::now() > deadline
            {
                return Err("Upload cancelled or expired".into());
            }
            let maximum = remaining.min(buffer.len() as u64) as usize;
            let n = stream
                .read(&mut buffer[..maximum])
                .map_err(|e| format!("Upload interrupted: {e}"))?;
            if n == 0 {
                return Err("Incomplete upload".into());
            }
            file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            remaining -= n as u64;
            shared.state.lock().unwrap().received = length - remaining;
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        // Reject boot-image companions and archive ambiguity: this endpoint is
        // specifically for a complete SquashFS image, not arbitrary file writes.
        let mut magic = [0; 4];
        File::open(&path)
            .map_err(|e| e.to_string())?
            .read_exact(&mut magic)
            .map_err(|e| e.to_string())?;
        if &magic != b"hsqs" {
            return Err("Choose a SquashFS .hsqs filesystem image, not a boot .img or ROM".into());
        }
        let mut progress = |message: &str| {
            shared.state.lock().unwrap().message = message.to_owned();
        };
        profile::import_image_cancellable(
            &path,
            &shared.root.join("profiles"),
            Some(&mut progress),
            control,
        )
        .map_err(|e| e.to_string())?;
        let (profiles, limited) = scan(&shared.root);
        let mut state = shared.state.lock().unwrap();
        state.profiles = profiles;
        state.limited = limited;
        Ok(())
    })()
}

// Private C ABI. Handles are exclusively owned by the main-thread UIKit leaf;
// worker Arc clones outlive shutdown safely without accessing the handle again.
use std::ffi::{CStr, c_char, c_void};
unsafe fn text<'a>(p: *const c_char) -> Result<&'a str, String> {
    if p.is_null() {
        return Err("Null string".into());
    }
    unsafe { CStr::from_ptr(p) }
        .to_str()
        .map_err(|e| e.to_string())
}
unsafe fn handle<'a>(p: *mut c_void) -> Result<&'a Transfer, String> {
    unsafe { p.cast::<Transfer>().as_ref() }.ok_or("Null transfer handle".into())
}
fn out(p: *mut c_char, size: usize, value: &str) -> Result<(), String> {
    if p.is_null() || value.len() >= size {
        return Err("Short transfer output buffer".into());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(value.as_ptr(), p.cast(), value.len());
        *p.add(value.len()) = 0;
    }
    Ok(())
}
fn ffi(error: *mut c_char, size: usize, f: impl FnOnce() -> Result<(), String>) -> i32 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(())) => 0,
        other => {
            let message = match other {
                Ok(Err(e)) => e,
                _ => "Transfer panic".into(),
            };
            let _ = out(error, size, &message);
            1
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_create(
    root: *const c_char,
    error: *mut c_char,
    size: usize,
) -> *mut c_void {
    let mut result = std::ptr::null_mut();
    ffi(error, size, || {
        result = Box::into_raw(Box::new(Transfer::new(PathBuf::from(unsafe {
            text(root)?
        }))?))
        .cast();
        Ok(())
    });
    result
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_enable(
    p: *mut c_void,
    token: *const c_char,
    code: *const c_char,
    port: *mut u16,
    error: *mut c_char,
    size: usize,
) -> i32 {
    ffi(error, size, || {
        if port.is_null() {
            return Err("Null port".into());
        }
        unsafe {
            *port = handle(p)?.enable(text(token)?, text(code)?)?;
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_renew(
    p: *mut c_void,
    token: *const c_char,
    code: *const c_char,
    port: *mut u16,
    error: *mut c_char,
    size: usize,
) -> i32 {
    ffi(error, size, || {
        if port.is_null() {
            return Err("Null port".into());
        }
        unsafe {
            *port = handle(p)?.renew(text(token)?, text(code)?)?;
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_snapshot(
    p: *mut c_void,
    json: *mut c_char,
    length: usize,
    error: *mut c_char,
    size: usize,
) -> i32 {
    ffi(error, size, || {
        out(json, length, &unsafe { handle(p)? }.snapshot().to_string())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_prepare(
    p: *mut c_void,
    profile: *const c_char,
    game: *const c_char,
    error: *mut c_char,
    size: usize,
) -> i32 {
    ffi(error, size, || unsafe {
        handle(p)?.prepare(text(profile)?.to_owned(), text(game)?.to_owned())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_prepare_collection(
    p: *mut c_void,
    profile: *const c_char,
    error: *mut c_char,
    size: usize,
) -> i32 {
    ffi(error, size, || unsafe {
        handle(p)?.prepare_collection(text(profile)?.to_owned())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_take_launch(
    p: *mut c_void,
    json: *mut c_char,
    length: usize,
    error: *mut c_char,
    size: usize,
) -> i32 {
    ffi(error, size, || {
        out(
            json,
            length,
            &unsafe { handle(p)? }
                .take_launch()
                .unwrap_or(Value::Null)
                .to_string(),
        )
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_disable(p: *mut c_void) {
    if let Ok(h) = unsafe { handle(p) } {
        h.disable();
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_cancel(p: *mut c_void) {
    if let Ok(h) = unsafe { handle(p) } {
        h.cancel();
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_destroy(p: *mut c_void) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(p.cast::<Transfer>()) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_activate(p: *mut c_void) {
    if let Ok(service) = unsafe { handle(p) } {
        let mut s = service.0.state.lock().unwrap();
        if !s.closed {
            s.foreground = true;
        }
    }
}
struct LocalImport {
    shared: Arc<Shared>,
    control: ImportControl,
}
/// # Safety
/// Called with a live main-thread-owned transfer handle. The returned single-use
/// lease, unlike that handle, may be moved to one coordinated reader worker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_local_begin(
    p: *mut c_void,
    error: *mut c_char,
    size: usize,
) -> *mut c_void {
    let mut result = std::ptr::null_mut();
    ffi(error, size, || {
        let service = unsafe { handle(p)? };
        let control = begin(&service.0, None)?;
        result = Box::into_raw(Box::new(LocalImport {
            shared: service.0.clone(),
            control,
        }))
        .cast();
        Ok(())
    });
    result
}
/// # Safety
/// Consumes one live local-import lease and, if nonnegative, an exclusively owned
/// read-only regular-file descriptor. The caller keeps coordinated/security-scoped
/// access alive until this synchronous worker call returns. No host/core access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_transfer_local_run(
    p: *mut c_void,
    fd: i32,
    error: *mut c_char,
    size: usize,
) -> i32 {
    use std::os::fd::FromRawFd;
    if p.is_null() {
        return 1;
    }
    let job = unsafe { Box::from_raw(p.cast::<LocalImport>()) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if fd < 0 {
            return Err("Cannot coordinate/read the selected image".to_owned());
        }
        let mut source = unsafe { File::from_raw_fd(fd) };
        let metadata = source.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_UPLOAD {
            return Err("Choose a regular .hsqs image no larger than 1 GiB".into());
        }
        receive(&job.shared, &mut source, None, metadata.len(), &job.control)
    }))
    .unwrap_or_else(|_| Err("Local image import failed unexpectedly".into()));
    {
        let mut s = job.shared.state.lock().unwrap();
        s.job = None;
        s.revision += 1;
        s.message = result
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| "Profile ready — select a collection".into());
    }
    match result {
        Ok(()) => 0,
        Err(message) => {
            let _ = out(error, size, &message);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";
    const CODE: &str = "012345";
    struct OwnedRoot(PathBuf);
    impl OwnedRoot {
        fn new() -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join(".build/apple-transfer-tests");
            fs::create_dir_all(&root).unwrap();
            let path = root.join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for OwnedRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn ready(service: &Transfer) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while service.snapshot()["busy"] == true {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn call(port: u16, request: String) -> String {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut result = String::new();
        let _ = stream.read_to_string(&mut result);
        result
    }
    fn req(port: u16, method: &str, path: &str, extra: &str, body: &str) -> String {
        call(
            port,
            format!(
                "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\n{extra}Content-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
    }
    #[test]
    fn paired_server_is_narrow_and_rejects_cross_origin_and_rebinding() {
        let root = OwnedRoot::new();
        let service = Transfer::new(root.0.clone()).unwrap();
        let port = service.enable(TOKEN, CODE).unwrap();
        ready(&service);
        assert!(req(port, "GET", "/status", "", "").starts_with("HTTP/1.1 403"));
        assert!(req(port, "POST", "/pair", "", CODE).starts_with("HTTP/1.1 200"));
        assert!(
            req(
                port,
                "POST",
                "/pair",
                "Origin: http://evil.example\r\n",
                CODE
            )
            .starts_with("HTTP/1.1 403")
        );
        assert!(req(port, "GET", "/", "", "").starts_with("HTTP/1.1 200"));
        assert!(req(port, "GET", "/status", "", "").contains("Ready to upload"));
        assert!(
            call(
                port,
                format!("GET /status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")
            )
            .starts_with("HTTP/1.1 403")
        );
        assert!(
            req(
                port,
                "GET",
                "/status",
                "Origin: http://evil.example\r\n",
                ""
            )
            .starts_with("HTTP/1.1 403")
        );
        assert!(
            call(
                port,
                format!("GET / HTTP/1.1\r\nHost: attacker.example:{port}\r\n\r\n")
            )
            .starts_with("HTTP/1.1 403")
        );
        for path in [
            "/../profile.json",
            "/files",
            "/upload?path=../../x",
            "/launch",
        ] {
            assert!(req(port, "GET", path, "", "").starts_with("HTTP/1.1 404"));
        }
        assert!(req(port, "POST", "/upload", "", "bad image").starts_with("HTTP/1.1 422"));
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1); // profiles only, no staged upload.
        assert!(
            service.snapshot()["profiles"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn malformed_or_unbounded_requests_never_begin_import() {
        let root = OwnedRoot::new();
        let service = Transfer::new(root.0.clone()).unwrap();
        let port = service.enable(TOKEN, CODE).unwrap();
        ready(&service);
        assert!(req(port, "POST", "/pair", "", CODE).starts_with("HTTP/1.1 200"));
        for extra in [
            "Transfer-Encoding: chunked\r\n",
            "Expect: 100-continue\r\n",
            "Content-Length: 1\r\n",
            "Authorization: duplicate\r\n",
        ] {
            assert!(req(port, "POST", "/upload", extra, "").starts_with("HTTP/1.1 400"));
        }
        let response = call(
            port,
            format!(
                "POST /upload HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: {}\r\n\r\n",
                MAX_UPLOAD + 1
            ),
        );
        assert!(response.starts_with("HTTP/1.1 413"));
        assert_eq!(service.snapshot()["busy"], false);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }
    #[test]
    fn single_flight_cancel_and_foreground_repairing_do_not_resurrect_uploads() {
        let root = OwnedRoot::new();
        let service = Transfer::new(root.0.clone()).unwrap();
        let port = service.enable(TOKEN, CODE).unwrap();
        ready(&service);
        assert!(req(port, "POST", "/pair", "", CODE).starts_with("HTTP/1.1 200"));
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.write_all(format!("POST /upload HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: 10000\r\n\r\nhsqs").as_bytes()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while service.snapshot()["received"] != 4 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(service.renew(TOKEN, "987654").is_err());
        assert_eq!(service.0.state.lock().unwrap().code, CODE);
        let response = req(port, "POST", "/upload", "", "other");
        assert!(response.starts_with("HTTP/1.1 409"), "{response:?}");
        assert!(req(port, "POST", "/cancel", "", "").starts_with("HTTP/1.1 200"));
        service.disable();
        drop(stream);
        ready(&service);
        assert_eq!(service.snapshot()["enabled"], false);
        let new_port = service
            .enable("fedcba9876543210fedcba9876543210", "987654")
            .unwrap();
        assert!(req(new_port, "GET", "/status", "", "").starts_with("HTTP/1.1 403"));
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
        assert!(service.take_launch().is_none());
    }
    #[test]
    fn invalid_profile_identity_cannot_open_files_outside_the_store() {
        let root = OwnedRoot::new();
        for name in ["../escape", "/absolute", ".", "not-a-profile"] {
            assert!(profile_at(&root.0, name).is_err());
        }
        assert!(!allowed_host("evil.local:8080", 8080));
        assert!(!allowed_host("192.168.1.1:8081", 8080));
        assert!(allowed_host("192.168.1.1:8080", 8080));
        assert!(!allowed_host("8.8.8.8:8080", 8080));
    }

    #[test]
    fn six_digit_pairing_limits_attempts_expires_and_issues_only_long_sessions() {
        let root = OwnedRoot::new();
        let service = Transfer::new(root.0.clone()).unwrap();
        assert!(service.enable(TOKEN, "1234").is_err());
        let port = service.enable(TOKEN, CODE).unwrap();
        ready(&service);
        assert!(req(port, "GET", "/status", "", "").starts_with("HTTP/1.1 403"));
        for _ in 0..4 {
            assert!(req(port, "POST", "/pair", "", "999999").starts_with("HTTP/1.1 403"));
        }
        assert!(req(port, "POST", "/pair", "", "999999").starts_with("HTTP/1.1 429"));
        assert!(req(port, "POST", "/pair", "", CODE).starts_with("HTTP/1.1 429"));
        service.0.state.lock().unwrap().pairing_locked =
            Some(Instant::now() - Duration::from_secs(1));
        let response = req(port, "POST", "/pair", "", CODE);
        assert!(response.starts_with("HTTP/1.1 200") && response.contains(TOKEN));
        assert!(req(port, "GET", "/status", "", "").starts_with("HTTP/1.1 200"));
        let status = call(
            port,
            format!(
                "GET /status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {CODE}\r\n\r\n"
            ),
        );
        assert!(status.starts_with("HTTP/1.1 403"));
        service.0.state.lock().unwrap().pairing_expires =
            Some(Instant::now() - Duration::from_secs(1));
        assert!(req(port, "POST", "/pair", "", CODE).starts_with("HTTP/1.1 410"));
        // A paired browser can finish its transfer after the display PIN expires.
        assert!(req(port, "GET", "/status", "", "").starts_with("HTTP/1.1 200"));
        let next = service
            .renew("fedcba9876543210fedcba9876543210", "987654")
            .unwrap();
        assert_eq!(next, port);
        assert!(req(next, "POST", "/pair", "", CODE).starts_with("HTTP/1.1 403"));
        assert!(req(next, "GET", "/status", "", "").starts_with("HTTP/1.1 403"));
        assert!(req(next, "POST", "/pair", "", "987654").starts_with("HTTP/1.1 200"));
        assert!(req(next, "GET", "/status", "", "").starts_with("HTTP/1.1 403")); // The old long credential stays revoked.
    }

    #[test]
    fn restart_scan_and_fresh_open_reject_changed_profile_assets() {
        let root = OwnedRoot::new();
        let sha = "a".repeat(64);
        let name = format!("{sha}-v{}", profile::IMPORTER_VERSION);
        let directory = root.0.join("profiles").join(&name);
        let assets = directory.join("assets");
        fs::create_dir_all(assets.join("skin")).unwrap();
        fs::create_dir(assets.join("roms")).unwrap();
        fs::write(assets.join("skin/layout.json"), b"{}").unwrap();
        fs::write(assets.join("catalog.json"),json!({"schema":1,"games":[{"id":"GAME-1","title":"Self-authored fixture","rom":"roms/GAME-1.nes"}]}).to_string()).unwrap();
        let mut rom = vec![0; 16 + 16384 + 8192];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        rom[5] = 1;
        fs::write(assets.join("roms/GAME-1.nes"), rom).unwrap();
        let files: serde_json::Map<String, Value> =
            ["skin/layout.json", "catalog.json", "roms/GAME-1.nes"]
                .into_iter()
                .map(|name| {
                    (
                        format!("assets/{name}"),
                        json!(crate::hash::file_digest(&assets.join(name)).unwrap()),
                    )
                })
                .collect();
        fs::write(directory.join("profile.json"),json!({"importerVersion":profile::IMPORTER_VERSION,"sourceSHA256":sha,"system":"nes","files":files}).to_string()).unwrap();
        let service = Transfer::new(root.0.clone()).unwrap();
        service.enable(TOKEN, CODE).unwrap();
        ready(&service);
        assert_eq!(service.snapshot()["profiles"].as_array().unwrap().len(), 1);
        service.disable();
        unsafe {
            clover_transfer_activate(std::ptr::from_ref(&service).cast_mut().cast());
        }
        assert_eq!(service.snapshot()["enabled"], false); // Offline launch needs no server.
        service.prepare_collection(name.clone()).unwrap();
        ready(&service);
        let collection = service.take_launch().unwrap();
        assert_eq!(collection["action"], "open-classic");
        assert_eq!(collection["assets"], assets.to_str().unwrap());
        assert!(collection.get("rom").is_none());
        service.prepare(name.clone(), "GAME-1".into()).unwrap();
        ready(&service);
        let launch = service.take_launch().unwrap();
        assert_eq!(
            launch["rom"],
            assets.join("roms/GAME-1.nes").to_str().unwrap()
        );
        assert_eq!(
            launch["saves"],
            directory.join("data/saves/GAME-1").to_str().unwrap()
        );
        assert!(service.take_launch().is_none());
        fs::write(assets.join("roms/GAME-1.nes"), b"changed").unwrap();
        service.prepare(name, "GAME-1".into()).unwrap();
        ready(&service);
        assert_eq!(service.take_launch().unwrap()["action"], "classic-error");
        assert!(
            service.snapshot()["profiles"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fs::read(assets.join("roms/GAME-1.nes")).unwrap(),
            b"changed"
        );
    }

    #[test]
    fn local_coordinated_lease_survives_owner_shutdown_and_never_mutates_source() {
        use std::{ffi::CString, os::fd::IntoRawFd};
        let root = OwnedRoot::new();
        let source = root.0.join("selected.hsqs");
        fs::write(&source, b"invalid selected image").unwrap();
        let store = root.0.join("store");
        let path = CString::new(store.to_str().unwrap()).unwrap();
        let mut error = [0; 2048];
        unsafe {
            let owner = clover_transfer_create(path.as_ptr(), error.as_mut_ptr(), error.len());
            assert!(!owner.is_null());
            ready(handle(owner).unwrap());
            clover_transfer_activate(owner);
            let lease = clover_transfer_local_begin(owner, error.as_mut_ptr(), error.len());
            assert!(!lease.is_null());
            let fd = File::open(&source).unwrap().into_raw_fd();
            clover_transfer_destroy(owner); // Only the independent Arc-backed job remains.
            assert_eq!(
                clover_transfer_local_run(lease, fd, error.as_mut_ptr(), error.len()),
                1
            );
        }
        assert_eq!(fs::read(&source).unwrap(), b"invalid selected image");
        assert_eq!(fs::read_dir(store).unwrap().count(), 1); // No upload stage was left behind.
    }
}
