package soroban_pulse

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

func testClient(url string) *Client {
	return NewClient(ClientConfig{
		BaseURL:           url,
		APIKey:            "sk_test",
		Timeout:           100 * time.Millisecond,
		MaxRetries:        2,
		RetryInitialDelay: time.Millisecond,
		RetryMaxDelay:     2 * time.Millisecond,
	})
}

func TestGetEvents(t *testing.T) {
	tests := []struct {
		name      string
		status    int
		body      string
		delay     time.Duration
		wantCalls int32
		wantTotal int64
		wantAPI   int // expected APIError status, 0 for none
		wantErr   bool
	}{
		{name: "success", status: 200, body: `{"data":[{"id":"e1","contract_id":"CABC","ledger":7}],"total":1,"page":1,"limit":20}`, wantCalls: 1, wantTotal: 1},
		{name: "4xx is not retried", status: 400, body: `{"error":"invalid limit"}`, wantCalls: 1, wantAPI: 400, wantErr: true},
		{name: "5xx is retried then reported", status: 503, body: `{"error":"unavailable"}`, wantCalls: 3, wantAPI: 503, wantErr: true},
		{name: "timeout", status: 200, body: `{}`, delay: 300 * time.Millisecond, wantCalls: 3, wantErr: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var calls int32
			var gotQuery, gotKey string
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				atomic.AddInt32(&calls, 1)
				gotQuery, gotKey = r.URL.RequestURI(), r.Header.Get("X-Api-Key")
				if tt.delay > 0 {
					select {
					case <-time.After(tt.delay):
					case <-r.Context().Done():
						return
					}
				}
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(tt.status)
				_, _ = w.Write([]byte(tt.body))
			}))
			defer srv.Close()

			opts := NewGetEventsOptions()
			opts.EventType = "contract"
			resp, err := testClient(srv.URL).GetEvents(context.Background(), opts)

			if got := atomic.LoadInt32(&calls); got != tt.wantCalls {
				t.Errorf("server calls = %d, want %d", got, tt.wantCalls)
			}
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error, got %+v", resp)
				}
				var apiErr *APIError
				if tt.wantAPI != 0 && (!errors.As(err, &apiErr) || apiErr.StatusCode != tt.wantAPI) {
					t.Fatalf("err = %v, want APIError %d", err, tt.wantAPI)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if resp.Total != tt.wantTotal || len(resp.Data) != 1 || resp.Data[0].ContractID != "CABC" || resp.Data[0].Ledger != 7 {
				t.Fatalf("decoded %+v", resp)
			}
			if gotQuery != "/v1/events?event_type=contract&limit=20&page=1" || gotKey != "sk_test" {
				t.Fatalf("request %q key %q", gotQuery, gotKey)
			}
		})
	}
}
