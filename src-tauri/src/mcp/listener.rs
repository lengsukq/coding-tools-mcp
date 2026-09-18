use std::sync::Arc;

use axum::extract::{Form, Path, Query, State};
use axum::http::{
    header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, REFERRER_POLICY, WWW_AUTHENTICATE, X_CONTENT_TYPE_OPTIONS},
    HeaderMap, StatusCode,
};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

use crate::auth::{
    authorization_server_metadata, authorize_get, authorize_post, external_base_url,
    protected_resource_metadata, protected_resource_metadata_url, register_client, token_exchange,
    verify_bearer_header, verify_oauth_bearer_header, AuthorizeForm, AuthorizeParams,
    ClientRegistrationRequest, OAuthRuntime, TokenForm,
};
use crate::local_network;
use crate::mcp::audit::{log_request, record_response, RpcRequestMeta};
use crate::mcp::gateway::{
    session_id_from_metadata, session_id_from_request, GatewayState, MCP_SESSION_HEADER,
};
use crate::mcp::server::handle_gateway_request;
use crate::secret::SecretStore;
use crate::settings::AppSettings;
use crate::tunnel::append_profile_log;
use crate::workspace::AuthConfig;

pub type ShutdownSender = oneshot::Sender<()>;

#[derive(Debug, Clone)]
pub struct GatewayListenerConfig {
    pub port: u16,
    pub auth: AuthConfig,
    pub public_base_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct ListenerSecrets {
    pub oauth_client_secret: Option<String>,
    pub oauth_password: Option<String>,
    pub oauth_token_secret: Option<String>,
}

async fn oauth_register_post(
    State(state): State<ListenerState>,
    Json(request): Json<ClientRegistrationRequest>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    register_client(oauth, request)
}

#[derive(Clone)]
struct ListenerState {
    gateway: Arc<GatewayState>,
    auth: AuthConfig,
    scope_id: String,
    scope_label: String,
    bind_port: u16,
    configured_public_url: String,
    bearer_token: Option<String>,
    oauth: Option<Arc<OAuthRuntime>>,
    oauth_client_secret: Option<String>,
}

pub fn spawn_gateway_listener(
    config: GatewayListenerConfig,
    secrets: ListenerSecrets,
    gateway: Arc<GatewayState>,
) -> Result<(ShutdownSender, tauri::async_runtime::JoinHandle<()>), String> {
    let GatewayListenerConfig {
        port,
        auth,
        public_base_url,
    } = config;
    let ListenerSecrets {
        oauth_client_secret,
        oauth_password,
        oauth_token_secret,
    } = secrets;
    let bearer_token = if auth.bearer_enabled() {
        SecretStore::get_shared("bearer_token").map_err(|error| error.to_string())?
    } else {
        None
    };
    let configured_public_url = public_base_url.trim().to_string();
    let oauth = if auth.oauth_enabled() {
        let oauth_base = external_base_url(&HeaderMap::new(), port, &configured_public_url);
        let oauth_runtime = OAuthRuntime::new_app_persistent(
            oauth_base,
            auth.oauth_client_id.clone(),
            oauth_client_secret.clone(),
            oauth_password.unwrap_or_default(),
            oauth_token_secret.unwrap_or_default(),
            "global_mcp".into(),
            "oauth_dynamic_clients".into(),
        )?;
        if let Some(notice) = oauth_runtime.recovery_notice() {
            append_profile_log("global-mcp", "stderr.log", notice);
        }
        Some(Arc::new(oauth_runtime))
    } else {
        None
    };
    let state = ListenerState {
        gateway,
        auth,
        scope_id: "global-mcp".into(),
        scope_label: "Global MCP Gateway".into(),
        bind_port: port,
        configured_public_url,
        bearer_token,
        oauth,
        oauth_client_secret,
    };
    let allow_lan_access = AppSettings::load_or_default().allow_lan_access;
    let listener = bind_listener(port, allow_lan_access)?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = tauri::async_runtime::spawn(async move {
        if let Err(error) = serve(listener, port, allow_lan_access, state, shutdown_rx).await {
            eprintln!("global mcp listener stopped: {error}");
        }
    });
    Ok((shutdown_tx, handle))
}

async fn serve(
    listener: tokio::net::TcpListener,
    port: u16,
    allow_lan_access: bool,
    state: ListenerState,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let profile_id = state.scope_id.clone();
    // CORS is intentionally applied only to the MCP/OAuth protocol router.
    // Review links are bearer URLs and must not inherit permissive CORS.
    let app = build_router(state);

    append_profile_log(
        &profile_id,
        "stdout.log",
        &format!(
            "[mcp] listening on http://{}:{port}/mcp",
            local_network::bind_host(allow_lan_access)
        ),
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown.await;
        })
        .await?;
    Ok(())
}

fn build_router(state: ListenerState) -> Router {
    let protocol = Router::new()
        .route("/mcp", get(mcp_discovery).post(mcp_post))
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth_protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(oauth_protected_resource_metadata),
        )
        .route("/register", post(oauth_register_post))
        .route(
            "/oauth/authorize",
            get(oauth_authorize_get).post(oauth_authorize_post),
        )
        .route("/oauth/token", post(oauth_token_post))
        .layer(CorsLayer::permissive());
    let review = Router::new()
        .route("/review/{id}", get(review_page))
        .route("/api/reviews/{id}", get(review_api))
        .route("/api/reviews/{id}/diff", get(review_api));
    protocol.merge(review).with_state(state)
}

