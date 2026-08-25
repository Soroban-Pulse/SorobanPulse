# Quantum-Ready Cryptography

## Overview

SorobanPulse implements post-quantum cryptographic algorithms to prepare for the era of quantum computing. This document outlines the quantum-resistant features, migration strategy, and implementation details.

## Why Quantum-Ready Cryptography?

### Threat Landscape

**Quantum Computing Risks**:
- Large-scale quantum computers could break current public-key cryptography (RSA, ECDSA, Ed25519)
- "Store now, decrypt later" attacks pose immediate risks to long-term data confidentiality
- NIST estimates quantum computers capable of breaking current cryptography by 2030-2035

**Timeline**:
- **2024-2026**: Hybrid cryptography deployment
- **2027-2029**: Post-quantum primary with classical fallback
- **2030+**: Post-quantum only

## Supported Algorithms

### Key Encapsulation Mechanisms (KEM)

#### CRYSTALS-Kyber ✅ NIST Standard
- **Kyber-512**: NIST Security Level 1 (128-bit)
- **Kyber-768**: NIST Security Level 3 (192-bit) - **Recommended**
- **Kyber-1024**: NIST Security Level 5 (256-bit)

**Use Cases**:
- TLS/HTTPS connections
- Secure key exchange
- Encrypted data storage

### Digital Signatures

#### CRYSTALS-Dilithium ✅ NIST Standard
- **Dilithium2**: NIST Security Level 2
- **Dilithium3**: NIST Security Level 3 - **Recommended**
- **Dilithium5**: NIST Security Level 5

**Use Cases**:
- API request signing
- Webhook verification
- Transaction signatures

#### SPHINCS+ ✅ NIST Standard
- **SPHINCS+-SHA256-128f**: Fast, smaller signatures
- **SPHINCS+-SHA256-192f**: Medium security
- **SPHINCS+-SHA256-256f**: Highest security

**Use Cases**:
- Long-term signatures
- Critical infrastructure
- Regulatory compliance

#### Falcon (Alternative)
- **Falcon-512**: Compact signatures
- **Falcon-1024**: Higher security

**Use Cases**:
- Resource-constrained environments
- Mobile applications

## Architecture

### Hybrid Cryptography Mode

```
┌─────────────────────────────────────┐
│     Hybrid Cryptography Layer       │
├─────────────────┬───────────────────┤
│   Classical     │  Post-Quantum     │
│   (Ed25519)     │  (Dilithium3)     │
├─────────────────┼───────────────────┤
│  Sign & Verify  │  Sign & Verify    │
│  Both Required  │  Primary Method   │
└─────────────────┴───────────────────┘
```

### Crypto-Agility Framework

```rust
// Configuration
{
  "mode": "Hybrid",
  "classical_algorithm": "Ed25519",
  "quantum_algorithm": "Dilithium3",
  "crypto_agility": true,
  "dual_signatures": true
}
```

**Benefits**:
- Seamless algorithm migration
- Backward compatibility
- Zero-downtime transitions
- Future-proof architecture

## Configuration

### Environment Variables

```bash
# Enable post-quantum cryptography
QUANTUM_CRYPTO_ENABLED=true

# Cryptography mode: classical, hybrid, post-quantum
CRYPTO_MODE=hybrid

# Preferred post-quantum algorithm
PQ_SIGNATURE_ALGORITHM=Dilithium3
PQ_KEM_ALGORITHM=Kyber768

# Enable dual signatures during migration
DUAL_SIGNATURES=true

# Crypto agility for algorithm migration
CRYPTO_AGILITY=true
```

### Configuration File

```toml
[crypto]
mode = "hybrid"
crypto_agility = true
dual_signatures = true

[crypto.classical]
signature = "Ed25519"
key_size = 256

[crypto.post_quantum]
signature = "Dilithium3"
kem = "Kyber768"

[crypto.migration]
phase = "HybridDeployment"
start_date = "2024-01-01"
target_date = "2030-01-01"
progress = 15.5
```

## API Usage

