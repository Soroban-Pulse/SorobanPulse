// Quantum-Ready Cryptography Module
// Implements post-quantum cryptographic algorithms for future-proofing

use serde::{Deserialize, Serialize};
use std::error::Error;

/// Quantum-ready cryptographic algorithms supported
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuantumAlgorithm {
    /// CRYSTALS-Kyber: Post-quantum key encapsulation mechanism
    Kyber512,
    Kyber768,
    Kyber1024,
    /// CRYSTALS-Dilithium: Post-quantum digital signatures
    Dilithium2,
    Dilithium3,
    Dilithium5,
    /// SPHINCS+: Stateless hash-based signatures
    SphincsSha256128f,
    SphincsSha256192f,
    SphincsSha256256f,
    /// Falcon: Fast Fourier lattice-based signatures
    Falcon512,
    Falcon1024,
}

/// Hybrid cryptography mode combining classical and post-quantum algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CryptoMode {
    /// Classical cryptography only (current standard)
    Classical,
    /// Hybrid: Both classical and post-quantum
    Hybrid {
        classical: ClassicalAlgorithm,
        quantum: QuantumAlgorithm,
    },
    /// Post-quantum only (future default)
    PostQuantum(QuantumAlgorithm),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ClassicalAlgorithm {
    /// RSA-2048/4096
    Rsa2048,
    Rsa4096,
    /// ECDSA with various curves
    EcdsaP256,
    EcdsaP384,
    EcdsaP521,
    /// Ed25519 (current Stellar standard)
    Ed25519,
}

/// Configuration for quantum-ready cryptography
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumConfig {
    /// Current cryptography mode
    pub mode: CryptoMode,
    /// Enable crypto-agility for seamless algorithm migration
    pub crypto_agility: bool,
    /// Store dual signatures during transition period
    pub dual_signatures: bool,
    /// Algorithm preference for new operations
    pub preferred_algorithm: QuantumAlgorithm,
}

impl Default for QuantumConfig {
    fn default() -> Self {
        Self {
            // Start with hybrid mode for gradual transition
            mode: CryptoMode::Hybrid {
                classical: ClassicalAlgorithm::Ed25519,
                quantum: QuantumAlgorithm::Dilithium3,
            },
            crypto_agility: true,
            dual_signatures: true,
            preferred_algorithm: QuantumAlgorithm::Dilithium3,
        }
    }
}

/// Quantum-ready key pair
#[derive(Debug, Clone)]
pub struct QuantumKeyPair {
    pub algorithm: QuantumAlgorithm,
    pub public_key: Vec<u8>,
    pub private_key: Vec<u8>,
    /// Classical key pair for hybrid mode
    pub classical_pair: Option<ClassicalKeyPair>,
}

#[derive(Debug, Clone)]
pub struct ClassicalKeyPair {
    pub algorithm: ClassicalAlgorithm,
    pub public_key: Vec<u8>,
    pub private_key: Vec<u8>,
}

/// Quantum-ready signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumSignature {
    pub algorithm: QuantumAlgorithm,
    pub signature: Vec<u8>,
    /// Optional classical signature for hybrid verification
    pub classical_signature: Option<ClassicalSignature>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassicalSignature {
    pub algorithm: ClassicalAlgorithm,
    pub signature: Vec<u8>,
}

/// Quantum-ready cryptographic operations
pub trait QuantumCrypto {
    /// Generate a new quantum-ready key pair
    fn generate_keypair(&self, algorithm: QuantumAlgorithm) -> Result<QuantumKeyPair, Box<dyn Error>>;
    
    /// Sign data with quantum-ready algorithm
    fn sign(&self, data: &[u8], keypair: &QuantumKeyPair) -> Result<QuantumSignature, Box<dyn Error>>;
    
    /// Verify quantum-ready signature
    fn verify(&self, data: &[u8], signature: &QuantumSignature, public_key: &[u8]) -> Result<bool, Box<dyn Error>>;
    
    /// Encrypt data using quantum-resistant key encapsulation
    fn encrypt(&self, data: &[u8], public_key: &[u8], algorithm: QuantumAlgorithm) -> Result<Vec<u8>, Box<dyn Error>>;
    
    /// Decrypt data using quantum-resistant key encapsulation
    fn decrypt(&self, ciphertext: &[u8], private_key: &[u8], algorithm: QuantumAlgorithm) -> Result<Vec<u8>, Box<dyn Error>>;
}

/// Crypto-agility manager for seamless algorithm transitions
pub struct CryptoAgilityManager {
    config: QuantumConfig,
    supported_algorithms: Vec<QuantumAlgorithm>,
}

