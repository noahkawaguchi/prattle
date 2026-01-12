use anyhow::{Result, anyhow};
use pem::Pem;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType, string::Ia5String};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer},
};
use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::Path,
    str::FromStr,
    sync::{Arc, Mutex, OnceLock},
};
use tracing::info;

/// The file path for the server's certificate (public key and metadata) for TLS.
const CERT_PATH: &str = "server.crt";

/// The file path for the server's private key for TLS.
const KEY_PATH: &str = "server.key";

/// Global lock to ensure certificate generation happens only once across concurrent threads.
static CERT_FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Creates a Rustls `ServerConfig` using a persistent self-signed certificate.
///
/// If certificate files (`CERT_PATH` and `KEY_PATH`) exist, they are loaded. Otherwise, a new
/// self-signed certificate is generated and saved to file.
///
/// This function uses a lock to ensure that certificate generation is atomic across threads,
/// preventing race conditions when multiple servers/tests start simultaneously.
///
/// # Errors
///
/// Returns `Err` if certificate generation, file I/O, or config creation fails.
pub fn create_config() -> Result<Arc<ServerConfig>> {
    // Get/initialize and acquire the lock to ensure atomic check/generate
    let guard = CERT_FILE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| anyhow!("Lock poisoned: {e}"))?;

    // Check if certificate files exist and load/regenerate them accordingly while holding the lock
    let files_found = Path::new(CERT_PATH).exists() && Path::new(KEY_PATH).exists();

    let (cert, key) = if files_found {
        load_cert_and_key()?
    } else {
        let (cert, key) = generate_self_signed_cert_and_key()?;
        save_cert_and_key(&cert, &key)?;
        (cert, key)
    };

    drop(guard);

    if files_found {
        info!("Loaded existing TLS certificate from file");
    } else {
        info!("Generated and saved new self-signed TLS certificate");
    }

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