### Generating Keys

```rust
use soroban_pulse::crypto::quantum_ready::*;

// Generate hybrid key pair
let config = QuantumConfig {
    mode: CryptoMode::Hybrid {
        classical: ClassicalAlgorithm::Ed25519,
        quantum: QuantumAlgorithm::Dilithium3,
    },
    crypto_agility: true,
    dual_signatures: true,
    preferred_algorithm: QuantumAlgorithm::Dilithium3,
};

let keypair = generate_hybrid_keypair(&config)?;
```

### Signing Data

```rust
// Sign with hybrid mode (both classical and post-quantum)
let signature = sign_hybrid(
    message.as_bytes(),
    &keypair,
    &config
)?;

// Signature includes both:
// - Ed25519 signature (32 bytes)
// - Dilithium3 signature (~2420 bytes)
```

### Verifying Signatures

```rust
// Verify hybrid signature
let is_valid = verify_hybrid(
    message.as_bytes(),
    &signature,
    &keypair.public_key,
    &config
)?;

// Both signatures must be valid
assert!(is_valid);
```

### Key Encapsulation

```rust
// Encapsulate shared secret with Kyber
let (ciphertext, shared_secret) = encapsulate_kyber768(
    &recipient_public_key
)?;

// Decapsulate on recipient side
let shared_secret = decapsulate_kyber768(
    &ciphertext,
    &recipient_private_key
)?;
```

## Migration Strategy

### Phase 1: Assessment (Current)

**Objectives**:
- Inventory all cryptographic operations
- Identify dependencies and constraints
- Test post-quantum algorithm implementations
- Develop migration timeline

**Actions**:
- ✅ Implement quantum-ready module
- ✅ Add hybrid cryptography support
- ✅ Create migration plan
- 🔄 Test with production data

### Phase 2: Hybrid Deployment (2024-2026)

**Objectives**:
- Deploy hybrid cryptography in production
- Generate dual signatures for all operations
- Validate both classical and post-quantum signatures
- Monitor performance impact

**Actions**:
- Enable hybrid mode in configuration
- Update all signing operations
- Add dual verification logic
- Deploy to staging environment
- Gradual production rollout

### Phase 3: Dual Validation (2026-2028)

**Objectives**:
- Require both signatures for critical operations
- Build confidence in post-quantum algorithms
- Identify and resolve compatibility issues
- Train operations team

**Actions**:
- Enforce dual signature validation
- Monitor error rates
- Performance optimization
- Update documentation

### Phase 4: Post-Quantum Primary (2028-2030)

**Objectives**:
- Make post-quantum the primary method
- Maintain classical as fallback
- Prepare for full transition
- Update client libraries

**Actions**:
- Switch to post-quantum first
- Classical signatures optional
- Update API documentation
- Client SDK updates

### Phase 5: Post-Quantum Only (2030+)

**Objectives**:
- Complete migration to post-quantum
- Remove classical cryptography support
- Achieve quantum resistance
- Maintain crypto-agility for future algorithms

**Actions**:
- Disable classical algorithms
- Post-quantum only mode
- Remove legacy code
- Continuous security monitoring

## Performance Considerations

### Signature Sizes

| Algorithm | Public Key | Private Key | Signature |
|-----------|------------|-------------|-----------|
| Ed25519 (Classical) | 32 bytes | 32 bytes | 64 bytes |
| Dilithium2 | 1,312 bytes | 2,528 bytes | 2,420 bytes |
| Dilithium3 | 1,952 bytes | 4,000 bytes | 3,293 bytes |
| Dilithium5 | 2,592 bytes | 4,864 bytes | 4,595 bytes |
| Falcon-512 | 897 bytes | 1,281 bytes | 666 bytes |
| SPHINCS+-128f | 32 bytes | 64 bytes | 17,088 bytes |

### Performance Impact

**Signing Speed**:
- Ed25519: ~50,000 signatures/sec
- Dilithium3: ~8,000 signatures/sec (6x slower)
- Falcon-512: ~12,000 signatures/sec (4x slower)

