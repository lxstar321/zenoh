use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::{
    rustls::{self, version::TLS13, ServerConfig, server::WebPkiClientVerifier, pki_types::{CertificateDer, PrivateKeyDer}},
    TlsAcceptor, client::TlsStream,
};

/// 统一的异步流trait
trait AsyncStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> AsyncStream for T {}

/// 流类型的枚举，用于处理TLS和TCP流
enum StreamType {
    Tcp(TcpStream),
    Tls(Box<dyn AsyncStream>),
}

impl StreamType {
    async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            StreamType::Tcp(stream) => stream.read(buf).await,
            StreamType::Tls(stream) => stream.read(buf).await,
        }
    }

    async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            StreamType::Tcp(stream) => stream.write_all(buf).await,
            StreamType::Tls(stream) => stream.write_all(buf).await,
        }
    }
}
use x509_parser::prelude::*;
use std::net::SocketAddr;
use tracing::{debug, error, info, warn};
use zenoh::{
    internal::{
        plugins::{RunningPluginTrait, ZenohPlugin},
        runtime::DynamicRuntime,
    },
    Result as ZResult,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};
use zenoh_util::ffi::JsonKeyValueMap;
use zenoh::prelude::*;
use zenoh::transport::tls::{TlsConfig, HandshakeEvent};

lazy_static::lazy_static! {
    static ref TOKIO_RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(50)
        .enable_all()
        .build()
        .expect("Unable to create runtime");
}

#[inline(always)]
fn spawn_runtime(task: impl Future<Output = ()> + Send + 'static) {
    // Check whether able to get the current runtime
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => {
            // Able to get the current runtime (standalone binary), spawn on the current runtime
            rt.spawn(task);
        }
        Err(_) => {
            // Unable to get the current runtime (dynamic plugins), spawn on the global runtime
            TOKIO_RUNTIME.spawn(task);
        }
    }
}

// ============================================================================
// 数据模型
// ============================================================================

/// 客户端权限信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClientPermissions {
    pub client_id: String,
    pub permissions: Vec<String>,
    pub session_id: String,
}

/// 鉴权服务响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthResponse {
    pub is_authorized: bool,
    pub permissions: Vec<String>,
    pub client_info: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

/// 权限缓存项
#[derive(Debug, Clone)]
pub struct PermissionCacheEntry {
    pub permissions: ClientPermissions,
    pub created_at: std::time::Instant,
}

/// 插件统计信息
#[derive(Debug, Default)]
pub struct GatewayAuthStats {
    pub total_auth_requests: u64,
    pub successful_auths: u64,
    pub failed_auths: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub messages_allowed: u64,
    pub messages_denied: u64,
    pub http_requests: u64,
    pub http_errors: u64,
}

/// 插件配置
#[derive(Debug, Clone)]
pub struct GatewayAuthConfig {
    // 代理模式配置
    pub listen_address: String,
    pub backend_address: String,
    pub enable_proxy_mode: bool,

    // TLS配置
    pub enable_mtls: bool,
    pub ca_certificate: Option<String>,
    pub server_certificate: Option<String>,
    pub server_private_key: Option<String>,

    // 认证服务配置
    pub auth_service_url: String,
    pub request_timeout_seconds: u64,

    // 缓存配置
    pub cache_ttl_seconds: u64,

    // 调试配置
    pub enable_debug_logging: bool,
}

impl Default for GatewayAuthConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0:7447".to_string(),
            backend_address: "127.0.0.1:7448".to_string(),
            enable_proxy_mode: true,
            enable_mtls: true,
            ca_certificate: None,
            server_certificate: None,
            server_private_key: None,
            auth_service_url: "http://localhost:8080".to_string(),
            request_timeout_seconds: 10,
            cache_ttl_seconds: 3600,
            enable_debug_logging: false,
        }
    }
}

/// 流类型枚举
// The struct implementing the ZenohPlugin and ZenohPlugin traits
pub struct GatewayAuthPlugin {}

// declaration of the plugin's VTable for zenohd to find the plugin's functions to be called
#[cfg(feature = "dynamic_plugin")]
zenoh_plugin_trait::declare_plugin!(GatewayAuthPlugin);

impl ZenohPlugin for GatewayAuthPlugin {}

impl Plugin for GatewayAuthPlugin {
    type StartArgs = DynamicRuntime;
    type Instance = zenoh::internal::plugins::RunningPlugin;

