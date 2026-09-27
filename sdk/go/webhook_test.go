package soroban_pulse

import (
	"encoding/json"
	"errors"
	"os"
	"testing"
)

type webhookVector struct {
	Name            string   `json:"name"`
	Secrets         []string `json:"secrets"`
	Payload         string   `json:"payload"`
	SignatureHeader string   `json:"signature_header"`
	Valid           bool     `json:"valid"`
	Error           string   `json:"error"`
}

func TestVerifyWebhookSharedVectors(t *testing.T) {
	raw, err := os.ReadFile("../testdata/webhook_vectors.json")
	if err != nil {
		t.Fatal(err)
	}
	var doc struct {
		Header  string          `json:"header"`
		Vectors []webhookVector `json:"vectors"`
	}
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatal(err)
	}
	if doc.Header != WebhookSignatureHeader || len(doc.Vectors) == 0 {
		t.Fatalf("unexpected vector file: header %q, %d vectors", doc.Header, len(doc.Vectors))
	}

	wantErr := map[string]error{
		"missing":  ErrMissingSignature,
		"format":   ErrInvalidSignatureFormat,
		"mismatch": ErrSignatureMismatch,
	}
	for _, v := range doc.Vectors {
		t.Run(v.Name, func(t *testing.T) {
			err := VerifyWebhook([]byte(v.Payload), v.SignatureHeader, v.Secrets...)
			if v.Valid {
				if err != nil {
					t.Fatalf("expected valid, got %v", err)
				}
				return
			}
			if !errors.Is(err, wantErr[v.Error]) {
				t.Fatalf("err = %v, want %v", err, wantErr[v.Error])
			}
		})
	}
}

func TestVerifyWebhookWithoutSecretsRejects(t *testing.T) {
	if err := VerifyWebhook([]byte("{}"), "sha256=00"); !errors.Is(err, ErrSignatureMismatch) {
		t.Fatalf("err = %v, want ErrSignatureMismatch", err)
	}
}
