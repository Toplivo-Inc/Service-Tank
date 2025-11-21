use async_trait::async_trait;
use pingora::prelude::*;
use std::sync::Arc;
use clap::Parser;

#[derive(Parser, Debug)]
struct Opt {
    /// Upstream backends
    #[arg(short, long, default_values_t = [
        "1.1.1.1:443".to_string(),
        "1.0.0.1:443".to_string(),
    ])]
    upstreams: Vec<String>,

    /// Addresses for listeners: http & https
    #[arg(short, long, default_values_t = [
        "0.0.0.0:6188".to_string(),
        "0.0.0.0:6189".to_string(),
    ])]
    listeners: Vec<String>,

    /// Path to TLS certificate
    #[arg(short, long, default_value_os_t = format!("{}/tests/keys/server.crt", env!("CARGO_MANIFEST_DIR")))]
    cert: String,

    /// Path to TLS private key
    #[arg(short, long, default_value_os_t = format!("{}/tests/keys/key.pem", env!("CARGO_MANIFEST_DIR")))]
    key: String,
}

pub struct LB(Arc<LoadBalancer<RoundRobin>>);

#[async_trait]
impl ProxyHttp for LB {
    type CTX = ();

    fn new_ctx(&self) -> () {}

    async fn upstream_peer(&self, _session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        let upstream = self.0.select(b"", 256).unwrap();

        println!("upstream peer is: {upstream:?}");

        let peer = Box::new(HttpPeer::new(upstream, true, "one.one.one.one".to_string()));
        Ok(peer)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        upstream_request.insert_header("Host", "one.one.one.one")?;
        Ok(())
    }
}

fn main() {
    let opt = Opt::parse();

    let tcp_http_list = &opt.listeners[0];
    let tcp_https_list = &opt.listeners[1];

    let cert_path = &opt.cert;
    let key_path = &opt.key;

    let mut my_server = Server::new(None).unwrap();
    my_server.bootstrap();

    let mut upstreams =
        LoadBalancer::try_from_iter(opt.upstreams).unwrap();

    let hc = TcpHealthCheck::new();
    upstreams.set_health_check(hc);
    upstreams.health_check_frequency = Some(std::time::Duration::from_secs(1));

    let background = background_service("health check", upstreams);

    let upstreams = background.task();

    let mut lb = http_proxy_service(&my_server.configuration, LB(upstreams));
    lb.add_tcp(tcp_http_list);

    // let cert_path = format!("{}/tests/keys/server.crt", env!("CARGO_MANIFEST_DIR"));
    // let key_path = format!("{}/tests/keys/key.pem", env!("CARGO_MANIFEST_DIR"));

    let mut tls_settings =
        pingora_core::listeners::tls::TlsSettings::intermediate(&cert_path, &key_path).unwrap();
    tls_settings.enable_h2();
    lb.add_tls_with_settings(tcp_https_list, None, tls_settings);

    my_server.add_service(lb);
    my_server.add_service(background);
    my_server.run_forever();
}
