use crate::sources::postgres::errors::PostgresSourceError;
use crate::utils::remove_query_params;
#[cfg(feature = "postgres-openssl")]
use openssl::ssl::{SslConnector, SslFiletype, SslMethod, SslVerifyMode};
use postgres::{config::SslMode, Config};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use url::Url;

#[derive(Clone, Debug)]
pub struct TlsConfig {
    /// Postgres config, pg_config.sslmode (`sslmode`).
    pub pg_config: Config,
    /// Location of the client cert and key (`sslcert`, `sslkey`).
    pub client_cert: Option<(PathBuf, PathBuf)>,
    /// Location of the root certificate (`sslrootcert`).
    pub root_cert: Option<PathBuf>,
}

/// TLS connector of the Postgres source: OpenSSL with `src_postgres`, rustls with `src_postgres_rustls` only.
#[cfg(feature = "postgres-openssl")]
pub use postgres_openssl::MakeTlsConnector;
#[cfg(not(feature = "postgres-openssl"))]
pub use tokio_postgres_rustls::MakeRustlsConnect as MakeTlsConnector;

#[cfg(not(any(feature = "postgres-openssl", feature = "tokio-postgres-rustls")))]
compile_error!("the Postgres source needs a TLS backend: enable `src_postgres` or `src_postgres_rustls`");

/// Whether the server certificate is checked against the root certificate, and its name against the host.
fn verification(tls_config: &TlsConfig) -> (bool, bool) {
    let ssl_mode = tls_config.pg_config.get_ssl_mode();
    match ssl_mode {
        SslMode::Disable | SslMode::Prefer => (false, false),
        SslMode::Require => match tls_config.root_cert {
            // If a root CA file exists, the behavior of sslmode=require will be the same as
            // that of verify-ca, meaning the server certificate is validated against the CA.
            //
            // For more details, check out the note about backwards compatibility in
            // https://postgresql.org/docs/current/libpq-ssl.html#LIBQ-SSL-CERTIFICATES.
            Some(_) => (true, false),
            None => (false, false),
        },
        // These two modes will not work until upstream rust-postgres supports parsing
        // them as part of the TLS config.
        //
        // SslMode::VerifyCa => (true, false),
        // SslMode::VerifyFull => (true, true),
        _ => panic!("unexpected sslmode {:?}", ssl_mode),
    }
}

#[cfg(feature = "postgres-openssl")]
impl TryFrom<TlsConfig> for MakeTlsConnector {
    type Error = PostgresSourceError;
    // The logic of this function adapted primarily from:
    // https://github.com/sfackler/rust-postgres/pull/774
    // We only support server side authentication (`sslrootcert`) for now
    fn try_from(tls_config: TlsConfig) -> Result<Self, Self::Error> {
        let mut builder = SslConnector::builder(SslMethod::tls_client())?;
        let (verify_ca, verify_hostname) = verification(&tls_config);

        if let Some((cert, key)) = tls_config.client_cert {
            builder.set_certificate_file(cert, SslFiletype::PEM)?;
            builder.set_private_key_file(key, SslFiletype::PEM)?;
        }

        if let Some(root_cert) = tls_config.root_cert {
            builder.set_ca_file(root_cert)?;
        }

        if !verify_ca {
            builder.set_verify(SslVerifyMode::NONE); // do not verify CA
        }

        let mut tls_connector = MakeTlsConnector::new(builder.build());

        if !verify_hostname {
            tls_connector.set_callback(|connect, _| {
                connect.set_verify_hostname(false);
                Ok(())
            });
        }

        Ok(tls_connector)
    }
}

#[cfg(not(feature = "postgres-openssl"))]
mod rustls_tls {
    use super::{verification, MakeTlsConnector, TlsConfig};
    use crate::sources::postgres::errors::PostgresSourceError;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::client::WebPkiServerVerifier;
    use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
    use rustls::{
        CertificateError, ClientConfig, DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
    };
    use std::convert::TryFrom;
    use std::sync::Arc;

