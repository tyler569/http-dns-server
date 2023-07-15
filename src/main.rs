use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt::Debug;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use domain::base::Rtype;
use domain::rdata::A;
use hyper::{Body, Method, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use once_cell::sync::Lazy;
use serde::Deserialize;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;

static CACHE: Lazy<RwLock<BTreeMap<String, IpAddr>>> = Lazy::new(|| {
    RwLock::new(BTreeMap::new())
});

#[tokio::main]
async fn main() {
    let j0 = tokio::spawn(serve_dns());
    let j1 = tokio::spawn(serve_http());

    tokio::try_join!(j0, j1).unwrap();
}

async fn cache_get(name: &str) -> Option<IpAddr> {
    let cache = CACHE.read().await;
    cache.get(name).copied()
}

async fn cache_set(name: String, addr: IpAddr) {
    let mut cache = CACHE.write().await;
    cache.insert(name, addr);
}

#[derive(Debug, Clone, Deserialize)]
struct DomainNameUpdate {
    name: String,
    address: String,
}

async fn http_set_value(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    if req.method() == Method::POST {
        let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap();
        let update: DomainNameUpdate = serde_json::from_slice(&body_bytes).unwrap();
        let addr: IpAddr = IpAddr::from_str(&update.address).unwrap();
        cache_set(update.name, addr).await;
    }

    eprintln!("{:?}", CACHE.read().await);
    Ok(Response::new(Body::from(r#"{"status": "ok"}"#)))
}

async fn serve_http() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 1080));

    let service = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(http_set_value))
    });

    let server = Server::bind(&addr).serve(service);

    if let Err(err) = server.await {
        eprintln!("HTTP server error: {}", err);
    }
}

async fn serve_dns() {
    let listener = Arc::new(UdpSocket::bind("127.0.0.1:1053").await.expect("Failed to bind to DNS socket"));
    loop {
        let mut buf = [0u8; 512];
        let (size, addr) = listener.recv_from(&mut buf).await.unwrap();
        let sock = listener.clone();


        tokio::spawn(async move {
            let pkt = &buf[0..size];

            respond_dns(sock, addr, pkt).await.unwrap_or_else(|err| {
                eprintln!("Failed to send: {}", err)
            })
        });
    }
}

async fn respond_dns(sock: Arc<UdpSocket>, addr: SocketAddr, buf: &[u8]) -> io::Result<()> {
    let message = domain::base::Message::from_slice(buf).unwrap();

    let qname = message.question().next().unwrap().unwrap();
    let name = qname.qname();

    let sname = format!("{}", name);
    let saddr = cache_get(&sname).await;
    eprintln!("{:?}, {:?}", sname, saddr);
    let daddr = match saddr {
        Some(IpAddr::V4(v4)) => Some(A::from(v4)),
        _ => None,
    };

    let mut resp = domain::base::MessageBuilder::new_bytes();
    resp.header_mut().set_qr(true);
    resp.header_mut().set_id(message.header().id());
    let mut resp = resp.question();
    resp.push((name, Rtype::A)).unwrap();
    let mut resp = resp.answer();
    if let Some(daddr) = daddr {
        resp.push((name, 300, daddr)).unwrap();
    }
    let resp = resp.finish();

    sock.send_to(resp.as_ref(), addr).await?;
    Ok(())
}