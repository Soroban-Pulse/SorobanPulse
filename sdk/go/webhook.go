package soroban_pulse

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strings"
)

// WebhookSignatureHeader is the header Soroban Pulse signs webhook deliveries with.
const WebhookSignatureHeader = "X-Signature-256"

var (
	// ErrMissingSignature means the request carried no signature header.
	ErrMissingSignature = errors.New("webhook: missing signature header")
	// ErrInvalidSignatureFormat means the header is not `sha256=<hex digest>`.
	ErrInvalidSignatureFormat = errors.New("webhook: signature header must be sha256=<hex digest>")
	// ErrSignatureMismatch means no configured secret produced the signature.
	ErrSignatureMismatch = errors.New("webhook: signature mismatch")
)

// VerifyWebhook checks a delivery's X-Signature-256 header against the raw
// request body, as described in docs/webhook_signing.md: the header is
// `sha256=` followed by the hex HMAC-SHA256 of the body keyed with the webhook
// secret. Pass several secrets during rotation; the request is accepted if any
// of them matches. Digests are compared in constant time.
func VerifyWebhook(payload []byte, signatureHeader string, secrets ...string) error {
	if signatureHeader == "" {
		return ErrMissingSignature
	}
	hexDigest, ok := strings.CutPrefix(signatureHeader, "sha256=")
	if !ok {
		return ErrInvalidSignatureFormat
	}
	provided, err := hex.DecodeString(hexDigest)
	if err != nil {
		return ErrInvalidSignatureFormat
	}

	matched := false
	for _, secret := range secrets {
		mac := hmac.New(sha256.New, []byte(secret))
		mac.Write(payload)
		// Check every secret so timing does not reveal which one matched.
		if hmac.Equal(provided, mac.Sum(nil)) {
			matched = true
		}
	}
	if !matched {
		return ErrSignatureMismatch
	}
	return nil
}
