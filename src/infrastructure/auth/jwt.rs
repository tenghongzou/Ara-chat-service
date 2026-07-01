//! JWT token validation

use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use std::fs;
use uuid::Uuid;

use crate::domain::validation::limits;

/// JWT configuration
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// Symmetric secret — only used for the HS256 fallback.
    pub secret: Option<String>,
    pub issuer: Option<String>,
    pub audience: Option<String>,
    /// Signature algorithm: "RS256" (asymmetric, recommended) or "HS256".
    pub algorithm: Option<String>,
    /// Path to the RSA public key PEM — required for RS256.
    pub publickey: Option<String>,
}

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    #[serde(default)]
    pub iat: usize,
    #[serde(default)]
    pub iss: Option<String>,
    #[serde(default)]
    pub aud: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
}

impl Claims {
    /// Get user ID from claims
    pub fn user_id(&self) -> Result<Uuid, JwtError> {
        Uuid::parse_str(&self.sub).map_err(|_| JwtError::InvalidSubject)
    }

    /// Get tenant ID, defaulting to "default"
    pub fn tenant_id(&self) -> String {
        self.tenant_id.clone().unwrap_or_else(|| "default".to_string())
    }
}

/// JWT validator
pub struct JwtValidator {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtValidator {
    pub fn new(config: &JwtConfig) -> Result<Self, JwtError> {
        let algorithm = config.algorithm.as_deref().unwrap_or("HS256");

        let (decoding_key, alg) = match algorithm {
            "RS256" => {
                // Asymmetric verification with the backend's RSA public key.
                // Chat never holds a signing-capable secret, so knowledge of its
                // config cannot be used to forge tokens for arbitrary users.
                let pem_path = config
                    .publickey
                    .as_deref()
                    .ok_or(JwtError::MissingPublicKey)?;
                let pem = fs::read(pem_path)
                    .map_err(|e| JwtError::PublicKeyRead(pem_path.to_string(), e.to_string()))?;
                let key = DecodingKey::from_rsa_pem(&pem)
                    .map_err(|e| JwtError::InvalidPublicKey(e.to_string()))?;
                (key, Algorithm::RS256)
            }
            _ => {
                // Symmetric fallback (HS256): require a sufficiently long secret.
                let secret = config.secret.as_deref().ok_or(JwtError::MissingSecret)?;
                if secret.len() < limits::MIN_JWT_SECRET_LENGTH {
                    return Err(JwtError::SecretTooShort {
                        min: limits::MIN_JWT_SECRET_LENGTH,
                        actual: secret.len(),
                    });
                }
                (DecodingKey::from_secret(secret.as_bytes()), Algorithm::HS256)
            }
        };

        let mut validation = Validation::new(alg);
        validation.validate_exp = true;

        if let Some(ref issuer) = config.issuer {
            validation.set_issuer(&[issuer]);
        }

        if let Some(ref audience) = config.audience {
            validation.set_audience(&[audience]);
        }

        Ok(Self {
            decoding_key,
            validation,
        })
    }

    /// Validate a JWT token
    pub fn validate(&self, token: &str) -> Result<Claims, JwtError> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map_err(|e| JwtError::Validation(e.to_string()))?;

        Ok(token_data.claims)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("Token validation failed: {0}")]
    Validation(String),

    #[error("Invalid subject in token")]
    InvalidSubject,

    #[error("JWT secret too short (minimum {min} characters required, got {actual})")]
    SecretTooShort { min: usize, actual: usize },

    #[error("JWT secret is required for HS256 but was not provided")]
    MissingSecret,

    #[error("JWT public key path is required for RS256 but was not provided")]
    MissingPublicKey,

    #[error("Failed to read JWT public key {0}: {1}")]
    PublicKeyRead(String, String),

    #[error("Invalid RSA public key: {0}")]
    InvalidPublicKey(String),
}