#[derive(serde::Deserialize)]
struct ReviewQuery { t: String }

fn review_from_request(state: &ListenerState, id: &str, token: &str) -> Result<crate::review::ChangeSet, StatusCode> {
    let roots = state.gateway.registry.list().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter().map(|profile| std::path::PathBuf::from(profile.path));
    crate::review::find_review(roots, id, token).map_err(|message| match message.as_str() {
        "invalid review token" => StatusCode::UNAUTHORIZED,
        "review expired" => StatusCode::GONE,
        _ => StatusCode::NOT_FOUND,
    })
}

async fn review_api(State(state): State<ListenerState>, Path(id): Path<String>, Query(query): Query<ReviewQuery>) -> Response {
    match review_from_request(&state, &id, &query.t) {
        Ok(review) => ([(CACHE_CONTROL, "no-store"),(X_CONTENT_TYPE_OPTIONS,"nosniff"),(REFERRER_POLICY,"no-referrer")], Json(review)).into_response(),
        Err(status) => status.into_response(),
    }
}

async fn review_page(State(state): State<ListenerState>, Path(id): Path<String>, Query(query): Query<ReviewQuery>) -> Response {
    let review = match review_from_request(&state, &id, &query.t) { Ok(review) => review, Err(status) => return status.into_response() };
    let data = serde_json::to_string(&review).unwrap_or_else(|_| "{}".into()).replace('<', "\\u003c");
    let html = format!(r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>AI Change Review</title><style>
:root{{color-scheme:light;--review-sticky-offset:154px;--page:#f3f4f8;--surface:rgba(255,255,255,.72);--surface-strong:rgba(255,255,255,.9);--border:rgba(60,60,67,.11);--border-soft:rgba(255,255,255,.78);--text:#17171a;--secondary:#63636b;--muted:#92929c;--primary:#0a84ff;--primary-soft:rgba(10,132,255,.11);--success:#30d158;--success-soft:rgba(48,209,88,.10);--danger:#ff453a;--danger-soft:rgba(255,69,58,.09);--purple:#af52de;--shadow:0 22px 70px rgba(31,35,48,.09),0 2px 10px rgba(31,35,48,.035);font-family:-apple-system,BlinkMacSystemFont,"SF Pro Display","SF Pro Text",Inter,ui-sans-serif,system-ui,sans-serif}}
*{{box-sizing:border-box}}html{{min-height:100%;background:var(--page)}}body{{min-height:100vh;margin:0;color:var(--text);letter-spacing:-.012em;background:radial-gradient(circle at 12% -8%,rgba(10,132,255,.14),transparent 31%),radial-gradient(circle at 94% 4%,rgba(175,82,222,.10),transparent 28%),var(--page)}}button,input,select{{font:inherit}}button{{cursor:pointer}}.shell{{width:min(1680px,100%);margin:auto;padding:22px 24px 42px}}
.hero{{position:sticky;top:0;z-index:20;margin-bottom:16px;padding:17px 18px 14px;border:1px solid var(--border-soft);border-radius:24px;background:rgba(255,255,255,.74);box-shadow:var(--shadow);backdrop-filter:blur(30px) saturate(165%);-webkit-backdrop-filter:blur(30px) saturate(165%)}}
.hero-top{{display:flex;align-items:center;justify-content:space-between;gap:18px}}.brand{{display:flex;align-items:center;gap:12px;min-width:0}}.brand-mark{{position:relative;width:38px;height:38px;flex:0 0 auto;border-radius:13px;background:linear-gradient(145deg,rgba(10,132,255,.17),rgba(175,82,222,.15));border:1px solid rgba(255,255,255,.9);box-shadow:inset 0 1px rgba(255,255,255,.8),0 8px 24px rgba(10,132,255,.13)}}.brand-mark:before,.brand-mark:after{{content:"";position:absolute;border-radius:4px;background:linear-gradient(135deg,var(--primary),#5e5ce6)}}.brand-mark:before{{width:15px;height:4px;left:11px;top:10px;box-shadow:0 7px 0 rgba(94,92,230,.85),0 14px 0 rgba(175,82,222,.78)}}.brand-mark:after{{width:4px;height:18px;right:8px;top:10px;opacity:.35}}h1{{margin:0;font-size:17px;font-weight:720;letter-spacing:-.025em}}.eyebrow{{margin:0 0 3px;color:var(--muted);font-size:9px;font-weight:750;letter-spacing:.13em;text-transform:uppercase}}#summary{{margin-top:3px;max-width:900px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--secondary);font-size:11px}}.status-chip{{display:inline-flex;align-items:center;gap:7px;flex:0 0 auto;padding:7px 10px;border:1px solid rgba(48,209,88,.14);border-radius:999px;background:var(--success-soft);color:#1f9d48;font-size:10px;font-weight:700}}.status-chip:before{{content:"";width:6px;height:6px;border-radius:50%;background:var(--success);box-shadow:0 0 0 4px rgba(48,209,88,.10)}}
.toolbar{{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-top:14px;padding-top:13px;border-top:1px solid var(--border)}}.segmented{{display:flex;gap:3px;padding:3px;border-radius:12px;background:rgba(118,118,128,.09)}}.toolbar button,.toolbar select,.toolbar input[type=text]{{height:32px;border:1px solid transparent;border-radius:10px;background:rgba(118,118,128,.07);color:var(--secondary);outline:none;transition:.18s ease}}.toolbar button{{padding:0 11px;font-size:11px;font-weight:650}}.toolbar button:hover{{background:rgba(118,118,128,.12);color:var(--text)}}.toolbar button.active{{background:var(--surface-strong);color:var(--text);box-shadow:0 2px 8px rgba(31,35,48,.08)}}.nav-actions{{display:flex;gap:4px}}.toolbar input[type=text]{{min-width:220px;flex:1;padding:0 11px;border-color:var(--border);background:rgba(255,255,255,.55);font-size:11px}}.toolbar input[type=text]:focus{{border-color:rgba(10,132,255,.35);box-shadow:0 0 0 3px rgba(10,132,255,.08)}}.toolbar select{{padding:0 30px 0 10px;border-color:var(--border);background-color:rgba(255,255,255,.55);font-size:11px}}.check{{display:inline-flex;align-items:center;gap:7px;padding:0 4px;color:var(--secondary);font-size:11px;white-space:nowrap}}.check input{{width:15px;height:15px;accent-color:var(--primary)}}
main{{display:grid;grid-template-columns:270px minmax(0,1fr);gap:16px;align-items:start}}nav{{position:sticky;top:var(--review-sticky-offset);max-height:calc(100vh - var(--review-sticky-offset) - 18px);overflow:auto;padding:10px;border:1px solid var(--border-soft);border-radius:20px;background:var(--surface);box-shadow:0 14px 44px rgba(31,35,48,.055);backdrop-filter:blur(24px) saturate(150%);transition:top .16s ease,max-height .16s ease}}nav:before{{content:"CHANGED FILES";display:block;padding:7px 9px 9px;color:var(--muted);font-size:9px;font-weight:760;letter-spacing:.13em}}nav a{{position:relative;display:block;margin:2px 0;padding:10px 10px 10px 25px;border-radius:11px;color:var(--secondary);text-decoration:none;font-size:11px;font-weight:590;line-height:1.35;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;transition:.16s ease}}nav a:before{{content:"";position:absolute;left:10px;top:50%;width:7px;height:7px;border-radius:50%;background:#ff9f0a;transform:translateY(-50%)}}nav a[data-status="add"]:before{{background:var(--success)}}nav a[data-status="delete"]:before{{background:var(--danger)}}nav a:hover{{background:rgba(10,132,255,.075);color:var(--text)}}nav a.viewed{{color:var(--muted)}}nav a.viewed:after{{content:"✓";position:absolute;right:9px;color:var(--success);font-size:10px}}
section{{min-width:0}}article{{overflow:hidden;margin-bottom:16px;scroll-margin-top:calc(var(--review-sticky-offset) + 10px);border:1px solid var(--border-soft);border-radius:20px;background:var(--surface);box-shadow:0 16px 46px rgba(31,35,48,.055);backdrop-filter:blur(22px) saturate(145%)}}.fh{{position:sticky;top:var(--review-sticky-offset);z-index:4;display:flex;align-items:center;gap:8px;padding:12px 14px;border-bottom:1px solid var(--border);background:rgba(251,251,253,.88);backdrop-filter:blur(20px);font:650 11px ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;transition:top .16s ease}}.fh .meta{{margin-left:auto;padding:4px 8px;border-radius:999px;background:rgba(118,118,128,.075);color:var(--muted);font:650 9px -apple-system,BlinkMacSystemFont,system-ui,sans-serif}}pre{{margin:0;overflow:auto;padding:8px 0 12px;font:11px/20px ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace}}.line{{display:grid;grid-template-columns:74px minmax(max-content,1fr);min-height:20px;white-space:pre;color:#3b3b40}}.line code{{display:block;padding:0 14px;font:inherit}}.ln{{display:grid;grid-template-columns:1fr 1fr;gap:0;border-right:1px solid var(--border);color:#a1a1aa;font-style:normal;text-align:right;user-select:none}}.ln b{{padding:0 7px;font-weight:450}}.add{{background:linear-gradient(90deg,rgba(48,209,88,.115),rgba(48,209,88,.045))}}.del{{background:linear-gradient(90deg,rgba(255,69,58,.105),rgba(255,69,58,.035))}}.add code{{color:#1f6f3c}}.del code{{color:#a63530}}.hdr{{margin:7px 0;background:linear-gradient(90deg,rgba(10,132,255,.095),rgba(94,92,230,.035));color:#356ba7}}.hdr .ln{{border-color:rgba(10,132,255,.10)}}.hdr code{{font-weight:650}}.split-grid{{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);border-top:0}}.split-grid>div:nth-child(odd){{border-right:1px solid var(--border)}}.split-grid .line{{grid-template-columns:54px minmax(max-content,1fr)}}.split-grid .ln{{grid-template-columns:1fr}}
@media(prefers-color-scheme:dark){{:root{{color-scheme:dark;--page:#0f0f11;--surface:rgba(37,37,41,.72);--surface-strong:rgba(49,49,54,.94);--border:rgba(255,255,255,.09);--border-soft:rgba(255,255,255,.08);--text:#f5f5f7;--secondary:#aeaeb7;--muted:#74747d;--shadow:0 22px 70px rgba(0,0,0,.34)}}body{{background:radial-gradient(circle at 12% -8%,rgba(10,132,255,.16),transparent 31%),radial-gradient(circle at 94% 4%,rgba(175,82,222,.11),transparent 28%),var(--page)}}.hero{{background:rgba(28,28,31,.78);border-color:var(--border)}}.toolbar input[type=text],.toolbar select{{background:rgba(255,255,255,.055)}}nav,article{{background:var(--surface);border-color:var(--border)}}.fh{{background:rgba(30,30,33,.9)}}.line{{color:#d5d5da}}.add code{{color:#79d794}}.del code{{color:#ff8d86}}.hdr{{color:#78aef0}}}}
@media(max-width:860px){{:root{{--review-sticky-offset:0px}}.shell{{padding:12px}}.hero{{position:relative;border-radius:18px}}.hero-top{{align-items:flex-start}}.status-chip{{display:none}}.toolbar{{gap:6px}}.toolbar input[type=text]{{order:5;min-width:100%;flex-basis:100%}}main{{display:block}}nav{{position:relative;top:auto;display:flex;max-height:none;margin-bottom:12px;overflow:auto;border-radius:16px}}nav:before{{display:none}}nav a{{flex:0 0 auto;max-width:230px;padding-right:28px}}article{{scroll-margin-top:12px;border-radius:16px}}.fh{{position:relative;top:auto}}.split-grid{{grid-template-columns:1fr}}.split-grid>div:nth-child(odd){{border-right:0;border-bottom:1px solid var(--border)}}}}
</style></head><body><div class="shell"><header class="hero"><div class="hero-top"><div class="brand"><div class="brand-mark"></div><div class="brand-copy"><p class="eyebrow">REMOTE CODE REVIEW</p><h1>AI Change Review</h1><div id="summary"></div></div></div><div class="status-chip">Read-only snapshot</div></div><div class="toolbar"><div class="segmented"><button id="unified">Unified</button><button id="split">Split</button></div><div class="nav-actions"><button id="prev">Previous</button><button id="next">Next</button><button id="collapse">Collapse all</button></div><input id="filter" type="text" placeholder="Search path or diff content"><select id="status"><option value="">All status</option><option value="add">Added</option><option value="update">Modified</option><option value="delete">Deleted</option><option value="rename">Renamed</option></select><label class="check"><input id="ws" type="checkbox"> Ignore whitespace</label></div></header><main><nav id="files"></nav><section id="diff"></section></main></div>
<script>const r={data};const esc=s=>s.replace(/[&<>"]/g,c=>({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}}[c]));let mode=localStorage.reviewMode||'unified';const viewed=new Set(JSON.parse(localStorage.getItem('reviewViewed:'+r.id)||'[]'));summary.textContent=r.stats.files+' files · +'+r.stats.additions+' -'+r.stats.deletions+' · '+r.summary;
const hero=document.querySelector('.hero');const syncStickyOffset=()=>{{if(innerWidth<=860){{document.documentElement.style.setProperty('--review-sticky-offset','0px');return}}const top=parseFloat(getComputedStyle(hero).top)||0;document.documentElement.style.setProperty('--review-sticky-offset',Math.ceil(top+hero.offsetHeight+12)+'px')}};new ResizeObserver(syncStickyOffset).observe(hero);addEventListener('resize',syncStickyOffset,{{passive:true}});syncStickyOffset();
function parsed(lines){{let old=0,neu=0;return lines.map(l=>{{let m=l.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)/);if(m){{old=+m[1];neu=+m[2];return{{t:'h',s:l}}}}if(l.startsWith('---')||l.startsWith('+++'))return{{t:'m',s:l}};let t=l[0]==='+'?'a':l[0]==='-'?'d':'c',o=t==='a'?'':old++,n=t==='d'?'':neu++;return{{t,s:l.slice(1),o,n}}}})}}
function rows(lines){{let p=parsed(lines),out=[];for(let i=0;i<p.length;i++){{let x=p[i];if(x.t==='d'){{let dels=[];while(p[i]?.t==='d')dels.push(p[i++]);let adds=[];while(p[i]?.t==='a')adds.push(p[i++]);i--;for(let j=0;j<Math.max(dels.length,adds.length);j++)out.push([dels[j],adds[j]])}}else if(x.t==='a')out.push([null,x]);else out.push([x,x])}}return out}}
const cell=x=>x?'<span class="line '+(x.t==='a'?'add':x.t==='d'?'del':x.t==='h'?'hdr':'')+'"><i class="ln"><b>'+esc(String(x.o||''))+'</b><b>'+esc(String(x.n||''))+'</b></i><code>'+esc(x.s)+'</code></span>':'<span class="line"><i class="ln"><b></b><b></b></i><code></code></span>';
function render(){{files.innerHTML='';diff.innerHTML='';unified.classList.toggle('active',mode==='unified');split.classList.toggle('active',mode==='split');const q=filter.value.toLowerCase();r.files.filter(f=>(!status.value||f.status===status.value)&&(f.path+' '+f.patch).toLowerCase().includes(q)).forEach((f,i)=>{{const key=f.path;const a=document.createElement('a');a.href='#f'+i;a.dataset.status=f.status;a.classList.toggle('viewed',viewed.has(key));a.textContent=f.path;files.appendChild(a);let lines=f.patch.split('\n');let rr=rows(lines);if(ws.checked)rr=rr.filter(x=>!(x[0]?.t==='d'&&x[1]?.t==='a'&&x[0].s.replace(/\s/g,'')===x[1].s.replace(/\s/g,'')));const article=document.createElement('article');article.id='f'+i;article.onclick=()=>{{viewed.add(key);localStorage.setItem('reviewViewed:'+r.id,JSON.stringify([...viewed]));a.classList.add('viewed')}};const body=(ws.checked?rr.flat().filter(Boolean):parsed(lines)).map(cell).join('');const splitBody=rr.map(x=>'<div>'+cell(x[0])+'</div><div>'+cell(x[1])+'</div>').join('');article.innerHTML='<div class="fh"><span>'+esc(f.path)+'</span><span class="meta">'+f.status+' · +'+f.additions+' -'+f.deletions+'</span></div>'+(mode==='split'?'<div class="split-grid">'+splitBody+'</div>':'<pre>'+body+'</pre>');diff.appendChild(article)}})}}unified.onclick=()=>{{mode='unified';localStorage.reviewMode=mode;render()}};split.onclick=()=>{{mode='split';localStorage.reviewMode=mode;render()}};filter.oninput=render;status.onchange=render;ws.onchange=render;collapse.onclick=()=>document.querySelectorAll('article').forEach(x=>x.style.display='none');let cur=-1;const jump=d=>{{let a=[...document.querySelectorAll('article')];if(!a.length)return;cur=(cur+d+a.length)%a.length;a[cur].scrollIntoView({{behavior:'smooth',block:'start'}})}};prev.onclick=()=>jump(-1);next.onclick=()=>jump(1);document.onkeydown=e=>{{if(e.key==='j'&&document.activeElement!==filter)jump(1);if(e.key==='k'&&document.activeElement!==filter)jump(-1)}};render();</script></body></html>"#);
    ([(CACHE_CONTROL, "no-store"),(X_CONTENT_TYPE_OPTIONS,"nosniff"),(REFERRER_POLICY,"no-referrer"),(CONTENT_SECURITY_POLICY,"default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; base-uri 'none'; frame-ancestors 'none'")], axum::response::Html(html)).into_response()
}

fn bind_listener(port: u16, allow_lan_access: bool) -> Result<tokio::net::TcpListener, String> {
    let addr = local_network::bind_addr(port, allow_lan_access);
    let listener = std::net::TcpListener::bind(addr)
        .map_err(|err| format!("MCP 本地端口 {port} 绑定失败: {err}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("MCP 本地端口 {port} 设置非阻塞失败: {err}"))?;
    tokio::net::TcpListener::from_std(listener)
        .map_err(|err| format!("MCP 本地监听器初始化失败: {err}"))
}

async fn mcp_discovery() -> Response {
    ([(CACHE_CONTROL, "no-store")], Json(mcp_discovery_payload())).into_response()
}

fn mcp_discovery_payload() -> Value {
    json!({
        "name": "coding-tools-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "protocolVersion": crate::mcp::LATEST_PROTOCOL_VERSION,
        "supportedProtocolVersions": crate::mcp::SUPPORTED_PROTOCOL_VERSIONS
    })
}

fn resolve_oauth_base(state: &ListenerState, headers: &HeaderMap) -> String {
    external_base_url(headers, state.bind_port, &state.configured_public_url)
}

async fn mcp_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_mcp_auth(&state, &headers) {
        return response;
    }
    let meta = RpcRequestMeta::from_body(&body);

    let requested_session_id = session_id_from_request(&headers, &body);
    let host_session_id = if headers.get(MCP_SESSION_HEADER).is_none() {
        session_id_from_metadata(&body)
    } else {
        None
    };
    if meta.method != "initialize" {
        if let Some(session_id) = host_session_id.as_deref() {
            state.gateway.sessions.ensure_host_session(session_id);
        }
    }
    let effective_session_id = if meta.method == "initialize" {
        match requested_session_id.as_deref() {
            Some(session_id) if state.gateway.sessions.contains(session_id) => {
                Some(session_id.to_string())
            }
            _ => Some(state.gateway.sessions.issue()),
        }
    } else {
        requested_session_id
    };
    let response_session_id = effective_session_id
        .as_deref()
        .filter(|session_id| state.gateway.sessions.contains(session_id))
        .map(str::to_string);
    log_request(&state.scope_id, effective_session_id.as_deref(), &meta);

    let gateway = state.gateway.clone();
    let body_for_worker = body.clone();
    let session_for_worker = effective_session_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        handle_gateway_request(
            gateway.as_ref(),
            session_for_worker.as_deref(),
            &body_for_worker,
        )
    })
    .await;
    match result {
        Ok(gateway_response) => {
            let response = gateway_response.body;
            if let Some(workspace) = gateway_response.workspace.as_ref() {
                record_response(
                    &workspace.tools,
                    &workspace.workspace_id,
                    effective_session_id.as_deref(),
                    &meta,
                    &response,
                );
            }
            let mut http_response = Json(response).into_response();
            if let Some(session_id) = response_session_id {
                if let Ok(value) = session_id.parse() {
                    http_response
                        .headers_mut()
                        .insert(MCP_SESSION_HEADER, value);
                }
            }
            http_response
        }
        Err(error) => {
            let error_response = json!({
                "jsonrpc": "2.0",
                "id": meta.request_id.clone(),
                "error": {
                    "code": -32603,
                    "message": "Exec RPC worker failed",
                    "data": {
                        "stage": "rpc_worker",
                        "reason": "worker_failed",
                        "retryable": true,
                        "suggestion": "重试请求或重启 MCP 运行时"
                    }
                }
            });
            append_profile_log(
                &state.scope_id,
                "mcp-requests.log",
                &format!(
                    "[rpc] worker_failed id={} method={} tool={} error={error}",
                    meta.request_id, meta.method, meta.tool_name
                ),
            );
            Json(error_response).into_response()
        }
    }
}

fn require_mcp_auth(state: &ListenerState, headers: &HeaderMap) -> Option<Response> {
    if state.auth.bearer_enabled() {
        let Some(expected) = state
            .bearer_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            return Some(
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Bearer authentication is enabled but no token is configured",
                )
                    .into_response(),
            );
        };
        return verify_bearer_header(headers, expected);
    }
    if state.auth.oauth_enabled() {
        if let Some(oauth) = state.oauth.as_ref() {
            let server_url = resolve_oauth_base(state, headers);
            match verify_oauth_bearer_header(headers, oauth, &server_url) {
                None => return None,
                Some(mut response) => {
                    if response.status() == StatusCode::UNAUTHORIZED {
                        let metadata_url = protected_resource_metadata_url(&server_url);
                        if let Ok(value) =
                            format!("Bearer resource_metadata=\"{metadata_url}\"").parse()
                        {
                            response.headers_mut().insert(WWW_AUTHENTICATE, value);
                        }
                    }
                    return Some(response);
                }
            }
        }
    }
    if state.auth.auth_enabled() {
        return Some(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Unsupported or incomplete MCP authentication configuration",
            )
                .into_response(),
        );
    }
    None
}

async fn oauth_authorization_server_metadata(
    State(state): State<ListenerState>,
    headers: HeaderMap,
) -> Response {
    if !state.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    let base = resolve_oauth_base(&state, &headers);
    Json(authorization_server_metadata(
        &base,
        state.oauth_client_secret.as_deref(),
    ))
    .into_response()
}

async fn oauth_protected_resource_metadata(
    State(state): State<ListenerState>,
    headers: HeaderMap,
) -> Response {
    if !state.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    Json(protected_resource_metadata(&resolve_oauth_base(
        &state, &headers,
    )))
    .into_response()
}

async fn oauth_authorize_get(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_get(
        oauth,
        params,
        Some(state.scope_label.as_str()),
        &resolve_oauth_base(&state, &headers),
    )
}

async fn oauth_authorize_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Form(form): Form<AuthorizeForm>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_post(oauth, form, &resolve_oauth_base(&state, &headers))
}

async fn oauth_token_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unsupported_grant_type" })),
        )
            .into_response();
    };
    token_exchange(oauth, &headers, form, &resolve_oauth_base(&state, &headers))
}

fn oauth_not_configured() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "OAuth not configured" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::{fs, path::Path};

    use axum::http::header::{AUTHORIZATION, CACHE_CONTROL};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;
    use serde_json::json;

    use crate::auth::OAuthRuntime;
    use crate::mcp::gateway::GatewayState;
    use crate::settings::AppSettings;
    use crate::workspace::{AuthConfig, WorkspaceProfile};

    use super::{
        bind_listener, build_router, mcp_discovery, mcp_discovery_payload, require_mcp_auth,
        ListenerState,
    };

    #[test]
    fn bind_listener_reports_port_conflict_synchronously() {
        let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("占用测试端口");
        let port = occupied.local_addr().expect("读取测试端口").port();

        assert!(bind_listener(port, false).is_err());
    }

    #[tokio::test]
    async fn discovery_reports_the_current_package_version() {
        let discovery = mcp_discovery_payload();

        assert_eq!(discovery["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn discovery_prevents_stale_tool_catalog_caching() {
        let response = mcp_discovery().await.into_response();

        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    }

    #[test]
    fn mcp_auth_fails_closed_for_missing_bearer_and_unknown_mode() {
        let mut state = test_listener_state();
        state.auth.auth_type = "bearer".into();
        let missing_bearer =
            require_mcp_auth(&state, &HeaderMap::new()).expect("missing bearer must fail");
        assert_eq!(missing_bearer.status(), StatusCode::SERVICE_UNAVAILABLE);

        state.auth.auth_type = "unexpected".into();
        let invalid_mode =
            require_mcp_auth(&state, &HeaderMap::new()).expect("unknown auth must fail closed");
        assert_eq!(invalid_mode.status(), StatusCode::SERVICE_UNAVAILABLE);

        state.auth.auth_type = "noauth".into();
        assert!(require_mcp_auth(&state, &HeaderMap::new()).is_none());
    }

    #[test]
    fn mcp_auth_allows_a_valid_oauth_bearer() {
        #[derive(Serialize)]
        struct Claims {
            iss: String,
            aud: String,
            iat: i64,
            exp: i64,
            scope: String,
            client_id: String,
            token_use: String,
        }

        let mut state = test_listener_state();
        state.auth.auth_type = "oauth".into();
        state.configured_public_url = "https://coding-mcp.lengsu.top".into();
        let token_secret = "oauth-token-secret";
        state.oauth = Some(Arc::new(OAuthRuntime::new(
            "https://coding-mcp.lengsu.top".into(),
            "test-client".into(),
            None,
            "test-password".into(),
            token_secret.into(),
        )));

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_secs() as i64;
        let token = encode(
            &Header::new(Algorithm::HS256),
            &Claims {
                iss: "https://coding-mcp.lengsu.top".into(),
                aud: "https://coding-mcp.lengsu.top".into(),
                iat: now,
                exp: now + 60,
                scope: "mcp".into(),
                client_id: "test-client".into(),
                token_use: "access".into(),
            },
            &EncodingKey::from_secret(token_secret.as_bytes()),
        )
        .expect("oauth access token");

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {token}")
                .parse()
                .expect("authorization header"),
        );

        assert!(require_mcp_auth(&state, &headers).is_none());
    }

    fn listener_state(profiles: Vec<WorkspaceProfile>) -> ListenerState {
        let auth = AuthConfig {
            auth_type: "noauth".into(),
            ..AuthConfig::default()
        };
        ListenerState {
            gateway: Arc::new(GatewayState::for_test_profiles(
                profiles,
                AppSettings::default(),
            )),
            auth,
            scope_id: "global-mcp".into(),
            scope_label: "Global MCP Gateway".into(),
            bind_port: 0,
            configured_public_url: String::new(),
            bearer_token: None,
            oauth: None,
            oauth_client_secret: None,
        }
    }

    fn test_listener_state() -> ListenerState {
        let mut profile = WorkspaceProfile::new(
            std::env::temp_dir().display().to_string(),
            Some("HTTP Test Workspace".into()),
        );
        profile.id = "http-test-workspace".into();
        listener_state(vec![profile])
    }

    async fn initialize_session(client: &reqwest::Client, url: &str) -> String {
        let response = client
            .post(url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2025-11-25" }
            }))
            .send()
            .await
            .expect("initialize request");
        assert!(response.status().is_success());
        response
            .headers()
            .get(super::MCP_SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .expect("server-issued session header")
            .to_string()
    }

    async fn call_tool(
        client: &reqwest::Client,
        url: &str,
        session_id: &str,
        id: u64,
        name: &str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
        client
            .post(url)
            .header(super::MCP_SESSION_HEADER, session_id)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }))
            .send()
            .await
            .expect("tool request")
            .json()
            .await
            .expect("tool response")
    }

    #[tokio::test]
    async fn http_transport_issues_and_requires_server_owned_sessions() {
        let app = build_router(test_listener_state());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test HTTP listener");
        let address = listener.local_addr().expect("test listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test HTTP server");
        });
        let client = reqwest::Client::new();
        let url = format!("http://{address}/mcp");

        let initialize = client
            .post(&url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2025-11-25" }
            }))
            .send()
            .await
            .expect("initialize request");
        assert!(initialize.status().is_success());
        let session_id = initialize
            .headers()
            .get(super::MCP_SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .expect("server-issued session header")
            .to_string();
        assert!(!session_id.is_empty());

        let current = client
            .post(&url)
            .header(super::MCP_SESSION_HEADER, &session_id)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "workspace_current", "arguments": {} }
            }))
            .send()
            .await
            .expect("workspace_current request");
        let current_body: serde_json::Value = current.json().await.expect("current response");
        assert_eq!(
            current_body["result"]["structuredContent"]["selected"],
            false
        );

        let forged = client
            .post(&url)
            .header(super::MCP_SESSION_HEADER, "client-chosen-session")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "workspace_current", "arguments": {} }
            }))
            .send()
            .await
            .expect("forged session request");
        assert!(forged.headers().get(super::MCP_SESSION_HEADER).is_none());
        let forged_body: serde_json::Value = forged.json().await.expect("forged response");
        assert_eq!(
            forged_body["error"]["data"]["reason"],
            "MCP_SESSION_INVALID"
        );

        server.abort();
    }

    #[tokio::test]
    async fn http_host_metadata_session_keeps_workspace_selection_without_transport_header() {
        let first = tempfile::tempdir().expect("workspace a");
        let second = tempfile::tempdir().expect("workspace b");
        fs::write(first.path().join("marker.txt"), "ONLY-A").expect("marker a");
        let mut first_profile =
            WorkspaceProfile::new(path_string(first.path()), Some("Workspace A".into()));
        first_profile.id = "workspace-a".into();
        let mut second_profile =
            WorkspaceProfile::new(path_string(second.path()), Some("Workspace B".into()));
        second_profile.id = "workspace-b".into();

        let app = build_router(listener_state(vec![first_profile, second_profile]));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test HTTP listener");
        let address = listener.local_addr().expect("test listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test HTTP server");
        });
        let client = reqwest::Client::new();
        let url = format!("http://{address}/mcp");

        let call = |id: u64, name: &str, arguments: serde_json::Value| {
            client.post(&url).json(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments,
                    "_meta": { "openai/session": "chatgpt-conversation-a" }
                }
            }))
        };

        let selected: serde_json::Value = call(
            1,
            "workspace_select",
            json!({ "workspace_id": "workspace-a" }),
        )
        .send()
        .await
        .expect("select request")
        .json()
        .await
        .expect("select response");
        assert_eq!(selected["result"]["structuredContent"]["ok"], true);

        let read: serde_json::Value = call(2, "read_file", json!({ "path": "marker.txt" }))
            .send()
            .await
            .expect("read request")
            .json()
            .await
            .expect("read response");
        assert_eq!(read["result"]["structuredContent"]["content"], "ONLY-A");
        server.abort();
    }

    #[tokio::test]
    async fn initialize_replaces_client_supplied_session_id() {
        let app = build_router(test_listener_state());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test HTTP listener");
        let address = listener.local_addr().expect("test listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test HTTP server");
        });
        let response = reqwest::Client::new()
            .post(format!("http://{address}/mcp"))
            .header(super::MCP_SESSION_HEADER, "client-chosen-session")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {}
            }))
            .send()
            .await
            .expect("initialize request");
        let session_id = response
            .headers()
            .get(super::MCP_SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .expect("server-issued session header");
        assert_ne!(session_id, "client-chosen-session");
        server.abort();
    }

    #[tokio::test]
    async fn http_sessions_keep_workspace_selection_isolated() {
        let first = tempfile::tempdir().expect("workspace a");
        let second = tempfile::tempdir().expect("workspace b");
        fs::write(first.path().join("marker.txt"), "ONLY-A").expect("marker a");
        fs::write(second.path().join("marker.txt"), "ONLY-B").expect("marker b");
        let mut first_profile =
            WorkspaceProfile::new(path_string(first.path()), Some("Workspace A".into()));
        first_profile.id = "workspace-a".into();
        let mut second_profile =
            WorkspaceProfile::new(path_string(second.path()), Some("Workspace B".into()));
        second_profile.id = "workspace-b".into();

        let app = build_router(listener_state(vec![first_profile, second_profile]));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test HTTP listener");
        let address = listener.local_addr().expect("test listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test HTTP server");
        });
        let client = reqwest::Client::new();
        let url = format!("http://{address}/mcp");
        let session_a = initialize_session(&client, &url).await;
        let session_b = initialize_session(&client, &url).await;

        let selected_a = call_tool(
            &client,
            &url,
            &session_a,
            2,
            "workspace_select",
            json!({ "workspace_id": "workspace-a" }),
        )
        .await;
        let selected_b = call_tool(
            &client,
            &url,
            &session_b,
            3,
            "workspace_select",
            json!({ "workspace_id": "workspace-b" }),
        )
        .await;
        assert_eq!(selected_a["result"]["structuredContent"]["ok"], true);
        assert_eq!(selected_b["result"]["structuredContent"]["ok"], true);

        let read_a = call_tool(
            &client,
            &url,
            &session_a,
            4,
            "read_file",
            json!({ "path": "marker.txt" }),
        )
        .await;
        let read_b = call_tool(
            &client,
            &url,
            &session_b,
            5,
            "read_file",
            json!({ "path": "marker.txt" }),
        )
        .await;
        assert_eq!(read_a["result"]["structuredContent"]["content"], "ONLY-A");
        assert_eq!(read_b["result"]["structuredContent"]["content"], "ONLY-B");
        server.abort();
    }

    #[tokio::test]
    async fn http_request_scoped_workspace_id_survives_transport_session_churn() {
        let first = tempfile::tempdir().expect("workspace a");
        let second = tempfile::tempdir().expect("workspace b");
        fs::write(first.path().join("marker.txt"), "ONLY-A").expect("marker a");
        fs::write(second.path().join("marker.txt"), "ONLY-B").expect("marker b");
        let mut first_profile =
            WorkspaceProfile::new(path_string(first.path()), Some("Workspace A".into()));
        first_profile.id = "workspace-a".into();
        let mut second_profile =
            WorkspaceProfile::new(path_string(second.path()), Some("Workspace B".into()));
        second_profile.id = "workspace-b".into();

        let app = build_router(listener_state(vec![first_profile, second_profile]));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test HTTP listener");
        let address = listener.local_addr().expect("test listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test HTTP server");
        });
        let client = reqwest::Client::new();
        let url = format!("http://{address}/mcp");
        let session_a = initialize_session(&client, &url).await;
        let session_b = initialize_session(&client, &url).await;

        let tools: serde_json::Value = client
            .post(&url)
            .header(super::MCP_SESSION_HEADER, &session_a)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .expect("tools/list request")
            .json()
            .await
            .expect("tools/list response");
        let read_file = tools["result"]["tools"]
            .as_array()
            .expect("tool catalog")
            .iter()
            .find(|tool| tool["name"] == "read_file")
            .expect("read_file tool");
        assert!(read_file["inputSchema"]["properties"]
            .get("workspace_id")
            .is_some());

        let read_a = call_tool(
            &client,
            &url,
            &session_a,
            3,
            "read_file",
            json!({ "workspace_id": "workspace-a", "path": "marker.txt" }),
        )
        .await;
        let read_after_churn = call_tool(
            &client,
            &url,
            &session_b,
            4,
            "read_file",
            json!({ "workspace_id": "workspace-a", "path": "marker.txt" }),
        )
        .await;
        assert_eq!(read_a["result"]["structuredContent"]["content"], "ONLY-A");
        assert_eq!(
            read_after_churn["result"]["structuredContent"]["content"],
            "ONLY-A"
        );
        server.abort();
    }

    fn path_string(path: &Path) -> String {
        path.display().to_string()
    }
}