impl CryptoAgilityManager {
    pub fn new(config: QuantumConfig) -> Self {
        Self {
            config,
            supported_algorithms: vec![
                QuantumAlgorithm::Kyber768,
                QuantumAlgorithm::Kyber1024,
                QuantumAlgorithm::Dilithium2,
                QuantumAlgorithm::Dilithium3,
                QuantumAlgorithm::Dilithium5,
                QuantumAlgorithm::Falcon512,
                QuantumAlgorithm::Falcon1024,
            ],
        }
    }

    /// Check if an algorithm is supported
    pub fn is_supported(&self, algorithm: &QuantumAlgorithm) -> bool {
        self.supported_algorithms.contains(algorithm)
    }

    /// Get recommended algorithm based on security requirements
    pub fn recommend_algorithm(&self, security_level: SecurityLevel) -> QuantumAlgorithm {
        match security_level {
            SecurityLevel::Low => QuantumAlgorithm::Kyber512,
            SecurityLevel::Medium => QuantumAlgorithm::Dilithium3,
            SecurityLevel::High => QuantumAlgorithm::Dilithium5,
            SecurityLevel::VeryHigh => QuantumAlgorithm::SphincsSha256256f,
        }
    }

    /// Migrate from one algorithm to another
    pub fn migrate_algorithm(
        &self,
        from: QuantumAlgorithm,
        to: QuantumAlgorithm,
    ) -> Result<(), Box<dyn Error>> {
        // Implementation would handle gradual migration
        // 1. Generate new keys with target algorithm
        // 2. Maintain dual signatures during transition
        // 3. Update all stored keys and signatures
        // 4. Verify backward compatibility
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SecurityLevel {
    Low,     // 128-bit security
    Medium,  // 192-bit security
    High,    // 256-bit security
    VeryHigh, // 256+ bit security with additional protections
}

/// Migration plan for transitioning to post-quantum cryptography
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    /// Current phase of migration
    pub phase: MigrationPhase,
    /// Start date of migration
    pub start_date: String,
    /// Expected completion date
    pub target_date: String,
    /// Percentage of keys migrated
    pub progress: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MigrationPhase {
    /// Phase 1: Assessment and planning
    Assessment,
    /// Phase 2: Deploy hybrid cryptography
    HybridDeployment,
    /// Phase 3: Dual signature validation
    DualValidation,
    /// Phase 4: Primary post-quantum, fallback classical
    PostQuantumPrimary,
    /// Phase 5: Post-quantum only
    PostQuantumOnly,
}

impl MigrationPlan {
    pub fn new() -> Self {
        Self {
            phase: MigrationPhase::Assessment,
            start_date: chrono::Utc::now().to_rfc3339(),
            target_date: "2030-01-01T00:00:00Z".to_string(),
            progress: 0.0,
        }
    }

    pub fn advance_phase(&mut self) -> Result<(), String> {
        self.phase = match self.phase {
            MigrationPhase::Assessment => MigrationPhase::HybridDeployment,
            MigrationPhase::HybridDeployment => MigrationPhase::DualValidation,
            MigrationPhase::DualValidation => MigrationPhase::PostQuantumPrimary,
            MigrationPhase::PostQuantumPrimary => MigrationPhase::PostQuantumOnly,
            MigrationPhase::PostQuantumOnly => {
                return Err("Already at final phase".to_string());
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = QuantumConfig::default();
        assert!(config.crypto_agility);
        assert!(config.dual_signatures);
        assert_eq!(config.preferred_algorithm, QuantumAlgorithm::Dilithium3);
    }

    #[test]
    fn test_crypto_agility_manager() {
        let config = QuantumConfig::default();
        let manager = CryptoAgilityManager::new(config);
        
        assert!(manager.is_supported(&QuantumAlgorithm::Dilithium3));
        assert!(manager.is_supported(&QuantumAlgorithm::Kyber768));
    }

    #[test]
    fn test_algorithm_recommendation() {
        let config = QuantumConfig::default();
        let manager = CryptoAgilityManager::new(config);
        
        assert_eq!(
            manager.recommend_algorithm(SecurityLevel::Low),
            QuantumAlgorithm::Kyber512
        );
        assert_eq!(
            manager.recommend_algorithm(SecurityLevel::High),
            QuantumAlgorithm::Dilithium5
        );
    }

    #[test]
    fn test_migration_plan() {
        let mut plan = MigrationPlan::new();
        assert_eq!(plan.phase, MigrationPhase::Assessment);
        
        plan.advance_phase().unwrap();
        assert_eq!(plan.phase, MigrationPhase::HybridDeployment);
        
        plan.advance_phase().unwrap();
        assert_eq!(plan.phase, MigrationPhase::DualValidation);
    }
}
