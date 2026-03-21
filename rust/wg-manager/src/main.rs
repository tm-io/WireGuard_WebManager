//! WireGuard Web Manager - Web UI（Rust 実装）
//! RHEL / Ubuntu 等で単一バイナリとして動作させる。

mod db;
mod wg_client;
mod wg_local;
mod templates;

use axum::{
    body::Bytes,
    extract::{Form, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router as AxumRouter,
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use minijinja::Environment;
use pulldown_cmark::{html, Options, Parser};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use wg_common::Settings;

struct AppState {
    settings: RwLock<Settings>,
    db: db::Database,
    templates: Environment<'static>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config_path = std::env::var("CONFIG_PATH")
        .ok()
        .map(|s| Path::new(&s).to_path_buf())
        .unwrap_or_else(|| Path::new(wg_common::config::DEFAULT_CONFIG_PATH).to_path_buf());

    let settings = Settings::load(Some(config_path.as_path())).map_err(|e| {
        let msg = format!(
            "設定ファイル '{}' の読み込みに失敗しました: {}",
            config_path.display(),
            e
        );
        tracing::error!("{}", msg);
        tracing::error!(
            "config.yaml の YAML 構文・権限・パスを確認してください \
             (journalctl -u wireguard-webmanager で詳細を確認)"
        );
        msg
    })?;

    let base_dir = base_dir_from_config_path(&config_path);
    let db_path = resolve_pathbuf_under_base(&base_dir, Path::new(&settings.paths.db_path));
    let database = db::Database::open(db_path.as_path())?;
    database.init()?;

    let state = Arc::new(AppState {
        settings: RwLock::new(settings.clone()),
        db: database,
        templates: templates::build_env(),
    });

    let api = AxumRouter::new()
        .route("/server/public-key", get(api_server_public_key))
        .route("/server/peer-stats", get(api_server_peer_stats))
        .route("/server/wg-version", get(api_server_wg_version))
        .route("/server/wg-update", post(api_server_wg_update))
        .route("/peers/", get(api_peers_list).post(api_peers_create))
        .route("/peers/:id", axum::routing::delete(api_peers_delete).put(api_peers_update))
        .route("/peers/:id/conf/download", get(api_peers_conf_download))
        .route("/peers/:id/qr", get(api_peers_qr))
        .route("/settings", get(api_settings_get).put(api_settings_put))
        .with_state(state.clone());

    let app = Router::new()
        .route("/", get(redirect_login))
        .route("/login", get(login_page).post(login_post))
        .route("/logout", get(logout))
        .route("/dashboard", get(dashboard_page))
        .route("/peers", get(peers_page))
        .route("/settings", get(settings_page))
        .route("/manual", get(docs_list_page))
        .route("/manual/view/:name", get(docs_view_page))
        .nest("/api", api)
        .with_state(state);

    let addr = format!("{}:{}", settings.app.host, settings.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            let msg = format!(
                "ポート {} はすでに使用中です。config.yaml の app.port を変更するか、\
                 競合するプロセスを停止してください (lsof -i :{} で確認可能)",
                settings.app.port, settings.app.port
            );
            tracing::error!("{}", msg);
            msg
        } else {
            let msg = format!("アドレス {} へのバインドに失敗しました: {}", addr, e);
            tracing::error!("{}", msg);
            msg
        }
    })?;
    tracing::info!(
        "wg-manager v{} 起動完了 - {} でリクエストを待機中",
        env!("CARGO_PKG_VERSION"),
        addr
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn redirect_login() -> axum::response::Redirect {
    axum::response::Redirect::to("/login")
}

async fn login_page(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if is_logged_in(&jar) {
        return axum::response::Redirect::to("/dashboard").into_response();
    }
    match templates::render_login(&state.templates, None) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

async fn login_post(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let settings = state.settings.read().await;
    let ok = form.username == settings.app.auth_username && form.password == settings.app.auth_password;

    if ok {
        let mut cookie = Cookie::new("wgwm_session", "authenticated");
        cookie.set_http_only(true);
        cookie.set_same_site(SameSite::Lax);
        cookie.set_path("/");
        // https の場合は Secure を有効にしたい（ここでは環境依存なので未設定）

        let jar = jar.add(cookie);
        return (jar, axum::response::Redirect::to("/dashboard")).into_response();
    }

    let msg = "ユーザー名またはパスワードが違います";
    match templates::render_login(&state.templates, Some(msg)) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn dashboard_page(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return axum::response::Redirect::to("/login").into_response();
    }
    match templates::render_dashboard(&state.templates) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

fn is_logged_in(jar: &CookieJar) -> bool {
    jar.get("wgwm_session")
        .map(|c| c.value() == "authenticated")
        .unwrap_or(false)
}

async fn logout(jar: CookieJar) -> impl IntoResponse {
    let jar = jar.remove(Cookie::from("wgwm_session"));
    (jar, axum::response::Redirect::to("/login"))
}

async fn peers_page(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return axum::response::Redirect::to("/login").into_response();
    }
    match templates::render_peers(&state.templates) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn settings_page(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return axum::response::Redirect::to("/login").into_response();
    }
    match templates::render_settings(&state.templates) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn docs_list_page(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return axum::response::Redirect::to("/login").into_response();
    }
    let entries = list_docs_entries();
    match templates::render_docs_list(&state.templates, entries) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn docs_view_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return axum::response::Redirect::to("/login").into_response();
    }
    let path = match safe_doc_path(&name) {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let md = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let title = md.lines().next().unwrap_or(&name).trim().trim_start_matches('#').trim();
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&md, opts);
    let mut html_body = String::new();
    html::push_html(&mut html_body, parser);
    match templates::render_docs_view(&state.templates, title, &html_body) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

fn docs_dir() -> std::path::PathBuf {
    // config.yaml の場所を基準に docs/ を探す（WorkingDirectory に依存しない）
    let config_path = std::env::var("CONFIG_PATH")
        .ok()
        .unwrap_or_else(|| wg_common::config::DEFAULT_CONFIG_PATH.to_string());
    let base = base_dir_from_config_path(Path::new(&config_path));
    base.join("docs")
}

fn base_dir_from_config_path(config_path: &Path) -> std::path::PathBuf {
    // 可能なら config.yaml の親ディレクトリ。相対や不正ならカレントを採用。
    config_path
        .parent()
        .map(|p| p.to_path_buf())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")))
}

fn resolve_pathbuf_under_base(base: &Path, p: &Path) -> std::path::PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

fn safe_doc_path(name: &str) -> Option<std::path::PathBuf> {
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return None;
    }
    let p = docs_dir().join(format!("{name}.md"));
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

fn list_docs_entries() -> serde_json::Value {
    let mut arr = Vec::new();
    if let Ok(rd) = std::fs::read_dir(docs_dir()) {
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let title = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.to_string()))
                    .unwrap_or_else(|| stem.to_string());
                let title = title.trim().trim_start_matches('#').trim().to_string();
                arr.push(json!({ "name": stem, "title": title }));
            }
        }
    }
    // docs/01-setup の順に並んで欲しいので name でソート
    arr.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    serde_json::Value::Array(arr)
}

