use anyhow::Result;
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use std::fs;

/// A certificate verifier that validates against a pinned certificate from file.
///
/// This verifier loads the server certificate from `CERT_PATH` and ensures that the server presents
/// exactly that certificate during the TLS handshake.
#[derive(Debug)]
pub struct PinnedCertVerifier {
    expected_cert: CertificateDer<'static>,
}

impl PinnedCertVerifier {
    /// Creates a new pinned certificate verifier by loading the certificate from file.
    pub fn from_file() -> Result<Self> {
        Ok(Self {
            expected_cert: CertificateDer::from(
                pem::parse(&fs::read_to_string(prattle::tls::CERT_PATH)?)?
                    .contents()
                    .to_vec(),
            ),
        })
    }
}

impl ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // Verify that the server's certificate matches the pinned certificate
        if end_entity.as_ref() == self.expected_cert.as_ref() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::CryptoProvider::get_default()
                .ok_or(rustls::Error::General(String::from(
                    "No default crypto provider",
                )))?
                .signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::CryptoProvider::get_default()
                .ok_or(rustls::Error::General(String::from(
                    "No default crypto provider",
                )))?
                .signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::CryptoProvider::get_default()
            .map(|provider| {
                provider
                    .signature_verification_algorithms
                    .supported_schemes()
            })
            .unwrap_or_default()
    }
}