    // A mandatory const to define, in case of the plugin is built as a standalone executable
    const DEFAULT_NAME: &'static str = "gateway_auth";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    // The first operation called by zenohd on the plugin
    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::Instance> {
        eprintln!("=== GATEWAY PLUGIN START: {} ===", name);
        let config = runtime.get_config().get_plugin_config(name);
        let config = match config {
            Ok(c) => c,
            Err(e) => {
                eprintln!("=== GATEWAY PLUGIN ERROR: Config error for {}: {} ===", name, e);
                return Err(format!("Plugin configuration error: {}", e).into());
            }
        };
        eprintln!("=== GATEWAY PLUGIN CONFIG: {:?} ===", config);
        let map_cfg = config.as_object().unwrap();
        eprintln!("=== GATEWAY PLUGIN CONFIG PARSED ===");

        // 读取代理模式配置
        let listen_address = match map_cfg.get("listen_address") {
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => "0.0.0.0:7447".to_string(),
        };

        let backend_address = match map_cfg.get("backend_address") {
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => "127.0.0.1:7448".to_string(),
        };

        let enable_proxy_mode = match map_cfg.get("enable_proxy_mode") {
            Some(serde_json::Value::Bool(b)) => {
                eprintln!("=== GATEWAY PLUGIN: enable_proxy_mode = {} ===", b);
                *b
            },
            _ => {
                eprintln!("=== GATEWAY PLUGIN: enable_proxy_mode not found, using default: true ===");
                true
            },
        };

        // 读取TLS配置
        let enable_mtls = match map_cfg.get("enable_mtls") {
            Some(serde_json::Value::Bool(b)) => *b,
            _ => true,
        };

        let ca_certificate = match map_cfg.get("ca_certificate") {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            _ => None,
        };

        let server_certificate = match map_cfg.get("server_certificate") {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            _ => None,
        };

        let server_private_key = match map_cfg.get("server_private_key") {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            _ => None,
        };

        // 读取认证服务配置
        let auth_service_url = match map_cfg.get("auth_service_url") {
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => "http://localhost:8080".to_string(),
        };

        let request_timeout_seconds = match map_cfg.get("request_timeout_seconds") {
            Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(10),
            _ => 10,
        };

        // 读取缓存配置
        let cache_ttl_seconds = match map_cfg.get("cache_ttl_seconds") {
            Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(3600),
            _ => 3600,
        };

        // 读取调试配置
        let enable_debug_logging = match map_cfg.get("enable_debug_logging") {
            Some(serde_json::Value::Bool(b)) => *b,
            _ => false,
        };

        let plugin_config = GatewayAuthConfig {
            listen_address,
            backend_address,
            enable_proxy_mode,
            enable_mtls,
            ca_certificate,
            server_certificate,
            server_private_key,
            auth_service_url: auth_service_url.clone(),
            request_timeout_seconds,
            cache_ttl_seconds,
            enable_debug_logging,
        };

        info!("[{}] Gateway Auth Plugin started - Proxy: {} -> {}, Auth: {}",
            name, plugin_config.listen_address, plugin_config.backend_address, auth_service_url);
        info!("[{}] Proxy mode enabled: {}, mTLS enabled: {}", name, enable_proxy_mode, enable_mtls);

        // 创建HTTP客户端
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(request_timeout_seconds))
            .build()
            .map_err(|e| {
                error!("[{}] Failed to create HTTP client: {}", name, e);
                e
            })?;

        // a flag to end the plugin's loop when the plugin is removed from the config
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // 创建权限缓存
        let permission_cache = Arc::new(Mutex::new(HashMap::new()));

        let inner = Arc::new(Mutex::new(RunningPluginInner {
            flag: flag.clone(),
            name: name.into(),
            runtime: runtime.clone(),
            config: plugin_config,
            permission_cache,
            http_client,
            stats: Arc::new(Mutex::new(GatewayAuthStats::default())),
        }));

        // 如果启用代理模式，启动代理服务器
        if enable_proxy_mode {
            eprintln!("=== GATEWAY PLUGIN: Starting proxy server... ===");
            spawn_runtime(RunningPlugin::start_proxy_server(inner.clone(), flag));
            eprintln!("=== GATEWAY PLUGIN: spawn_runtime called ===");
        } else {
            eprintln!("=== GATEWAY PLUGIN: Proxy mode disabled ===");
        }

        eprintln!("=== GATEWAY PLUGIN: Creating RunningPlugin ===");
        // return a RunningPlugin to zenohd
        Ok(Box::new(RunningPlugin(inner)))
    }
}

// An inner-state for the RunningPlugin
struct RunningPluginInner {
    flag: Arc<std::sync::atomic::AtomicBool>,
    name: String,
    runtime: DynamicRuntime,
    config: GatewayAuthConfig,
    /// 权限缓存: session_id -> PermissionCacheEntry
    permission_cache: Arc<Mutex<HashMap<String, PermissionCacheEntry>>>,
    /// HTTP客户端
    http_client: reqwest::Client,
    /// 统计信息
    stats: Arc<Mutex<GatewayAuthStats>>,
}

// The RunningPlugin struct implementing the RunningPluginTrait trait
#[derive(Clone)]
struct RunningPlugin(Arc<Mutex<RunningPluginInner>>);

impl PluginControl for RunningPlugin {}

impl RunningPluginTrait for RunningPlugin {
    fn config_checker(
        &self,
        _path: &str,
        _old: &JsonKeyValueMap,
        _new: &JsonKeyValueMap,
    ) -> ZResult<Option<JsonKeyValueMap>> {
        // For simplicity, just accept any configuration change
        Ok(None)
    }
}

