//! HTTP/WS host: serves the viewer and the `/ws` data plane.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use holotyp_preview_protocol::{
    Frame, Message as WireMessage, PROTOCOL_VERSION, PagePoint, PreviewSettings, Role,
    SourceSpan,
};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use typst_kit::watcher::Watcher;

use crate::preview::session::{Session, Trigger, Update};

/// Shared host state: one compile pipeline, many viewers.
pub struct AppState {
    pub session: Mutex<Session>,
    pub update_tx: broadcast::Sender<OutFrame>,
    pub next_seq: Mutex<u32>,
    /// Wakes the compile loop: file system events and editor buffers.
    pub trigger_tx: mpsc::UnboundedSender<Trigger>,
    /// `index.html` with this session's settings filled into its template slot.
    pub index_html: Vec<u8>,
    /// One preview serves one editor
    editor_connected: AtomicBool,
}

/// The two halves of one connection, as [`StreamExt::split`] hands them out.
type WsTx = SplitSink<WebSocket, Message>;
type WsRx = SplitStream<WebSocket>;

impl AppState {
    fn router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/", get(Self::viewer_handler))
            .route("/index.html", get(Self::viewer_handler))
            .route("/ws", get(Self::ws_handler))
            .fallback_service(get(Self::static_asset))
            .with_state(self)
    }

    async fn viewer_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let content_type = [(axum::http::header::CONTENT_TYPE, mime_of("index.html"))];
        (content_type, state.index_html.clone()).into_response()
    }

    async fn static_asset(uri: axum::http::Uri) -> impl IntoResponse {
        let path = uri.path().trim_start_matches('/');
        let Some(bytes) = holotyp_preview_viewer::get_file(path) else {
            return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
        };
        ([(axum::http::header::CONTENT_TYPE, mime_of(path))], bytes.to_vec())
            .into_response()
    }

    async fn ws_handler(
        State(state): State<Arc<AppState>>,
        headers: HeaderMap,
        ws: WebSocketUpgrade,
    ) -> impl IntoResponse {
        if let Some(origin) =
            headers.get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok())
            && !trusted_origin(origin)
        {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        ws.on_upgrade(move |socket| state.handle_socket(socket))
            .into_response()
    }

    /// Start one connection: handshake, resync, then run loop to handle incoming messages
    async fn handle_socket(self: Arc<Self>, socket: WebSocket) {
        let (mut ws_tx, mut ws_rx) = socket.split();
        let mut seq = 1_u32;

        if Self::send_hello(&mut ws_tx, &mut seq).await.is_none() {
            return;
        }
        let Some(role) = Self::recv_hello(&mut ws_tx, &mut ws_rx).await else {
            return;
        };
        let editor = role == Role::Editor;
        if editor {
            if self.editor_connected.swap(true, Ordering::SeqCst) {
                let _ = ws_tx.send(Message::Close(None)).await;
                return;
            }
            let _ = self.trigger_tx.send(Trigger::EditorHello);
        }

        let update_rx = self.update_tx.subscribe();
        let Some(resync_cycle) = self.resync(&mut ws_tx, &mut seq, role).await else {
            if editor {
                self.editor_connected.store(false, Ordering::SeqCst);
            }
            return;
        };

        let (close_tx, close_rx) = oneshot::channel();
        let reader = self.spawn_reader(role, ws_rx, close_tx);
        self.write_loop(&mut ws_tx, &mut seq, role, update_rx, resync_cycle, close_rx)
            .await;
        // can't write anymore, kill reader
        reader.abort();
        if editor {
            self.editor_connected.store(false, Ordering::SeqCst);
        }
    }

    /// Send our `Hello`.
    async fn send_hello(ws_tx: &mut WsTx, seq: &mut u32) -> Option<()> {
        let hello = WireMessage::Hello { protocol: PROTOCOL_VERSION, role: Role::Server };
        send(ws_tx, seq, hello).await
    }

    /// Wait for the client's `Hello`.
    async fn recv_hello(ws_tx: &mut WsTx, ws_rx: &mut WsRx) -> Option<Role> {
        while let Some(Ok(message)) = ws_rx.next().await {
            let Message::Binary(bytes) = message else { continue };
            let Some(Frame { message: WireMessage::Hello { protocol, role }, .. }) =
                Frame::from_bytes(&bytes)
            else {
                continue;
            };
            // unexpected protocol version / role
            if protocol != PROTOCOL_VERSION || role == Role::Server {
                let _ = ws_tx.send(Message::Close(None)).await;
                return None;
            }
            return Some(role);
        }
        None
    }

    /// Bring a fresh connection up to date with the current document.
    ///
    /// Returns the cycle the resync belongs to: everything published up to it is already
    /// contained in the frames sent here.
    async fn resync(
        self: &Arc<Self>,
        ws_tx: &mut WsTx,
        seq: &mut u32,
        role: Role,
    ) -> Option<u64> {
        // Rendering a viewer's document is as heavy as a compile, so it runs off the
        // async runtime.
        let this = self.clone();
        let update = tokio::task::spawn_blocking(move || {
            this.session.blocking_lock().resync(role)
        })
        .await
        .ok()?;
        let cycle = update.epoch;
        update.send_through(ws_tx, seq, role).await?;
        Some(cycle)
    }

    /// Reader task. Signals on `close_tx` once the client stops delivering frames.
    fn spawn_reader(
        self: &Arc<Self>,
        role: Role,
        mut ws_rx: WsRx,
        close_tx: oneshot::Sender<()>,
    ) -> JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            while let Some(Ok(message)) = ws_rx.next().await {
                let Message::Binary(bytes) = message else { continue };
                let Some(frame) = Frame::from_bytes(&bytes) else { continue };
                match (role, frame.message) {
                    (Role::Editor, WireMessage::SourceUpdate { file, version, text }) => {
                        let _ = this.trigger_tx.send(Trigger::Overlay {
                            file,
                            version,
                            text,
                        });
                    }
                    (Role::Viewer, WireMessage::DocToSource(point)) => {
                        let session = this.session.lock().await;
                        let hit = session.doc_loc_to_src_loc(
                            point.page as usize,
                            point.x,
                            point.y,
                        );
                        drop(session);
                        if let Some((file, range, _)) = hit {
                            let reply = WireMessage::SourceLocation {
                                span: SourceSpan { file, range },
                            };
                            this.publish_reply(Role::Editor, reply).await;
                        }
                    }
                    (Role::Editor, WireMessage::SourceToDoc { file, line, col }) => {
                        let session = this.session.lock().await;
                        let scroll = session.src_loc_to_doc_loc(&file, line, col);
                        drop(session);
                        if let Some((page, x, y)) = scroll {
                            let reply = WireMessage::ScrollTo(PagePoint {
                                page: page as u32,
                                x,
                                y,
                            });
                            this.publish_reply(Role::Viewer, reply).await;
                        }
                    }
                    // either side may send a free-form debug event
                    (_, WireMessage::Event { level, msg }) => {
                        eprintln!("event {level}: {msg}");
                    }
                    // `Viewport` and the rest are not handled yet
                    _ => {}
                }
            }
            // can't read anymore, stop the writer
            let _ = close_tx.send(());
        })
    }

    /// Writer loop
    async fn write_loop(
        self: &Arc<Self>,
        ws_tx: &mut WsTx,
        seq: &mut u32,
        role: Role,
        mut update_rx: broadcast::Receiver<OutFrame>,
        mut resync_cycle: u64,
        mut close_rx: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                frame = update_rx.recv() => match frame {
                    // cycle 0 is a reply
                    Ok(frame)
                        if (frame.cycle == 0 || frame.cycle > resync_cycle)
                            && frame.role == role =>
                    {
                        if ws_tx
                            .send(Message::Binary(frame.bytes.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    // older cycle, already contained in the resync
                    Ok(_) => {}
                    // fell behind the ring: resync to a consistent state, and drop
                    // everything the new resync already covers
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let Some(cycle) = self.resync(ws_tx, seq, role).await else {
                            break;
                        };
                        resync_cycle = cycle;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                // the reader stopped, so the client is gone
                _ = &mut close_rx => break,
            }
        }
    }

    /// Run the trigger - compile - update deps loop.
    async fn compile_loop(
        self: Arc<Self>,
        mut trigger_tx: mpsc::UnboundedReceiver<Trigger>,
        deps_tx: std::sync::mpsc::Sender<Vec<PathBuf>>,
    ) {
        // initial compile; its dependency list is what the watcher starts on.
        let this = self.clone();
        let (update, dependencies) = tokio::task::spawn_blocking(move || {
            let mut session = this.session.blocking_lock();
            let update = session.initial_cycle();
            (update, session.dependencies())
        })
        .await
        .expect("preview compilation task panicked");
        self.publish_update(update).await;
        let _ = deps_tx.send(dependencies);

        while let Some(trigger) = trigger_tx.recv().await {
            let this = self.clone();
            let (update, dependencies, receiver) =
                tokio::task::spawn_blocking(move || {
                    let mut session = this.session.blocking_lock();
                    let mut changed = session.apply(trigger);
                    while let Ok(next) = trigger_tx.try_recv() {
                        changed |= session.apply(next);
                    }
                    let update = changed.then(|| session.cycle());
                    (update, session.dependencies(), trigger_tx)
                })
                .await
                .expect("preview compilation task panicked");
            trigger_tx = receiver;
            if let Some(update) = update {
                self.publish_update(update).await;
            }
            let _ = deps_tx.send(dependencies);
        }
    }

    /// Turn one update into frames and fan them out to the connections.
    async fn publish_update(&self, update: Update) {
        // Nobody is listening; the frames would only be encoded to be dropped.
        if self.update_tx.receiver_count() == 0 {
            return;
        }
        let epoch = update.epoch;
        let mut seq = self.next_seq.lock().await;
        for (role, message) in update.messages() {
            let frame = OutFrame::new(*seq, epoch, role, message);
            *seq = seq.wrapping_add(1);
            let _ = self.update_tx.send(frame);
        }
    }

    /// Send special message with cycle = 0
    async fn publish_reply(&self, role: Role, message: WireMessage) {
        let mut seq = self.next_seq.lock().await;
        let frame = OutFrame::new(*seq, 0, role, message);
        *seq = seq.wrapping_add(1);
        let _ = self.update_tx.send(frame);
    }
}

impl Update {
    /// Send the frames of this update that are addressed to `role`.
    async fn send_through(
        self,
        ws_tx: &mut WsTx,
        seq: &mut u32,
        role: Role,
    ) -> Option<()> {
        for (target, message) in self.messages() {
            if target == role {
                send(ws_tx, seq, message).await?;
            }
        }
        Some(())
    }
}

/// Send one message of our own, taking the next sequence number.
async fn send(ws_tx: &mut WsTx, seq: &mut u32, message: WireMessage) -> Option<()> {
    ws_tx
        .send(Message::Binary(Frame::new(*seq, message).to_bytes().into()))
        .await
        .ok()?;
    *seq += 1;
    Some(())
}

#[derive(Debug, Clone)]
pub struct OutFrame {
    pub cycle: u64,
    pub bytes: Vec<u8>,
    /// The connection role this frame is for; decided by [`Update::messages`].
    pub role: Role,
}
impl OutFrame {
    fn new(seq: u32, cycle: u64, role: Role, message: WireMessage) -> Self {
        Self {
            cycle,
            role,
            bytes: Frame::new(seq, message).to_bytes(),
        }
    }
}

/// Bind, start the compile loop and the accept loop.
pub async fn serve(
    session: Session,
    host: &str,
    port: u16,
    settings: PreviewSettings,
) -> std::io::Result<SocketAddr> {
    // each `handle_socket` worker subscribe to this, we send `Update` through
    // `update_tx`, a.k.a `state.broadcast`, they receives `Update` and send to their ws
    // tunnel if it's their message
    let (update_tx, _) = broadcast::channel::<OutFrame>(1024);

    // send & receive `Trigger`, which wakes up compilation
    let (trigger_tx, trigger_rx) = mpsc::unbounded_channel::<Trigger>();
    let state = Arc::new(AppState {
        session: Mutex::new(session),
        update_tx,
        next_seq: Mutex::new(1),
        trigger_tx: trigger_tx.clone(),
        index_html: render_index(settings),
        editor_connected: AtomicBool::new(false),
    });

    // Start file watcher.
    //
    // We compile and send `deps` through `deps_tx`, watcher watches `deps` and send
    //`Trigger` through `triggers_tx`
    let deps_tx = spawn_watcher(trigger_tx);

    tokio::spawn(state.clone().compile_loop(trigger_rx, deps_tx));

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let bound = listener.local_addr()?;
    eprintln!("serving at http://{host}:{}", bound.port());
    let app = state.router();
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("server error: {err}");
        }
    });
    Ok(bound)
}

