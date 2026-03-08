use std::sync::Arc;

use rcgen::{CertificateParams, KeyPair};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error, ServerConfig, SignatureScheme};

/// 시험용 TLS 설정 묶음.
///
/// rcgen으로 자체 서명 인증서를 생성하고,
/// 클라이언트는 인증서 검증을 비활성화(NoCertVerifier)하여
/// IP 주소로 직접 연결해도 동작한다.
///
/// ALPN:
/// - server_config: ["h2", "http/1.1"] (둘 다 지원)
/// - client_h1_config: ["http/1.1"]
/// - client_h2_config: ["h2"]
pub struct TlsBundle {
    pub server_config: Arc<ServerConfig>,
    pub client_h1_config: Arc<ClientConfig>,
    pub client_h2_config: Arc<ClientConfig>,
}

/// 자체 서명 인증서 기반 TlsBundle을 빌드한다.
pub fn build() -> anyhow::Result<TlsBundle> {
    // 자체 서명 서버 인증서 생성 (SAN: test.net-meter.com)
    let params = CertificateParams::new(vec!["test.net-meter.com".to_string()])
        .map_err(|e| anyhow::anyhow!("rcgen params: {}", e))?;
    let key_pair = KeyPair::generate()
        .map_err(|e| anyhow::anyhow!("rcgen keygen: {}", e))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| anyhow::anyhow!("rcgen self_signed: {}", e))?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
        PrivatePkcs8KeyDer::from(key_pair.serialize_der()),
    );

    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .map_err(|e| anyhow::anyhow!("ServerConfig: {}", e))?;
    // ALPN: 서버는 h2와 http/1.1 모두 수락
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let mut client_h1_config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertVerifier))
        .with_no_client_auth();
    client_h1_config.alpn_protocols = vec![b"http/1.1".to_vec()];

    let mut client_h2_config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertVerifier))
        .with_no_client_auth();
    client_h2_config.alpn_protocols = vec![b"h2".to_vec()];

    Ok(TlsBundle {
        server_config: Arc::new(server_config),
        client_h1_config: Arc::new(client_h1_config),
        client_h2_config: Arc::new(client_h2_config),
    })
}

/// 서버 인증서를 검증하지 않는 클라이언트 검증기.
///
/// net-meter는 자체 서명 인증서를 사용하는 시험 도구이므로,
/// 인증서 검증을 우회하여 IP 주소로도 TLS 연결이 가능하게 한다.
#[derive(Debug)]
struct NoCertVerifier;

impl ServerCertVerifier for NoCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}
