use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;

#[cfg(debug_assertions)]
use notify::{Event, RecursiveMode, Watcher};
#[cfg(debug_assertions)]
use std::path::Path;
#[cfg(debug_assertions)]
use tracing::info;

#[derive(Clone)]
pub struct LiveReloadState {
    pub tx: broadcast::Sender<()>,
}

impl Default for LiveReloadState {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveReloadState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<LiveReloadState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<LiveReloadState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let mut send_task = tokio::spawn(async move {
        while rx.recv().await.is_ok() {
            if sender
                .send(Message::Text("reload".to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            if matches!(msg, Ok(Message::Close(_))) {
                break;
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

#[cfg(debug_assertions)]
pub fn start_file_watcher(state: Arc<LiveReloadState>) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if event.kind.is_modify() || event.kind.is_create() {
                for path in &event.paths {
                    if let Some(ext) = path.extension() {
                        if ext == "html" || ext == "css" || ext == "js" {
                            let _ = tx.send(());
                            break;
                        }
                    }
                }
            }
        }
    })?;

    watcher.watch(Path::new("templates"), RecursiveMode::Recursive)?;
    watcher.watch(Path::new("static"), RecursiveMode::Recursive)?;

    let reload_tx = state.tx.clone();

    std::thread::spawn(move || {
        let _watcher = watcher;
        while rx.recv().is_ok() {
            info!("File change detected, triggering reload");
            let _ = reload_tx.send(());
        }
    });

    info!("Live reload watcher started for templates/ and static/ directories");
    Ok(())
}

pub fn inject_reload_script() -> &'static str {
    r#"
<script>
(function() {
    if (typeof WebSocket === 'undefined') return;

    let ws = null;
    let reconnectTimeout = null;

    function connect() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const url = `${protocol}//${window.location.host}/ws/reload`;

        ws = new WebSocket(url);

        ws.onopen = function() {
            console.log('Live reload connected');
            if (reconnectTimeout) {
                clearTimeout(reconnectTimeout);
                reconnectTimeout = null;
            }
        };

        ws.onmessage = function(event) {
            if (event.data === 'reload') {
                console.log('Reloading page...');
                window.location.reload();
            }
        };

        ws.onclose = function() {
            ws = null;
            reconnectTimeout = setTimeout(connect, 1000);
        };

        ws.onerror = function() {
            ws.close();
        };
    }

    connect();
})();
</script>
"#
}
