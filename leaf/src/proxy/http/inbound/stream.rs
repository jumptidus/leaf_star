use crate::{
    proxy::{AnyInboundTransport, AnyStream, InboundStreamHandler, InboundTransport},
    session::{Session, SocksAddr},
};

use async_trait::async_trait;
use futures::{future, Future};
use hyper::{server::conn::Http, service::Service, Body, Request, Response};
use log::*;
use std::{io, net::IpAddr, pin::Pin, task::Poll};

pub struct Handler;

// http proxy stream
#[async_trait]
impl InboundStreamHandler for Handler {
    async fn handle<'a>(
        &'a self,
        mut sess: Session,
        mut stream: AnyStream,
    ) -> std::io::Result<AnyInboundTransport> {
        let http = Http::new();

        let proxy_service = ProxyService::new();
        let conn = http
            .serve_connection(stream, proxy_service)
            .without_shutdown();

        let parts = match conn.await {
            Ok(v) => v,
            Err(err) => {
                debug!("accept conn failed: {}", err);
                return Err(io::Error::new(io::ErrorKind::Other, "unspecified"));
            }
        };

        let uri = parts.service.get_uri();
        let host_part: Vec<&str> = uri.split(":").collect();
        if host_part.len() != 2 {
            debug!("invalid uri: {}", uri);
            return Err(io::Error::new(io::ErrorKind::Other, "unspecified"));
        }

        let destination = if let Ok(port) = host_part[1].parse::<u16>() {
            if let Ok(ip) = host_part[0].parse::<IpAddr>() {
                SocksAddr::from((ip, port))
            } else {
                match SocksAddr::try_from((host_part[0], port)) {
                    Ok(v) => v,
                    Err(err) => {
                        debug!("invalid target {:?}: {}", uri, err);
                        return Err(io::Error::new(io::ErrorKind::Other, "unspecified"));
                    }
                }
            }
        } else {
            debug!("invalid target {:?}", uri);
            return Err(io::Error::new(io::ErrorKind::Other, "unspecified"));
        };

        sess.destination = destination;

        Ok(InboundTransport::Stream(parts.io, sess))
    }
}

struct ProxyService {
    uri: String,
}

impl ProxyService {
    pub fn new() -> Self {
        ProxyService {
            uri: "".to_string(),
        }
    }

    pub fn get_uri(&self) -> &String {
        &self.uri
    }
}

#[allow(clippy::type_complexity)]
impl Service<Request<Body>> for ProxyService {
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Response = Response<Body>;

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        self.uri = req.uri().to_string();
        Box::pin(future::ready(Ok(Response::builder()
            .status(200)
            .body(hyper::Body::empty())
            .unwrap())))
    }

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