// --------------------------
// API
// --------------------------

async fn api_server_public_key(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let settings = state.settings.read().await.clone();
    // まず Worker を試し、失敗したら sudo wg にフォールバック（運用の回復性を優先）
    let pubkey = if !settings.paths.wg_worker_socket.trim().is_empty() {
        match wg_client::get_server_public_key(&settings) {
            Ok(k) => Ok(k),
            Err(e) => {
                tracing::warn!(
                    "Worker ソケット ({}) への接続に失敗しました: {}。sudo wg にフォールバックします",
                    settings.paths.wg_worker_socket,
                    e
                );
                wg_local::sudo_wg(&["show", &settings.wireguard.interface, "public-key"])
            }
        }
    } else {
        wg_local::sudo_wg(&["show", &settings.wireguard.interface, "public-key"])
    };
    match pubkey {
        Ok(k) => Json(json!({ "public_key": k })).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, e).into_response(),
    }
}

async fn api_server_peer_stats(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let settings = state.settings.read().await.clone();
    let db_peers = match state.db.list_peers() {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    let wg_peers = if !settings.paths.wg_worker_socket.trim().is_empty() {
        wg_client::get_peer_stats(&settings)
    } else {
        // sudo wg show dump でフォールバック
        match wg_local::sudo_wg_dump(&settings.wireguard.interface) {
            Ok(dump) => parse_dump(&settings.wireguard.interface, &dump),
            Err(e) => Err(e),
        }
    };
    let wg_peers = match wg_peers {
        Ok(p) => p,
        Err(e) => return (StatusCode::SERVICE_UNAVAILABLE, e).into_response(),
    };
    let mut by_pub = std::collections::HashMap::new();
    for p in wg_peers {
        by_pub.insert(p.public_key.clone(), p);
    }
    let mut result = Vec::new();
    for peer in db_peers {
        let s = by_pub.get(&peer.public_key);
        let connected = s.and_then(|x| x.latest_handshake).is_some();
        result.push(json!({
            "id": peer.id,
            "public_key": peer.public_key,
            "connected": connected,
            "latest_handshake": s.and_then(|x| x.latest_handshake),
            "rx_bytes": s.map(|x| x.rx_bytes).unwrap_or(0),
            "tx_bytes": s.map(|x| x.tx_bytes).unwrap_or(0),
        }));
    }
    Json(json!({ "peers": result })).into_response()
}

async fn api_server_wg_update(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let settings = state.settings.read().await.clone();
    match tokio::task::spawn_blocking(move || wg_client::update_wireguard(&settings)).await {
        Ok(Ok(output)) => Json(json!({ "ok": true, "output": output })).into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "error": e })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "error": format!("タスクエラー: {}", e) })),
        )
            .into_response(),
    }
}

