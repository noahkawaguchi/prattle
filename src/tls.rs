use anyhow::{Result, anyhow};
use pem::Pem;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType, string::Ia5String};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::Path,
    str::FromStr,
    sync::Arc,
};
use tokio_rustls::rustls::ServerConfig;
use tracing::info;

const CERT_PATH: &str = "server.crt";
const KEY_PATH: &str = "server.key";

/// Creates a Tokio Rustls `ServerConfig` using a persistent self-signed certificate.
///
/// If certificate files (`server.crt` and `server.key`) exist, they are loaded. Otherwise, a new
/// self-signed certificate is generated and saved to file.
///
/// # Errors
///
/// Returns `Err` if certificate generation, file I/O, or config creation fails.
pub fn create_config() -> Result<Arc<ServerConfig>> {
    // Check if certificate files exist and load or regenerate them accordingly
    let (cert, key) = if Path::new(CERT_PATH).exists() && Path::new(KEY_PATH).exists() {
        info!("Loading existing TLS certificate from file");
        load_cert_and_key()?
    } else {
        info!("Generating new self-signed TLS certificate");
        let (cert, key) = generate_self_signed_cert_and_key()?;
        save_cert_and_key(&cert, &key)?;
        info!("Saved files {CERT_PATH} and {KEY_PATH}");
        (cert, key)
    };

    // Configure to use the self-signed certificate and not to require client certificates
    Ok(Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)?,
    ))
}

/// Generates a self-signed certificate and private key for TLS valid for localhost/127.0.0.1.
fn generate_self_signed_cert_and_key() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)>
{
    let mut params = CertificateParams::default();

    // Set certificate subject and provide human-readable names
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "Prattle Chat Server");
    distinguished_name.push(DnType::OrganizationName, "Prattle");
    params.distinguished_name = distinguished_name;

    // Add SANs to allow localhost/127.0.0.1 connections
    params.subject_alt_names = vec![
        SanType::DnsName(Ia5String::from_str("localhost")?),
        SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)),
    ];

    // Generate public/private key pair
    let key_pair = KeyPair::generate()?;

    Ok((
        // Generate and serialize self-signed cert
        params.self_signed(&key_pair)?.der().clone(),
        // Serialize private key
        PrivateKeyDer::try_from(key_pair.serialize_der())
            .map_err(|e| anyhow!("Failed to serialize private key: {e}"))?,
    ))
}

/// Saves a certificate and private key to file in PEM format.
fn save_cert_and_key(cert: &CertificateDer<'_>, key: &PrivateKeyDer<'_>) -> Result<()> {
    // Convert DER to PEM format and save as files
    fs::write(
        CERT_PATH,
        pem::encode(&Pem::new("CERTIFICATE", cert.as_ref())),
    )?;

    fs::write(
        KEY_PATH,
        pem::encode(&Pem::new("PRIVATE KEY", key.secret_der())),
    )?;

    Ok(())
}

/// Loads a certificate and private key from file in PEM format.
fn load_cert_and_key() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    // Read files, parse PEM, and convert to DER
    Ok((
        CertificateDer::from(
            pem::parse(&fs::read_to_string(CERT_PATH)?)?
                .contents()
                .to_vec(),
        ),
        PrivateKeyDer::try_from(
            pem::parse(&fs::read_to_string(KEY_PATH)?)?
                .contents()
                .to_vec(),
        )
        .map_err(|e| anyhow!("Failed to parse private key: {e}"))?,
    ))
}
