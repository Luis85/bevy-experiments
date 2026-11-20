//! Explicit server identity and trusted-CA TLS, using rustls verification.
//! This protects bearer capabilities in transit; it is not account enrollment.
use crate::{NetError, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject};
use std::{fs, io::Read, path::Path, sync::Arc};

const MAX_PEM_BYTES: u64 = 1024 * 1024;
pub(crate) fn alpn() -> Vec<u8> {
    format!("terra-world/{}", crate::PROTOCOL_VERSION).into_bytes()
}

#[derive(Clone)]
pub struct ServerTlsConfig {
    pub(crate) config: Arc<rustls::ServerConfig>,
}

#[derive(Clone)]
pub struct ClientTlsConfig {
    pub(crate) config: Arc<rustls::ClientConfig>,
    pub(crate) server_name: ServerName<'static>,
}

fn configuration(error: impl std::fmt::Display) -> NetError {
    NetError::Configuration(format!("TLS: {error}"))
}

fn read_pem(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_PEM_BYTES {
        return Err(configuration(
            "PEM input must be a regular file no larger than 1 MiB",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_PEM_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PEM_BYTES {
        return Err(configuration("PEM input exceeds 1 MiB"));
    }
    Ok(bytes)
}

fn certificates(bytes: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
    let chain = CertificateDer::pem_slice_iter(bytes)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(configuration)?;
    if chain.is_empty() || chain.len() > 32 {
        return Err(configuration(
            "certificate chain must contain 1..=32 certificates",
        ));
    }
    Ok(chain)
}

impl ServerTlsConfig {
    pub fn from_pem_files(
        certificate_chain: impl AsRef<Path>,
        private_key: impl AsRef<Path>,
    ) -> Result<Self> {
        Self::from_pem(
            &read_pem(certificate_chain.as_ref())?,
            &read_pem(private_key.as_ref())?,
        )
    }

    pub fn from_pem(certificate_chain: &[u8], private_key: &[u8]) -> Result<Self> {
        if certificate_chain.len() as u64 > MAX_PEM_BYTES
            || private_key.len() as u64 > MAX_PEM_BYTES
        {
            return Err(configuration("PEM input exceeds 1 MiB"));
        }
        let key = PrivateKeyDer::from_pem_slice(private_key).map_err(configuration)?;
        let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(configuration)?
        .with_no_client_auth()
        .with_single_cert(certificates(certificate_chain)?, key)
        .map_err(configuration)?;
        config.alpn_protocols = vec![alpn()];
        config.max_early_data_size = 0;
        Ok(Self {
            config: Arc::new(config),
        })
    }
}

impl ClientTlsConfig {
    pub fn from_ca_pem_file(ca: impl AsRef<Path>, server_name: impl Into<String>) -> Result<Self> {
        Self::from_ca_pem(&read_pem(ca.as_ref())?, server_name)
    }

    /// Every CA is explicit; no invalid-certificate or hostname bypass exists.
    pub fn from_ca_pem(ca: &[u8], server_name: impl Into<String>) -> Result<Self> {
        if ca.len() as u64 > MAX_PEM_BYTES {
            return Err(configuration("CA PEM exceeds 1 MiB"));
        }
        let server_name = ServerName::try_from(server_name.into()).map_err(configuration)?;
        let mut roots = rustls::RootCertStore::empty();
        for certificate in certificates(ca)? {
            roots.add(certificate).map_err(configuration)?;
        }
        let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(configuration)?
        .with_root_certificates(roots)
        .with_no_client_auth();
        config.alpn_protocols = vec![alpn()];
        config.enable_early_data = false;
        Ok(Self {
            config: Arc::new(config),
            server_name,
        })
    }
}
