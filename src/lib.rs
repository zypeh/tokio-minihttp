extern crate tokio_proto;
extern crate tokio_core;
extern crate tokio_tls;
extern crate futures;
extern crate bytes;
extern crate time;
extern crate httparse;

mod date;
mod request;
mod response;
mod ssl;

pub use request::Request;
pub use response::Response;
pub use ssl::NewSslContext;

use tokio_proto::{server, Service, NewService};
use tokio_proto::io::Framed;
use tokio_proto::proto::pipeline;

use tokio_core::Receiver;
use tokio_core::Loop;

use futures::{Future, Map};
use bytes::BlockBuf;
use std::io;
use std::net::SocketAddr;

pub struct Server {
    addr: SocketAddr,
    ssl: Option<Box<NewSslContext>>,
}

impl Server {
    pub fn new(addr: SocketAddr) -> Self {
        Server {
            addr: addr,
            ssl: None,
        }
    }

    pub fn addr(mut self, addr: SocketAddr) -> Self {
        self.addr = addr;
        self
    }

    pub fn ssl<T: NewSslContext>(mut self, ssl: T) -> Self {
        self.ssl = Some(Box::new(ssl));
        self
    }

    pub fn serve<T>(self, new_service: T)
        where T: NewService<Req = Request, Resp = Response, Error = io::Error> + Send + 'static
    {
        let reactor = Loop::new().unwrap();
        let handle = reactor.handle();
        let addr = self.addr;
        let ssl = self.ssl;

        server::listen(&handle, addr, move |socket| {
            // Create the service
            let service = try!(new_service.new_service());
            let service = HttpService { inner: service };

            let mut socket = ssl::MaybeSsl::new(socket);

            if let Some(ref new_context) = ssl {
                socket.establish(try!(new_context.new_context()));
            }

            // Create the transport
            let transport =
                Framed::new(socket,
                            request::Parser,
                            response::Serializer,
                            BlockBuf::default(),
                            BlockBuf::default());

            // Return the pipeline server task
            pipeline::Server::new(service, transport)
        }).unwrap();

        reactor.run().unwrap();
    }
}

impl Default for Server {
    fn default() -> Server {
        Server {
            addr: "0.0.0.0:3000".parse().unwrap(),
            ssl: None,
        }
    }
}

struct HttpService<T> {
    inner: T,
}

impl<T> Service for HttpService<T>
    where T: std::marker::Send, {
    type Req = Request;
    type Resp = pipeline::Message<Response, Receiver<()>>;
    type Error = io::Error;
    type Fut = Box<Future<Item = Self::Resp, Error = io::Error>>;

    fn call(&self, req: Request) -> Self::Fut {
        self.inner.call(req).map(pipeline::Message::WithoutBody)
    }
}

pub fn serve<T>(addr: SocketAddr, new_service: T)
    where T: NewService<Req = Request, Resp = Response, Error = io::Error> + Send + 'static
{
    Server::default()
        .addr(addr)
        .serve(new_service)
}
