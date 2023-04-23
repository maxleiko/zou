use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context};
use axum::body::{boxed, Body, BoxBody};
use axum::extract::{Host, State};
use axum::handler::HandlerWithoutStateExt;
use axum::http::{HeaderValue, Request};
use axum::http::{Response, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect};
use axum::{middleware, BoxError, Router};
use axum_server::tls_rustls::RustlsConfig;
use clap::{Parser, ValueEnum};
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let args = Args::parse();

    let config = match (&args.tls_cert, &args.tls_key) {
        (Some(cert), Some(key)) => {
            // configure certificate and private key used by https
            let config = RustlsConfig::from_pem_file(cert, key).await.unwrap();

            Config {
                http: 80,
                https: Some((args.port, config)),
            }
        }
        _ => Config {
            http: args.port,
            https: None,
        },
    };

    let mode = ServeMode::try_from(args)?;

    let app = Router::new()
        .fallback(axum::routing::get(get_static_file))
        .layer(middleware::from_fn(most_important_middleware))
        .with_state(mode.clone());

    match config.https {
        Some((https_port, tls_config)) => {
            // add a redirect from "config.http" to "config.https"
            tokio::spawn(redirect_http_to_https(config.http, https_port));

            let addr = SocketAddr::from(([0, 0, 0, 0], https_port));

            tracing::info!("{mode}");
            tracing::info!("listening on {addr}");

            axum_server::bind_rustls(addr, tls_config)
                .serve(app.into_make_service())
                .await
                .unwrap();
        }
        None => {
            let addr = SocketAddr::from(([0, 0, 0, 0], config.http));

            tracing::info!("{mode}");
            tracing::info!("listening on {addr}");

            axum_server::bind(addr)
                .serve(app.into_make_service())
                .await
                .unwrap();
        }
    }

    Ok(())
}

async fn most_important_middleware<B>(request: Request<B>, next: Next<B>) -> impl IntoResponse {
    let mut response = next.run(request).await;
    response.headers_mut().append(
        "x-braindead",
        HeaderValue::from_static("never gonna give you up"),
    );
    response
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "zoubida=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[derive(Clone)]
enum ServeMode {
    Path(PathBuf),
    Subdomain(PathBuf),
}

impl TryFrom<Args> for ServeMode {
    type Error = anyhow::Error;

    fn try_from(value: Args) -> Result<Self, Self::Error> {
        let dir = value
            .dir
            .unwrap_or(std::env::current_dir().context("unable to to read current directory")?);

        if !dir.is_dir() {
            bail!("unable to find directory {:?}", dir);
        }

        let mode = match &value.mode {
            Mode::Path => Self::Path(dir),
            Mode::Subdomain => Self::Subdomain(dir),
        };

        Ok(mode)
    }
}

impl std::fmt::Display for ServeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeMode::Path(m) => write!(f, "serving directory {m:?} in mode PATH",),
            ServeMode::Subdomain(m) => write!(f, "serving directory {m:?} in mode SUBDOMAIN",),
        }
    }
}

async fn get_static_file(
    Host(host): Host,
    uri: Uri,
    State(mode): State<ServeMode>,
) -> Result<Response<BoxBody>, (StatusCode, String)> {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();

    let dir = match mode {
        ServeMode::Path(root_dir) => root_dir,
        ServeMode::Subdomain(mut root_dir) => {
            match subdomain(&host) {
                Some(subdomain) => {
                    root_dir.push(subdomain);
                    root_dir
                }
                None => return Err((StatusCode::BAD_REQUEST, "Oops 2!".to_string())),
            }
        }
    };

    tracing::trace!("servedir={dir:?}");

    match ServeDir::new(dir)
        .append_index_html_on_directories(true)
        .try_call(req)
        .await
    {
        Ok(res) => Ok(res.map(boxed)),
        Err(_) => Err((StatusCode::BAD_REQUEST, "Oops!".to_string())),
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value = "4242")]
    port: u16,

    #[clap(
        index = 1,
        help = "Directory to server files from, uses current dir by default"
    )]
    dir: Option<PathBuf>,

    #[clap(short, long, help = "Serving mode", default_value = "path", value_enum)]
    mode: Mode,

    #[clap(long, help = "TLS certificate to use")]
    tls_cert: Option<PathBuf>,

    #[clap(long, help = "TLS private key to use")]
    tls_key: Option<PathBuf>,
}

#[derive(ValueEnum, Clone, Debug)]
enum Mode {
    Path,
    Subdomain,
}

fn subdomain(host: &str) -> Option<&str> {
    host.rsplitn(3, '.').skip(2).next()
}

#[test]
fn test_subdomains() {
    assert_eq!(Some("leiko"), subdomain("leiko.braindead.fr"));
    assert_eq!(Some("foo.bar"), subdomain("foo.bar.braindead.fr"));
    assert_eq!(Some("foo.bar-baz"), subdomain("foo.bar-baz.braindead.fr"));
    assert_eq!(None, subdomain("braindead.fr"));
}

struct Config {
    http: u16,
    https: Option<(u16, RustlsConfig)>,
}

async fn redirect_http_to_https(http_port: u16, https_port: u16) {
    fn make_https(host: String, uri: Uri, from: u16, to: u16) -> Result<Uri, BoxError> {
        let mut parts = uri.into_parts();

        parts.scheme = Some(axum::http::uri::Scheme::HTTPS);

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some("/".parse().unwrap());
        }

        let https_host = host.replace(&from.to_string(), &to.to_string());
        parts.authority = Some(https_host.parse()?);

        Ok(Uri::from_parts(parts)?)
    }

    let redirect = move |Host(host): Host, uri: Uri| async move {
        match make_https(host, uri, http_port, https_port) {
            Ok(uri) => Ok(Redirect::permanent(&uri.to_string())),
            Err(error) => {
                tracing::warn!(%error, "failed to convert URI to HTTPS");
                Err(StatusCode::BAD_REQUEST)
            }
        }
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
    tracing::info!("redirect :{http_port} to :{https_port}",);
    axum::Server::bind(&addr)
        .serve(redirect.into_make_service())
        .await
        .unwrap();
}