/// Where the viewer's `index.html` expects the session settings.
const SETTINGS_MARKER: &str = "/*ht-settings*/";

/// `index.html` with the settings marker replaced, so the viewer knows what to paint.
fn render_index(settings: PreviewSettings) -> Vec<u8> {
    let html = holotyp_preview_viewer::get_file("index.html").unwrap_or_default();
    let html = String::from_utf8_lossy(html);
    let json = serde_json::to_string(&settings).expect("settings serialize");
    html.replace(SETTINGS_MARKER, &json).into_bytes()
}

/// Start the file watcher on its own thread.
fn spawn_watcher(
    trigger_tx: mpsc::UnboundedSender<Trigger>,
) -> std::sync::mpsc::Sender<Vec<PathBuf>> {
    let (deps_tx, deps_rx) = std::sync::mpsc::channel::<Vec<PathBuf>>();
    let thread =
        std::thread::Builder::new()
            .name("preview-watch".into())
            .spawn(move || {
                let mut watcher = match Watcher::new(None) {
                    Ok(watcher) => watcher,
                    Err(err) => {
                        eprintln!("file watching unavailable: {err}");
                        return;
                    }
                };
                // Wait for the dependency list of the next compilation, watch it, block
                // until something moves, then wake the compile loop.
                loop {
                    let Ok(deps) = deps_rx.recv() else { return };
                    if let Err(err) = watcher.update(deps) {
                        eprintln!("{err}");
                    }
                    if let Err(err) = watcher.wait() {
                        eprintln!("file watching failed: {err}");
                        return;
                    }
                    if trigger_tx.send(Trigger::Disk).is_err() {
                        return;
                    }
                }
            });
    if let Err(err) = thread {
        eprintln!("could not start the watcher thread: {err}");
    }
    deps_tx
}

/// The URL of a served preview, as a browser on this machine should open it.
pub fn browser_url(bound: SocketAddr) -> String {
    if bound.ip().is_unspecified() {
        return format!("http://127.0.0.1:{}", bound.port());
    }
    format!("http://{bound}")
}

/// The content type of one embedded viewer asset, by file extension.
fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("json") => "application/json",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Whether a page at `origin` may open the WebSocket.
fn trusted_origin(origin: &str) -> bool {
    let Ok(url) = url::Url::parse(origin) else { return false };
    match url.scheme() {
        "vscode-webview" => true,
        "http" | "https" => match url.host() {
            Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        },
        _ => false,
    }
}
