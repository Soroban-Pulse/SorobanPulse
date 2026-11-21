package soroban_pulse

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strconv"
	"strings"
	"time"
)

// WebhookSignatureHeader is the header Soroban Pulse signs webhook deliveries with.
const WebhookSignatureHeader = "X-Signature-256"

// WebhookTimestampHeader is the header carrying the Unix-seconds timestamp that
// is covered by the signature when timestamped signatures are used.
const WebhookTimestampHeader = "X-Signature-Timestamp"

var (
	// ErrMissingSignature means the request carried no signature header.
	ErrMissingSignature = errors.New("webhook: missing signature header")
	// ErrInvalidSignatureFormat means the header is not `sha256=<hex digest>`
	// (optionally prefixed with `t=<unix seconds>,`).
	ErrInvalidSignatureFormat = errors.New("webhook: signature header must be sha256=<hex digest> or t=<unix seconds>,sha256=<hex digest>")
	// ErrSignatureMismatch means no configured secret produced the signature.
	ErrSignatureMismatch = errors.New("webhook: signature mismatch")
	// ErrTimestampOutsideTolerance means the signed timestamp is too old or too
	// far in the future relative to the configured tolerance.
	ErrTimestampOutsideTolerance = errors.New("webhook: timestamp outside tolerance")
)

// VerifyWebhookOptions configures optional verification behaviour.
type VerifyWebhookOptions struct {
	// ToleranceSec is the maximum allowed absolute difference, in seconds,
	// between the signed timestamp and the current time. Zero disables the
	// timestamp check (but a timestamped signature must still be well formed).
	ToleranceSec int64
	// Now overrides the clock used for the tolerance check; nil means time.Now.
	Now func() time.Time
}

// VerifyWebhook checks a delivery's X-Signature-256 header against the raw
// request body, as described in docs/webhook_signing.md: the header is
// `sha256=` followed by the hex HMAC-SHA256 of the body keyed with the webhook
// secret. Pass several secrets during rotation; the request is accepted if any
// of them matches. Digests are compared in constant time.
//
// Timestamped signatures are also accepted: `t=<unix seconds>,sha256=<hex>`,
// where the HMAC is computed over `<t>.<body>`. Use VerifyWebhookWithOptions to
// enforce a timestamp tolerance.
func VerifyWebhook(payload []byte, signatureHeader string, secrets ...string) error {
	return VerifyWebhookWithOptions(payload, signatureHeader, VerifyWebhookOptions{}, secrets...)
}

// VerifyWebhookWithOptions is VerifyWebhook with configurable timestamp
// tolerance. When the signature header carries a timestamp and
// opts.ToleranceSec > 0, the timestamp must be within ToleranceSec seconds of
// opts.Now() (or time.Now() when nil).
func VerifyWebhookWithOptions(payload []byte, signatureHeader string, opts VerifyWebhookOptions, secrets ...string) error {
	if signatureHeader == "" {
		return ErrMissingSignature
	}

	signedPayload := payload
	var timestamp int64
	var hasTimestamp bool

	parts := strings.Split(signatureHeader, ",")
	var hexDigest string
	for _, part := range parts {
		part = strings.TrimSpace(part)
		switch {
		case strings.HasPrefix(part, "t="):
			raw := strings.TrimPrefix(part, "t=")
			ts, err := strconv.ParseInt(raw, 10, 64)
			if err != nil {
				return ErrInvalidSignatureFormat
			}
			timestamp = ts
			hasTimestamp = true
		case strings.HasPrefix(part, "sha256="):
			hexDigest = strings.TrimPrefix(part, "sha256=")
		default:
			return ErrInvalidSignatureFormat
		}
	}
	if hexDigest == "" {
		return ErrInvalidSignatureFormat
	}
	provided, err := hex.DecodeString(hexDigest)
	if err != nil {
		return ErrInvalidSignatureFormat
	}

	if hasTimestamp {
		if opts.ToleranceSec > 0 {
			now := time.Now
			if opts.Now != nil {
				now = opts.Now
			}
			diff := now().Unix() - timestamp
			if diff < 0 {
				diff = -diff
			}
			if diff > opts.ToleranceSec {
				return ErrTimestampOutsideTolerance
			}
		}
		signedPayload = []byte(strconv.FormatInt(timestamp, 10) + "." + string(payload))
	}

	matched := false
	for _, secret := range secrets {
		mac := hmac.New(sha256.New, []byte(secret))
		mac.Write(signedPayload)
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