**Verification Speed**:
- Ed25519: ~20,000 verifications/sec
- Dilithium3: ~14,000 verifications/sec (1.4x slower)
- Falcon-512: ~16,000 verifications/sec (1.25x slower)

**Mitigation Strategies**:
- Signature caching
- Batch verification
- Hardware acceleration
- Algorithm selection based on use case

## Security Best Practices

### 1. Crypto-Agility

Always design systems to support multiple algorithms:
- Use algorithm identifiers in signatures
- Version all cryptographic formats
- Support algorithm negotiation
- Plan for future migrations

### 2. Hybrid Mode During Transition

Never rely solely on new algorithms during transition:
- Require both classical and post-quantum signatures
- Validate independently
- Fail closed on verification errors
- Monitor algorithm-specific failures

### 3. Key Management

- Store post-quantum keys securely (HSM/KMS)
- Implement key rotation policies
- Backup keys with quantum-resistant encryption
- Use key derivation functions (KDFs)

### 4. Performance Optimization

- Cache verified signatures
- Use faster algorithms for non-critical operations
- Implement signature batching
- Consider hardware acceleration

### 5. Monitoring and Alerts

- Track algorithm usage
- Monitor performance metrics
- Alert on verification failures
- Audit cryptographic operations

## Testing

### Unit Tests

```bash
cargo test --features quantum-crypto
```

### Integration Tests

```bash
# Test hybrid signing
cargo test test_hybrid_signatures

# Test migration scenarios
cargo test test_algorithm_migration

# Performance benchmarks
cargo bench quantum_crypto
```

### Compatibility Testing

```bash
# Test with different algorithm combinations
./scripts/test-crypto-compatibility.sh
```

## Compliance & Standards

### NIST Post-Quantum Cryptography

**Selected Algorithms** (2022):
- ✅ CRYSTALS-Kyber (KEM)
- ✅ CRYSTALS-Dilithium (Signatures)
- ✅ SPHINCS+ (Signatures)
- ✅ Falcon (Signatures)

### Industry Standards

- **CNSA 2.0** (NSA): Quantum-resistant algorithms by 2030
- **BSI** (Germany): Post-quantum migration requirements
- **ANSSI** (France): Hybrid cryptography recommendations
- **ISO**: ISO/IEC 14888-3 (Digital signatures)

## Future Technologies

### Quantum Key Distribution (QKD)

While SorobanPulse focuses on post-quantum cryptography, QKD may complement the solution:
- Unconditional security based on physics
- Requires specialized hardware
- Limited to specific use cases
- Expensive infrastructure

### Homomorphic Encryption

Future enhancements may include quantum-resistant homomorphic encryption:
- Compute on encrypted data
- Privacy-preserving analytics
- Regulatory compliance
- Research ongoing

## Resources

### Documentation

- [NIST Post-Quantum Cryptography](https://csrc.nist.gov/projects/post-quantum-cryptography)
- [Quantum-Safe Cryptography](https://www.etsi.org/technologies/quantum-safe-cryptography)
- [RFC 8784: Mixing Preshared Keys in IKEv2](https://www.rfc-editor.org/rfc/rfc8784.html)

### Libraries

- [Open Quantum Safe](https://openquantumsafe.org/)
- [pqcrypto](https://github.com/rustpq/pqcrypto)
- [liboqs](https://github.com/open-quantum-safe/liboqs)

### Training

- SorobanPulse Security Specialist (SPSS) Certification
- Quantum-Safe Cryptography Workshop
- Migration Planning Training

## Support

For questions about quantum-ready cryptography:
- Documentation: https://docs.soroban-pulse.example.com/quantum
- Security Team: security@soroban-pulse.example.com
- Community: #quantum-crypto channel

## Changelog

- **v1.0.0** (2024-01): Initial quantum-ready implementation
- **v1.1.0** (2024-06): Hybrid mode deployment
- **v2.0.0** (2027): Post-quantum primary mode
- **v3.0.0** (2030): Post-quantum only mode