async fn api_server_wg_version(jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let current_raw = wg_local::get_wg_version();
    let current = current_raw
        .as_deref()
        .and_then(extract_semverish)
        .map(|s| s.to_string());
    let latest = fetch_latest_wg_version().ok().flatten();
    let outdated = match (current.as_deref(), latest.as_deref()) {
        (Some(c), Some(l)) => parse_ver_tuple(c) < parse_ver_tuple(l),
        _ => false,
    };
    Json(json!({
        "current": current,
        "current_raw": current_raw,
        "latest": latest,
        "outdated": outdated
    })).into_response()
}

#[derive(serde::Serialize)]
struct PeerDto {
    id: i64,
    name: String,
    public_key: String,
    allocated_ip: String,
    is_active: bool,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pre_shared_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_key_encrypted: Option<String>,
}

async fn api_peers_list(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.db.list_peers() {
        Ok(peers) => {
            let out: Vec<PeerDto> = peers
                .into_iter()
                .map(|p| PeerDto {
                    id: p.id,
                    name: p.name,
                    public_key: p.public_key,
                    allocated_ip: p.allocated_ip,
                    is_active: p.is_active,
                    created_at: p.created_at,
                    pre_shared_key: p.pre_shared_key,
                    private_key_encrypted: None,
                })
                .collect();
            Json(out).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct CreatePeerReq {
    name: String,
}

async fn api_peers_create(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(req): Json<CreatePeerReq>,
) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let settings = state.settings.read().await.clone();
    let used_ips = state.db.list_allocated_ips().unwrap_or_default();
    let ip = match allocate_next_ip(&settings.wireguard.client_ip_range, &used_ips) {
        Ok(ip) => ip,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let privk = match wg_local::generate_private_key() {
        Ok(k) => k,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    let pubk = match wg_local::generate_public_key(&privk) {
        Ok(k) => k,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    let psk = wg_local::generate_preshared_key().ok();
    let peer = match state.db.create_peer(
        &req.name,
        &pubk,
        &privk,
        psk.as_deref(),
        &ip,
        true,
    ) {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    // 作成直後に wg set で即有効化する（Peer 作成 = すぐ使える状態）。
    let allowed = vec![format!("{}/32", peer.allocated_ip)];
    let r = if !settings.paths.wg_worker_socket.trim().is_empty() {
        wg_client::peer_set(&settings, &peer.public_key, &allowed, peer.pre_shared_key.as_deref())
    } else {
        wg_local::sudo_wg_set_peer(
            &settings.wireguard.interface,
            &peer.public_key,
            &allowed.join(","),
            peer.pre_shared_key.as_deref(),
        )
    };
    if let Err(e) = r {
        return (StatusCode::SERVICE_UNAVAILABLE, e).into_response();
    }
    Json(PeerDto {
        id: peer.id,
        name: peer.name,
        public_key: peer.public_key,
        allocated_ip: peer.allocated_ip,
        is_active: peer.is_active,
        created_at: peer.created_at,
        pre_shared_key: peer.pre_shared_key.clone(),
        private_key_encrypted: Some(peer.private_key_encrypted),
    })
    .into_response()
}

#[derive(serde::Deserialize)]
struct UpdatePeerReq {
    is_active: bool,
}

async fn api_peers_update(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<UpdatePeerReq>,
) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let settings = state.settings.read().await.clone();
    let peer = match state.db.get_peer(id) {
        Ok(Some(p)) => p,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    if peer.is_active != req.is_active {
        let allowed = vec![format!("{}/32", peer.allocated_ip)];
        let r = if req.is_active {
            // enable -> wg set peer allowed-ips (+ psk if any)
            if !settings.paths.wg_worker_socket.trim().is_empty() {
                wg_client::peer_set(&settings, &peer.public_key, &allowed, peer.pre_shared_key.as_deref())
            } else {
                wg_local::sudo_wg_set_peer(
                    &settings.wireguard.interface,
                    &peer.public_key,
                    &allowed.join(","),
                    peer.pre_shared_key.as_deref(),
                )
            }
        } else {
            // disable -> remove from wg
            if !settings.paths.wg_worker_socket.trim().is_empty() {
                wg_client::peer_remove(&settings, &peer.public_key)
            } else {
                wg_local::sudo_wg(&[
                    "set",
                    &settings.wireguard.interface,
                    "peer",
                    &peer.public_key,
                    "remove",
                ])
                .map(|_| ())
            }
        };
        if let Err(e) = r {
            return (StatusCode::SERVICE_UNAVAILABLE, e).into_response();
        }
        if let Err(e) = state.db.set_peer_active(id, req.is_active) {
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn api_peers_delete(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let settings = state.settings.read().await.clone();
    if let Ok(Some(peer)) = state.db.get_peer(id) {
        // 有効なら wg からも remove してから削除（親切実装）
        if peer.is_active {
            let _ = if !settings.paths.wg_worker_socket.trim().is_empty() {
                wg_client::peer_remove(&settings, &peer.public_key)
            } else {
                wg_local::sudo_wg(&[
                    "set",
                    &settings.wireguard.interface,
                    "peer",
                    &peer.public_key,
                    "remove",
                ])
                .map(|_| ())
            };
        }
    }
    let _ = state.db.delete_peer(id);
    StatusCode::NO_CONTENT.into_response()
}

async fn api_peers_conf_download(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<i64>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let settings = state.settings.read().await.clone();
    let peer = match state.db.get_peer(id) {
        Ok(Some(p)) => p,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let server_public_key = match q.get("server_public_key") {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return (StatusCode::BAD_REQUEST, "server_public_key required").into_response(),
    };
    let conf = build_client_conf(&settings, &peer, &server_public_key);
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}.conf\"", peer.name)).unwrap(),
    );
    (headers, conf).into_response()
}

async fn api_peers_qr(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<i64>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let settings = state.settings.read().await.clone();
    let peer = match state.db.get_peer(id) {
        Ok(Some(p)) => p,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let server_public_key = match q.get("server_public_key") {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return (StatusCode::BAD_REQUEST, "server_public_key required").into_response(),
    };
    let conf = build_client_conf(&settings, &peer, &server_public_key);
    let png = match qrcode_png(&conf) {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    let mut headers = HeaderMap::new();
    headers.insert(axum::http::header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    (headers, Bytes::from(png)).into_response()
}

async fn api_settings_get(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let path = std::env::var("CONFIG_PATH")
        .ok()
        .unwrap_or_else(|| wg_common::config::DEFAULT_CONFIG_PATH.to_string());
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut v: serde_json::Value = serde_yaml::from_str::<serde_yaml::Value>(&raw)
        .ok()
        .and_then(|y| serde_json::to_value(y).ok())
        .unwrap_or_else(|| json!({}));
    // auth_password は空で返す（UI は空なら維持）
    if let Some(app) = v.get_mut("app") {
        if let Some(obj) = app.as_object_mut() {
            obj.insert("auth_password".to_string(), serde_json::Value::String(String::new()));
        }
    }
    Json(v).into_response()
}

async fn api_settings_put(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(mut body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if !is_logged_in(&jar) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let path = std::env::var("CONFIG_PATH")
        .ok()
        .unwrap_or_else(|| wg_common::config::DEFAULT_CONFIG_PATH.to_string());
    // 空パスワードは「維持」なので、既存を読み込んで埋め戻す
    let existing_raw = std::fs::read_to_string(&path).unwrap_or_default();
    let existing_yaml: serde_yaml::Value = serde_yaml::from_str(&existing_raw).unwrap_or(serde_yaml::Value::Null);
    let existing_json = serde_json::to_value(existing_yaml).unwrap_or_else(|_| json!({}));
    if body.pointer("/app/auth_password").and_then(|v| v.as_str()) == Some("") {
        if let Some(old) = existing_json.pointer("/app/auth_password") {
            if let Some(app) = body.get_mut("app").and_then(|v| v.as_object_mut()) {
                app.insert("auth_password".to_string(), old.clone());
            }
        }
    }
    // JSON -> YAML へ変換して保存
    let yaml = serde_yaml::to_string(&body).map_err(|e| e.to_string());
    let yaml = match yaml {
        Ok(s) => s,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    if let Err(e) = std::fs::write(&path, yaml) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    // 保存後に設定を再読込して即時反映（ホスト/ポート変更などは再起動が必要）
    if let Ok(new_settings) = Settings::load(Some(std::path::Path::new(&path))) {
        *state.settings.write().await = new_settings;
    }
    StatusCode::NO_CONTENT.into_response()
}

fn allocate_next_ip(cidr: &str, used_ips: &[String]) -> Result<String, String> {
    let net: ipnet::IpNet = cidr.parse().map_err(|_| "client_ip_range が不正です".to_string())?;
    let used: std::collections::HashSet<String> = used_ips.iter().cloned().collect();
    // reserve_first_n=1: ネットワークの先頭(一般に .1)を避けたい場合が多いので .2 から探す
    let mut idx = 0usize;
    for ip in net.hosts() {
        idx += 1;
        if idx <= 1 {
            continue;
        }
        let s = ip.to_string();
        if !used.contains(&s) {
            return Ok(s);
        }
    }
    Err("空き IP がありません".to_string())
}

fn build_client_conf(settings: &Settings, peer: &db::Peer, server_public_key: &str) -> String {
    let endpoint = format!("{}:{}", settings.wireguard.server_endpoint, settings.wireguard.listen_port);
    let mut lines = vec![
        "[Interface]".to_string(),
        format!("PrivateKey = {}", peer.private_key_encrypted),
        format!("Address = {}/32", peer.allocated_ip),
        format!("DNS = {}", settings.wireguard.client_dns),
        "".to_string(),
        "[Peer]".to_string(),
        format!("PublicKey = {}", server_public_key),
        format!("Endpoint = {}", endpoint),
        "AllowedIPs = 0.0.0.0/0, ::/0".to_string(),
        format!("PersistentKeepalive = {}", settings.wireguard.persistent_keepalive),
    ];
    if let Some(psk) = peer.pre_shared_key.as_ref() {
        // AllowedIPs の前に入れる（Python の挿入位置に合わせる）
        let insert_at = lines.len().saturating_sub(2);
        lines.insert(insert_at, format!("PresharedKey = {}", psk));
    }
    lines.join("\n") + "\n"
}

fn qrcode_png(text: &str) -> Result<Vec<u8>, String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|e| e.to_string())?;
    let image = code.render::<image::Luma<u8>>().min_dimensions(320, 320).build();
    let mut bytes: Vec<u8> = Vec::new();
    let dynimg = image::DynamicImage::ImageLuma8(image);
    dynimg
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn parse_dump(interface: &str, dump: &str) -> Result<Vec<wg_common::PeerStat>, String> {
    let lines: Vec<&str> = dump.lines().collect();
    if lines.is_empty() {
        return Ok(vec![]);
    }
    // 1行目はインターフェース行。以降 peer 行（Python/worker と同様に最低8列）
    let mut peers = Vec::new();
    for line in lines.iter().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 8 {
            continue;
        }
        let latest_handshake = cols[4].parse::<u64>().ok().filter(|&x| x != 0);
        let rx_bytes = cols[5].parse().unwrap_or(0u64);
        let tx_bytes = cols[6].parse().unwrap_or(0u64);
        peers.push(wg_common::PeerStat {
            public_key: cols[0].to_string(),
            endpoint: if cols[2].is_empty() { None } else { Some(cols[2].to_string()) },
            allowed_ips: cols[3]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
            latest_handshake,
            rx_bytes,
            tx_bytes,
        });
    }
    let _ = interface; // 現状未使用（将来のログ用途）
    Ok(peers)
}

fn extract_semverish(raw: &str) -> Option<&str> {
    // "wireguard-tools v1.0.20210914 - https://..." or "1.0.20210914"
    let s = raw.trim();
    let s = s.strip_prefix("wireguard-tools ").unwrap_or(s);
    let s = s.strip_prefix('v').unwrap_or(s);
    // スペース以降（URL等）を除去してバージョン番号部分のみ取り出す
    let s = s.split_whitespace().next().unwrap_or(s);
    if s.split('.').take(3).all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        Some(s)
    } else {
        None
    }
}

fn parse_ver_tuple(v: &str) -> (u32, u32, u32) {
    let mut it = v.split('.');
    let a = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let b = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let c = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    (a, b, c)
}

fn fetch_latest_wg_version() -> Result<Option<String>, String> {
    let url = "https://api.github.com/repos/WireGuard/wireguard-tools/releases/latest";
    let resp = ureq::get(url)
        .set("Accept", "application/vnd.github.v3+json")
        .set("User-Agent", "wg-manager")
        .call()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e: std::io::Error| e.to_string())?;
    let mut tag = v.get("tag_name").and_then(|t| t.as_str()).unwrap_or("").to_string();
    if tag.starts_with('v') {
        tag = tag.trim_start_matches('v').to_string();
    }
    if tag.is_empty() { Ok(None) } else { Ok(Some(tag)) }
}