    /// The process-wide provider when one is installed, aws-lc-rs otherwise.
    fn provider() -> Arc<CryptoProvider> {
        CryptoProvider::get_default()
            .cloned()
            .unwrap_or_else(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
    }

    /// Same checks as the OpenSSL connector: none (`roots` is `None`, like `SslVerifyMode::NONE`), or the
    /// certificate chain against the root certificate without the host name. Handshake signatures are always
    /// verified.
    #[derive(Debug)]
    struct Verifier {
        roots: Option<Arc<WebPkiServerVerifier>>,
        provider: Arc<CryptoProvider>,
    }

    impl ServerCertVerifier for Verifier {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            intermediates: &[CertificateDer<'_>],
            server_name: &ServerName<'_>,
            ocsp_response: &[u8],
            now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            let Some(roots) = &self.roots else {
                return Ok(ServerCertVerified::assertion());
            };
            // The chain is checked before the name: a name error means the chain is valid.
            match roots.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now) {
                Err(Error::InvalidCertificate(
                    CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
                )) => Ok(ServerCertVerified::assertion()),
                other => other,
            }
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.provider.signature_verification_algorithms.supported_schemes()
        }
    }

    impl TryFrom<TlsConfig> for MakeTlsConnector {
        type Error = PostgresSourceError;

        // Same behavior as the OpenSSL connector, except that a root certificate (`sslrootcert`) is the only trust
        // anchor (OpenSSL also trusts the system store), and that rustls only speaks TLS 1.2 and 1.3.
        fn try_from(tls_config: TlsConfig) -> Result<Self, Self::Error> {
            let provider = provider();
            let (verify_ca, _verify_hostname) = verification(&tls_config);
            let roots = match (&tls_config.root_cert, verify_ca) {
                (Some(root_cert), true) => {
                    let mut store = RootCertStore::empty();
                    for cert in CertificateDer::pem_file_iter(root_cert)? {
                        store.add(cert?)?;
                    }
                    Some(
                        WebPkiServerVerifier::builder_with_provider(Arc::new(store), provider.clone())
                            .build()
                            .map_err(|e| anyhow::anyhow!("sslrootcert: {}", e))?,
                    )
                }
                _ => None,
            };
            let builder = ClientConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(Verifier { roots, provider }));
            let config = match &tls_config.client_cert {
                Some((cert, key)) => {
                    let chain = CertificateDer::pem_file_iter(cert)?.collect::<Result<Vec<_>, _>>()?;
                    builder.with_client_auth_cert(chain, PrivateKeyDer::from_pem_file(key)?)?
                }
                None => builder.with_no_client_auth(),
            };
            Ok(MakeTlsConnector::new(config))
        }
    }
}

// Strip URL params not accepted by upstream rust-postgres
fn strip_bad_opts(url: &Url) -> Url {
    remove_query_params(url, &["sslkey", "sslcert", "sslrootcert"])
}

pub fn rewrite_tls_args(
    conn: &Url,
) -> Result<(Config, Option<MakeTlsConnector>), PostgresSourceError> {
    // We parse the config, then strip unsupported SSL opts and rewrite the URI
    // before calling conn.parse().
    //
    // For more details on this approach, see the conversation here:
    // https://github.com/sfackler/rust-postgres/pull/774#discussion_r641784774

    let params: HashMap<String, String> = conn.query_pairs().into_owned().collect();

    let sslcert = params.get("sslcert").map(PathBuf::from);
    let sslkey = params.get("sslkey").map(PathBuf::from);
    let root_cert = params.get("sslrootcert").map(PathBuf::from);
    let client_cert = match (sslcert, sslkey) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    };

    let stripped_url = strip_bad_opts(conn);
    let pg_config: Config = stripped_url.as_str().parse()?;

    let tls_config = TlsConfig {
        pg_config: pg_config.clone(),
        client_cert,
        root_cert,
    };

    let tls_connector = match pg_config.get_ssl_mode() {
        SslMode::Disable => None,
        _ => Some(MakeTlsConnector::try_from(tls_config)?),
    };

    Ok((pg_config, tls_connector))
}
