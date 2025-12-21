use anyhow::{Result, anyhow};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType, string::Ia5String};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::{
    net::{IpAddr, Ipv4Addr},
    str::FromStr,
    sync::Arc,
};
use tokio_rustls::rustls::ServerConfig;

/// Generates a self-signed certificate and private key for TLS valid for localhost/127.0.0.1.
fn generate_self_signed_cert() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
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
        // Generate and serialize self-signed certificate
        params.self_signed(&key_pair)?.der().clone(),
        // Serialize private key
        PrivateKeyDer::try_from(key_pair.serialize_der())
            .map_err(|e| anyhow!("Failed to serialize private key: {e}"))?,
    ))
}

/// Creates a rustls `ServerConfig` with a new self-signed certificate on each call.
///
/// # Errors
///
/// Returns `Err` if cert generation or config creation fails.
pub fn create_tls_config() -> Result<Arc<ServerConfig>> {
    let (cert, key) = generate_self_signed_cert()?;

    // Configure to use the self-signed cert and not to require client certificates
    Ok(Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)?,
    ))
}