impl RunningPlugin {
    /// 启动代理服务器
    async fn start_proxy_server(inner: Arc<Mutex<RunningPluginInner>>, flag: Arc<std::sync::atomic::AtomicBool>) {
        eprintln!("=== START_PROXY_SERVER CALLED ===");
        info!("[GatewayAuth] 🚀 Starting proxy server...");

        // 提前获取配置，避免持有MutexGuard跨越await点
        let (listen_addr, backend_addr, enable_mtls, ca_cert_path, server_cert_path, server_key_path) = {
            let guard = inner.lock().unwrap();
            (
                guard.config.listen_address.clone(),
                guard.config.backend_address.clone(),
                guard.config.enable_mtls,
                guard.config.ca_certificate.clone(),
                guard.config.server_certificate.clone(),
                guard.config.server_private_key.clone(),
            )
        };

        // 创建TLS配置（如果启用mTLS）
        eprintln!("=== CREATING TLS ACCEPTOR ===");
        let tls_acceptor = if enable_mtls {
            eprintln!("=== mTLS ENABLED, creating TLS acceptor ===");
            eprintln!("=== CA: {:?}, Server: {:?}, Key: {:?} ===", ca_cert_path, server_cert_path, server_key_path);
            match Self::create_tls_acceptor(&ca_cert_path, &server_cert_path, &server_key_path) {
                Ok(acceptor) => {
                    eprintln!("=== TLS ACCEPTOR CREATED SUCCESSFULLY ===");
                    Some(acceptor)
                },
                Err(e) => {
                    eprintln!("=== TLS ACCEPTOR CREATION FAILED: {} ===", e);
                    error!("[GatewayAuth] Failed to create TLS acceptor: {}", e);
                    error!("[GatewayAuth] CA cert path: {:?}", ca_cert_path);
                    error!("[GatewayAuth] Server cert path: {:?}", server_cert_path);
                    error!("[GatewayAuth] Server key path: {:?}", server_key_path);
                    return;
                }
            }
        } else {
            eprintln!("=== mTLS DISABLED ===");
            info!("[GatewayAuth] mTLS disabled, using plain TCP proxy");
            None
        };

        // 初始化日志
        zenoh_util::init_log_from_env_or("info");

        info!("[GatewayAuth] Starting proxy server on {} -> {}", listen_addr, backend_addr);

        // 解析地址
        let listen_socket_addr: SocketAddr = match listen_addr.parse() {
            Ok(addr) => addr,
            Err(e) => {
                error!("[GatewayAuth] Invalid listen address {}: {}", listen_addr, e);
                return;
            }
        };

        let backend_socket_addr: SocketAddr = match backend_addr.parse() {
            Ok(addr) => addr,
            Err(e) => {
                error!("[GatewayAuth] Invalid backend address {}: {}", backend_addr, e);
                return;
            }
        };

        // 暂时不支持TLS，专注于基本代理功能

        // 启动TCP监听器
        eprintln!("=== BINDING TO LISTEN ADDRESS ===");
        let listener = match TcpListener::bind(listen_socket_addr).await {
            Ok(listener) => {
                eprintln!("=== BIND SUCCESSFUL ===");
                listener
            },
            Err(e) => {
                eprintln!("=== BIND FAILED: {} ===", e);
                error!("[GatewayAuth] Failed to bind to {}: {}", listen_addr, e);
                return;
            }
        };

        eprintln!("=== PROXY SERVER STARTED SUCCESSFULLY ===");
        info!("[GatewayAuth] Proxy server listening on {}", listen_addr);

        // 主循环
        info!("[GatewayAuth] Entering main accept loop");
        while flag.load(std::sync::atomic::Ordering::Relaxed) {
            // info!("[GatewayAuth] Waiting for client connection...");
            match tokio::time::timeout(Duration::from_millis(100), listener.accept()).await {
                Ok(Ok((client_stream, client_addr))) => {
                    // info!("[GatewayAuth] ACCEPTED: New connection from {}", client_addr);
                    if inner.lock().unwrap().config.enable_debug_logging {
                        debug!("[GatewayAuth] New connection from {}", client_addr);
                    }

                    let backend_addr_clone = backend_socket_addr;

                    let inner_clone = inner.clone();
                    let tls_acceptor_clone = tls_acceptor.clone();

                    // debug!("[GatewayAuth] Spawning handler for client {}", client_addr);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(
                            inner_clone,
                            client_stream,
                            tls_acceptor_clone,
                            backend_addr_clone,
                        ).await {
                            error!("[GatewayAuth] Error handling client {}: {}", client_addr, e);
                        }
                    });
                }
                Ok(Err(e)) => {
                    error!("[GatewayAuth] Accept error: {}", e);
                }
                Err(_) => {
                    // 超时，继续检查退出标志
                }
            }
        }

        info!("[GatewayAuth] Proxy server stopped");
    }

    async fn handle_client(
        inner: Arc<Mutex<RunningPluginInner>>,
        client_stream: TcpStream,
        tls_acceptor: Option<TlsAcceptor>,
        backend_addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // debug!("[GatewayAuth] >>> handle_client invoked for backend {}", backend_addr);
    
        // 前端 TLS 握手
        let (mut client_stream, cert_info) = if let Some(ref acceptor) = tls_acceptor {
            // debug!("[GatewayAuth] Starting TLS handshake with client...");
            match acceptor.accept(client_stream).await {
                Ok(tls_stream) => {
                    // debug!("[GatewayAuth] TLS handshake completed");
                    let cert_info = Self::extract_client_cert_info(&tls_stream)?;
                    info!("[GatewayAuth] Extracted client cert info: {:?}", cert_info);
                    (StreamType::Tls(Box::new(tls_stream)), cert_info)
                }
                Err(e) => {
                    error!("[GatewayAuth] TLS handshake failed: {:?}", e);
                    return Err(format!("TLS handshake failed: {}", e).into());
                }
            }
        } else {
            warn!("[GatewayAuth] Using plain TCP (mTLS disabled)");
            (StreamType::Tcp(client_stream), None)
        };
    
        // 会话 ID
        let session_id = uuid::Uuid::new_v4().to_string();
        info!("[GatewayAuth] Session {} created", session_id);
    
        // 客户端认证
        let permissions = if let Some(cert) = cert_info.as_ref() {
            // debug!("[GatewayAuth] Authenticating client with cert {:?}", cert);
            match Self::authenticate_client(&inner, &session_id, cert).await {
                Ok(perms) => {
                    info!("[GatewayAuth] Auth success for session {}: {:?}", session_id, perms.permissions);
                    perms
                }
                Err(e) => {
                    error!("[GatewayAuth] Auth failed for session {}: {}", session_id, e);
                    return Err(format!("Authentication failed: {}", e).into());
                }
            }
        } else {
            info!("[GatewayAuth] No cert info, allowing anonymous client for session {}", session_id);
            ClientPermissions {
                client_id: "anonymous-client".to_string(),
                permissions: vec!["pub:*".to_string(), "sub:*".to_string(), "admin:*".to_string()],
                session_id: session_id.clone(),
            }
        };
    
        // 缓存权限
        Self::cache_permissions(&inner, &session_id, permissions.clone()).await;
        info!("[GatewayAuth] Session {} permissions cached, authentication completed, starting backend connection", session_id);
    
        // 后端连接
        let mut backend_stream = {
            let cfg = inner.lock().unwrap().config.clone();
            debug!("[GatewayAuth] cfg: {:?}", cfg);
            if cfg.enable_mtls {
                match Self::connect_backend_tls(&inner, backend_addr).await {
                    Ok(tls_stream) => {
                        info!("[GatewayAuth] Backend TLS connection established");
                        StreamType::Tls(tls_stream)
                    }
                    Err(e) => {
                        error!("[GatewayAuth] Backend TLS connection failed: {}", e);
                        return Err(format!("Backend TLS connection failed: {}", e).into());
                    }
                }
            } else {
                info!("[GatewayAuth] Connecting to backend (TCP) {}", backend_addr);
                match TcpStream::connect(backend_addr).await {
                    Ok(stream) => {
                        info!("[GatewayAuth] ✅ Backend TCP connection established");
                        StreamType::Tcp(stream)
                    }
                    Err(e) => {
                        error!("[GatewayAuth] ❌ Backend TCP connection failed: {}", e);
                        return Err(format!("Backend TCP connection failed: {}", e).into());
                    }
                }
            }
        };
    
        // 转发阶段
        info!("[GatewayAuth] Starting proxy_streams_with_auth for session {} with permissions {:?}", session_id, permissions.permissions);
        let proxy_result = Self::proxy_streams_with_auth(inner, &mut client_stream, &mut backend_stream, session_id.clone(), permissions).await;
    
        match proxy_result {
            Ok(_) => {
                info!("[GatewayAuth] ✅ Stream forwarding completed for session {}", session_id);
            }
            Err(e) => {
                error!("[GatewayAuth] ❌ Stream forwarding failed for session {}: {}", session_id, e);
                return Err(format!("Stream forwarding failed: {}", e).into());
            }
        }
    
        info!("[GatewayAuth] Session {} handle_client finished", session_id);
        Ok(())
    }

    /// 加载证书
    fn load_certificates(cert_path: &str) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, Box<dyn std::error::Error + Send + Sync>> {
        let cert_file = std::fs::File::open(cert_path)?;
        let mut cert_reader = std::io::BufReader::new(cert_file);
        let certs = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(certs)
    }

    /// 加载私钥
    fn load_private_key(key_path: &str) -> Result<PrivateKeyDer<'static>, Box<dyn std::error::Error + Send + Sync>> {
        let key_file = std::fs::File::open(key_path)?;
        let mut key_reader = std::io::BufReader::new(key_file);
        let items = rustls_pemfile::read_all(&mut key_reader);
        for item in items {
            match item? {
                rustls_pemfile::Item::Pkcs1Key(key) => return Ok(PrivateKeyDer::Pkcs1(key)),
                rustls_pemfile::Item::Pkcs8Key(key) => return Ok(PrivateKeyDer::Pkcs8(key)),
                rustls_pemfile::Item::Sec1Key(key) => return Ok(PrivateKeyDer::Sec1(key)),
                _ => {}
            }
        }
        Err("No private key found".into())
    }

    /// 连接到后端TLS Router
    async fn connect_backend_tls(
        inner: &Arc<Mutex<RunningPluginInner>>,
        backend_addr: SocketAddr,
    ) -> Result<Box<dyn AsyncStream>, Box<dyn std::error::Error + Send + Sync>> {
        info!("[GatewayAuth] Connecting to backend TLS at {}", backend_addr);
        use rustls::ClientConfig;
        use tokio_rustls::TlsConnector;

        let config = inner.lock().unwrap().config.clone();
        debug!("[GatewayAuth] Loaded plugin config for backend connection");

        // 创建客户端TLS配置
        let mut root_store = rustls::RootCertStore::empty();
        if let Some(ca_path) = &config.ca_certificate {
            let ca_certs = Self::load_certificates(ca_path)?;
            for cert in ca_certs {
                root_store.add(cert)?;
            }
        }

        // 创建客户端配置 - 网关到Router的连接通常不需要客户端证书
        let client_config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(std::sync::Arc::new(client_config));

        // 建立TCP连接
        info!("[GatewayAuth] Establishing TCP connection to backend at {}", backend_addr);
        let tcp_stream = TcpStream::connect(backend_addr).await?;
        info!("[GatewayAuth] TCP connection to backend established successfully");

        // 执行TLS握手
        info!("[GatewayAuth] Starting TLS handshake to backend");
        let domain = rustls::pki_types::ServerName::try_from("localhost")?;
        info!("[GatewayAuth] TLS domain set to localhost, performing handshake");
        let tls_stream = connector.connect(domain, tcp_stream).await?;
        info!("[GatewayAuth] TLS handshake to backend completed successfully");

        Ok(Box::new(tls_stream))
    }

    /// 创建TLS接收器
    fn create_tls_acceptor(
        ca_cert_path: &Option<String>,
        server_cert_path: &Option<String>,
        server_key_path: &Option<String>,
    ) -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
        use rustls::RootCertStore;

        eprintln!("=== TLS ACCEPTOR: Installing crypto provider ===");
        // 安装默认的CryptoProvider (Rust 1.75.0 + rustls 0.23.35)
        rustls::crypto::ring::default_provider().install_default().ok();
        eprintln!("=== TLS ACCEPTOR: Crypto provider installed ===");

        // 加载CA证书
        eprintln!("=== TLS ACCEPTOR: Loading CA certificate ===");
        let mut root_cert_store = RootCertStore::empty();
        if let Some(ca_path) = ca_cert_path {
            eprintln!("=== TLS ACCEPTOR: CA path: {:?} ===", ca_path);
            let ca_file = std::fs::File::open(ca_path)?;
            let mut ca_reader = std::io::BufReader::new(ca_file);
            let certs = rustls_pemfile::certs(&mut ca_reader);
            for cert in certs {
                let cert = cert?;
                root_cert_store.add(cert)?;
            }
            eprintln!("=== TLS ACCEPTOR: CA certificate loaded ===");
        } else {
            eprintln!("=== TLS ACCEPTOR: CA certificate path not provided ===");
            return Err("CA certificate path not provided".into());
        };

        // 加载服务器证书
        eprintln!("=== TLS ACCEPTOR: Loading server certificate ===");
        let server_certs = if let Some(cert_path) = server_cert_path {
            eprintln!("=== TLS ACCEPTOR: Server cert path: {:?} ===", cert_path);
            let cert_file = std::fs::File::open(cert_path)?;
            let mut cert_reader = std::io::BufReader::new(cert_file);
            rustls_pemfile::certs(&mut cert_reader)
                .collect::<Result<Vec<_>, _>>()?
        } else {
            eprintln!("=== TLS ACCEPTOR: Server certificate path not provided ===");
            return Err("Server certificate path not provided".into());
        };
        eprintln!("=== TLS ACCEPTOR: Server certificate loaded ===");

        // 加载服务器私钥
        eprintln!("=== TLS ACCEPTOR: Loading server private key ===");
        let server_key_der = if let Some(key_path) = server_key_path {
            eprintln!("=== TLS ACCEPTOR: Server key path: {:?} ===", key_path);
            Self::load_private_key(key_path)?
        } else {
            eprintln!("=== TLS ACCEPTOR: Server private key path not provided ===");
            return Err("Server private key path not provided".into());
        };
        eprintln!("=== TLS ACCEPTOR: Server private key loaded ===");

        // 创建客户端证书验证器 - 与Zenoh Router完全一致的实现
        let client_verifier = WebPkiClientVerifier::builder(root_cert_store.into())
            .build()?;

        // 创建TLS配置 - 强制使用TLS 1.3，与Zenoh兼容
        let mut server_config = ServerConfig::builder_with_protocol_versions(&[&TLS13])
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(server_certs, server_key_der)?;

        // 设置最基本的兼容配置
        server_config.alpn_protocols = vec![];
        server_config.max_fragment_size = None; // 使用默认
        server_config.ignore_client_order = false; // 使用标准顺序

        // 禁用可能导致兼容性问题的功能
        server_config.max_early_data_size = 0;

        eprintln!("=== TLS ACCEPTOR: Using TLS 1.3 only, matching Zenoh Router configuration ===");

        Ok(TlsAcceptor::from(std::sync::Arc::new(server_config)))
    }

    /// 提取客户端证书信息
    fn extract_client_cert_info(tls_stream: &tokio_rustls::server::TlsStream<TcpStream>) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        // 从TLS连接中提取客户端证书
        if let Some(peer_certs) = tls_stream.get_ref().1.peer_certificates() {
            if let Some(cert_der) = peer_certs.first() {
                // 解析X.509证书
                match X509Certificate::from_der(cert_der.as_ref()) {
                    Ok((_, cert)) => {
                        // 提取Subject信息
                        let subject = cert.subject.to_string();
                        return Ok(Some(subject));
                    }
                    Err(e) => {
                        debug!("[GatewayAuth] Failed to parse certificate: {}", e);
                        return Ok(None);
                    }
                }
            }
        }

        Ok(None)
    }

    /// 双向流转发（带权限检查版本）
    async fn proxy_streams_with_auth(
        inner: Arc<Mutex<RunningPluginInner>>,
        client_stream: &mut StreamType,
        backend_stream: &mut StreamType,
        session_id: String,
        client_permissions: ClientPermissions,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[GatewayAuth] Session {} proxy_streams_with_auth started", session_id);
        let mut client_buf = [0; 4096];
        let mut backend_buf = [0; 4096];
        info!("[GatewayAuth] Session {} entering forwarding loop", session_id);

        loop {
            info!("[GatewayAuth] Session {} waiting for data in select loop", session_id);
            tokio::select! {
                result = client_stream.read(&mut client_buf) => {
                    match result {
                        Ok(0) => {
                            info!("[GatewayAuth] Session {} client stream EOF, exiting", session_id);
                            break;
                        }
                        Ok(n) => {
                            info!("[GatewayAuth] Session {} received {} bytes from client", session_id, n);
                            // 检查客户端到服务端的权限（发布权限）
                            if Self::check_outbound_permission(&inner, &client_permissions, &client_buf[..n]).await {
                                info!("[GatewayAuth] Session {} outbound permission granted, forwarding {} bytes", session_id, n);
                                backend_stream.write_all(&client_buf[..n]).await?;
                                info!("[GatewayAuth] Session {} forwarded {} bytes to backend", session_id, n);
                            } else {
                                info!("[GatewayAuth] Session {} outbound message blocked by permission check", session_id);
                                debug!("[GatewayAuth] Session {} outbound message blocked", session_id);
                                // 可以选择关闭连接或继续
                            }
                        }
                        Err(e) => {
                            error!("[GatewayAuth] Session {} error reading from client: {}", session_id, e);
                            break;
                        }
                    }
                }
                result = backend_stream.read(&mut backend_buf) => {
                    match result {
                        Ok(0) => {
                            info!("[GatewayAuth] Session {} backend stream EOF, exiting", session_id);
                            break;
                        }
                        Ok(n) => {
                            info!("[GatewayAuth] Session {} received {} bytes from backend", session_id, n);
                            // 检查服务端到客户端的权限（订阅权限）
                            if Self::check_inbound_permission(&inner, &client_permissions, &backend_buf[..n]).await {
                                info!("[GatewayAuth] Session {} inbound permission granted, forwarding {} bytes", session_id, n);
                                client_stream.write_all(&backend_buf[..n]).await?;
                                info!("[GatewayAuth] Session {} forwarded {} bytes to client", session_id, n);
                            } else {
                                info!("[GatewayAuth] Session {} inbound message blocked by permission check", session_id);
                                debug!("[GatewayAuth] Session {} inbound message blocked", session_id);
                                // 可以选择关闭连接或继续
                            }
                        }
                        Err(e) => {
                            error!("[GatewayAuth] Session {} error reading from backend: {}", session_id, e);
                            break;
                        }
                    }
                }
            }
        }

        info!("[GatewayAuth] Session {} proxy_streams_with_auth completed normally", session_id);
        Ok(())
    }

    /// 检查出站权限（客户端到服务端 - 发布权限）
    async fn check_outbound_permission(
        inner: &Arc<Mutex<RunningPluginInner>>,
        permissions: &ClientPermissions,
        data: &[u8],
    ) -> bool {
        // 简化实现：检查是否包含敏感主题
        let data_str = String::from_utf8_lossy(data);

        // 如果消息包含敏感主题（如admin），拒绝
        if data_str.contains("admin/") || data_str.contains("system/") {
            let guard = inner.lock().unwrap();
            if guard.config.enable_debug_logging {
                debug!("[GatewayAuth] Outbound blocked: sensitive content detected");
            }
            return false;
        }

        // 其他消息允许通过
        true
    }

    /// 检查入站权限（服务端到客户端 - 订阅权限）
    async fn check_inbound_permission(
        inner: &Arc<Mutex<RunningPluginInner>>,
        permissions: &ClientPermissions,
        data: &[u8],
    ) -> bool {
        // 简化实现：检查是否包含敏感主题
        let data_str = String::from_utf8_lossy(data);

        // 如果消息包含敏感主题（如admin），拒绝
        if data_str.contains("admin/") || data_str.contains("system/") {
            let guard = inner.lock().unwrap();
            if guard.config.enable_debug_logging {
                debug!("[GatewayAuth] Inbound blocked: sensitive content detected");
            }
            return false;
        }

        // 其他消息允许通过
        true
    }

    /// 从Zenoh消息中提取key expression
    fn extract_key_expr_from_message(data: &[u8]) -> Option<String> {
        // 简化实现：这是一个非常基础的解析
        // 实际的Zenoh协议解析会更复杂

        if data.len() < 4 {
            return None;
        }

        // Zenoh消息通常以消息头开始
        // 这里做一个简单的启发式解析，寻找可能的key expression

        // 转换为字符串尝试查找
        if let Ok(text) = std::str::from_utf8(data) {
            // 查找常见的key expression模式
            if let Some(start) = text.find('/') {
                // 尝试提取从第一个'/'到消息结束或空格的部分
                let potential_key = &text[start..];
                let key_end = potential_key.find(' ').unwrap_or(potential_key.len());
                let key_expr = &potential_key[..key_end];

                // 验证是否看起来像key expression
                if key_expr.contains('/') && !key_expr.contains('\n') {
                    return Some(key_expr.to_string());
                }
            }
        }

        None
    }

    /// 检查key expression是否匹配权限模式
    fn matches_key_pattern(pattern: &str, key_expr: &str) -> bool {
        // 支持通配符匹配
        // 例如: "sensor/**" 匹配 "sensor/temperature", "sensor/humidity"等

        if pattern == key_expr {
            return true;
        }

        // 处理通配符
        if pattern.ends_with("/**") {
            let prefix = &pattern[..pattern.len() - 3]; // 移除"/**"
            return key_expr.starts_with(prefix) && (key_expr.len() == prefix.len() || key_expr[prefix.len()..].starts_with('/'));
        }

        // 处理单层通配符
        if pattern.contains("/*") {
            let pattern_parts: Vec<&str> = pattern.split('/').collect();
            let key_parts: Vec<&str> = key_expr.split('/').collect();

            if pattern_parts.len() != key_parts.len() {
                return false;
            }

            for (p, k) in pattern_parts.iter().zip(key_parts.iter()) {
                if *p != "*" && *p != *k {
                    return false;
                }
            }

            return true;
        }

        false
    }

    /// 双向流转发（简化版本）
    async fn proxy_streams_simple(
        mut client_stream: TcpStream,
        mut backend_stream: TcpStream,
        session_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut client_buf = [0; 4096];
        let mut backend_buf = [0; 4096];

        loop {
            tokio::select! {
                result = client_stream.read(&mut client_buf) => {
                    match result {
                        Ok(0) => break,
                        Ok(n) => {
                            backend_stream.write_all(&client_buf[..n]).await?;
                        }
                        Err(_) => break,
                    }
                }
                result = backend_stream.read(&mut backend_buf) => {
                    match result {
                        Ok(0) => break,
                        Ok(n) => {
                            client_stream.write_all(&backend_buf[..n]).await?;
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        debug!("[GatewayAuth] Session {} connection closed", session_id);
        Ok(())
    }

    /// 从证书信息中提取客户端ID
    fn extract_client_id_from_cert(cert_info: &str) -> String {
        // 简化实现：假设证书信息格式为 "CN=client-id,O=org"
        if let Some(cn_part) = cert_info.split(',').find(|s| s.starts_with("CN=")) {
            if let Some(client_id) = cn_part.strip_prefix("CN=") {
                return client_id.to_string();
            }
        }

        // 如果找不到CN，尝试直接使用cert_info作为client_id
        cert_info.to_string()
    }

    /// 根据客户端ID推断客户端类型
    fn infer_client_type(client_id: &str) -> String {
        if client_id.contains("sensor") {
            "sensor".to_string()
        } else if client_id.contains("control") || client_id.contains("controller") {
            "controller".to_string()
        } else if client_id.contains("gateway") {
            "gateway".to_string()
        } else if client_id.contains("monitor") {
            "monitor".to_string()
        } else {
            "unknown".to_string()
        }
    }

    /// 调用外部鉴权服务
    async fn call_auth_service(
        inner: &Arc<Mutex<RunningPluginInner>>,
        client_id: &str,
        client_type: &str,
    ) -> Result<AuthResponse, Box<dyn std::error::Error + Send + Sync>> {
        // 在await之前获取需要的值
        let (url, http_client, enable_debug) = {
            let guard = inner.lock().unwrap();
            
            // 更新统计信息
            {
                let mut stats = guard.stats.lock().unwrap();
                stats.http_requests += 1;
            }
            
            let url = format!("{}/auth", guard.config.auth_service_url);
            let http_client = guard.http_client.clone();
            let enable_debug = guard.config.enable_debug_logging;
            (url, http_client, enable_debug)
        };

        let request_data = serde_json::json!({
            "client_id": client_id,
            "client_type": client_type
        });

        // 发送HTTP请求
        let response = http_client
            .post(&url)
            .json(&request_data)
            .send()
            .await
            .map_err(|e| {
                let guard = inner.lock().unwrap();
                let mut stats = guard.stats.lock().unwrap();
                stats.http_errors += 1;
                e
            })?;

        if !response.status().is_success() {
            let guard = inner.lock().unwrap();
            let mut stats = guard.stats.lock().unwrap();
            stats.http_errors += 1;
            return Err(format!("Auth service returned status: {}", response.status()).into());
        }

        let auth_response: AuthResponse = response.json().await
            .map_err(|e| {
                let guard = inner.lock().unwrap();
                let mut stats = guard.stats.lock().unwrap();
                stats.http_errors += 1;
                e
            })?;

        if enable_debug {
            debug!("[GatewayAuth] Auth service response: {:?}", auth_response);
        }

        Ok(auth_response)
    }

    /// 认证客户端
    async fn authenticate_client(
        inner: &Arc<Mutex<RunningPluginInner>>,
        session_id: &str,
        cert_info: &str,
    ) -> Result<ClientPermissions, Box<dyn std::error::Error + Send + Sync>> {
        // 更新统计信息
        {
            let guard = inner.lock().unwrap();
            let mut stats = guard.stats.lock().unwrap();
            stats.total_auth_requests += 1;
        }

        // 解析证书信息
        let client_id = Self::extract_client_id_from_cert(cert_info);
        let client_type = Self::infer_client_type(&client_id);

        // 检查是否启用调试日志
        let enable_debug = {
            let guard = inner.lock().unwrap();
            guard.config.enable_debug_logging
        };

        if enable_debug {
            debug!("[GatewayAuth] Calling auth service for client_id: {}, client_type: {}", client_id, client_type);
        }
        
        // 调用外部认证服务
        let auth_response = match Self::call_auth_service(inner, &client_id, &client_type).await {
            Ok(response) => {
                response
            }
            Err(e) => {
                error!("[GatewayAuth] Auth service call failed: {}", e);
                return Err(format!("Auth service error: {}", e).into());
            }
        };

        if auth_response.is_authorized {
            Ok(ClientPermissions {
                client_id,
                permissions: auth_response.permissions,
                session_id: session_id.to_string(),
            })
        } else {
            Err(format!("Authentication failed: {:?}", auth_response.error_message).into())
        }
    }

    /// 缓存客户端权限
    async fn cache_permissions(
        inner: &Arc<Mutex<RunningPluginInner>>,
        session_id: &str,
        permissions: ClientPermissions,
    ) {
        // 短暂锁住 inner，读取 config 标志、ttl 和 clone 出 Arc<Mutex<...>>
        let (enable_debug, cache_arc, cache_ttl_secs) = {
            let guard = inner.lock().unwrap();
            (
                guard.config.enable_debug_logging,
                guard.permission_cache.clone(),
                guard.config.cache_ttl_seconds,
            )
        }; // guard 在这里被释放
    
        // 现在只锁 permission_cache，先检查是否已有有效缓存
        {
            let cache = cache_arc.lock().unwrap();
            if let Some(entry) = cache.get(session_id) {
                let age = entry.created_at.elapsed();
                if age < Duration::from_secs(cache_ttl_secs) {
                    if enable_debug {
                        debug!("[GatewayAuth] Permissions already cached and valid for session: {}", session_id);
                    }
                    return;
                }
            }
        } // 释放读锁（这里是同一个 Mutex 的短期锁）
    
        // 插入或覆盖缓存（再次锁住进行写入）
        let mut cache = cache_arc.lock().unwrap();
        let cache_entry = PermissionCacheEntry {
            permissions,
            created_at: Instant::now(),
        };
        cache.insert(session_id.to_string(), cache_entry);
        drop(cache);
    
        if enable_debug {
            debug!("[GatewayAuth] Cached permissions for session: {}", session_id);
        }
    }
    
    
    /// 检查权限
    async fn check_permission(
        inner: &Arc<Mutex<RunningPluginInner>>,
        session_id: &str,
        action: &str,
        resource: &str,
    ) -> bool {
        let guard = inner.lock().unwrap();

        // 检查缓存
        if let Some(cache_entry) = guard.permission_cache.lock().unwrap().get(session_id) {
            let required_permission = format!("{}:{}", action, resource);
            let has_permission = cache_entry.permissions.permissions.iter().any(|p| {
                Self::matches_permission(p, &required_permission)
            });

            // 更新统计信息
            {
                let guard = inner.lock().unwrap();
                let mut stats = guard.stats.lock().unwrap();
                if has_permission {
                    stats.cache_hits += 1;
                    stats.messages_allowed += 1;
                } else {
                    stats.cache_hits += 1; // 仍然算缓存命中，只是权限不匹配
                    stats.messages_denied += 1;
                }
            }

            return has_permission;
        }

        // 缓存未命中
        {
            let guard = inner.lock().unwrap();
            let mut stats = guard.stats.lock().unwrap();
            stats.cache_misses += 1;
            stats.messages_denied += 1;
        }

        false
    }

    /// 检查权限匹配 (支持通配符)
    fn matches_permission(pattern: &str, permission: &str) -> bool {
        if pattern.contains('*') {
            // 简单通配符匹配
            let pattern_parts: Vec<&str> = pattern.split('*').collect();
            if pattern_parts.len() == 1 {
                return pattern == permission;
            }

            let mut remaining = permission;
            for (i, part) in pattern_parts.iter().enumerate() {
                if i == 0 {
                    if !remaining.starts_with(part) {
                        return false;
                    }
                    remaining = &remaining[part.len()..];
                } else if i == pattern_parts.len() - 1 {
                    if !remaining.ends_with(part) {
                        return false;
                    }
                } else {
                    if let Some(pos) = remaining.find(part) {
                        remaining = &remaining[pos + part.len()..];
                    } else {
                        return false;
                    }
                }
            }
            true
        } else {
            pattern == permission
        }
    }

    /// 清理过期缓存
    async fn cleanup_expired_cache(inner: &Arc<Mutex<RunningPluginInner>>) {
        let guard = inner.lock().unwrap();
        let ttl = guard.config.cache_ttl_seconds;
        let mut cache = guard.permission_cache.lock().unwrap();

        let expired_sessions: Vec<String> = cache.iter()
            .filter(|(_, entry)| entry.created_at.elapsed().as_secs() > ttl)
            .map(|(session_id, _)| session_id.clone())
            .collect();

        for session_id in expired_sessions {
            cache.remove(&session_id);
            // debug!("[GatewayAuth] Cleaned up expired session: {}", session_id);
        }
    }
}

