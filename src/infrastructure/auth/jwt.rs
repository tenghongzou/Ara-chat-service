//! JWT token validation

use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT configuration
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub issuer: Option<String>,
    pub audience: Option<String>,
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
    pub fn new(config: &JwtConfig) -> Self {
        let decoding_key = DecodingKey::from_secret(config.secret.as_bytes());

        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        if let Some(ref issuer) = config.issuer {
            validation.set_issuer(&[issuer]);
        }

        if let Some(ref audience) = config.audience {
            validation.set_audience(&[audience]);
        }

        Self {
            decoding_key,
            validation,
        }
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
}
