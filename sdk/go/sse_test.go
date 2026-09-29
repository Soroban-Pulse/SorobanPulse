package soroban_pulse

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestSSEReaderParsesMessages(t *testing.T) {
	body := ": keep-alive\n\n" +
		"id: 1\r\ndata: {\"a\":1}\r\n\r\n" +
		"event: lag\nretry: 3000\ndata: line1\ndata:line2\n\n" +
		"id: 9\n\n" +
		"data: unterminated"

	r := newSSEReader(strings.NewReader(body))

	msg, err := r.Next()
	if err != nil || msg != (sseMessage{ID: "1", Event: "message", Data: `{"a":1}`}) {
		t.Fatalf("first message = %+v, %v", msg, err)
	}
	msg, err = r.Next()
	if err != nil || msg != (sseMessage{Event: "lag", Data: "line1\nline2"}) {
		t.Fatalf("second message = %+v, %v", msg, err)
	}
	if _, err = r.Next(); err != io.EOF {
		t.Fatalf("expected io.EOF for id-only and unterminated frames, got %v", err)
	}
	if r.LastEventID != "9" {
		t.Fatalf("LastEventID = %q, want 9", r.LastEventID)
	}
}

func sseServer(t *testing.T, handler http.HandlerFunc) *Client {
	t.Helper()
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)
	return NewClient(ClientConfig{BaseURL: srv.URL, APIKey: "sk_test", Timeout: 50 * time.Millisecond})
}

func writeSSE(w http.ResponseWriter, frames ...string) {
	for _, f := range frames {
		fmt.Fprint(w, f)
	}
	w.(http.Flusher).Flush()
}

func TestStreamEventsDeliversEvents(t *testing.T) {
	var gotPath, gotKey, gotAccept string
	client := sseServer(t, func(w http.ResponseWriter, r *http.Request) {
		gotPath, gotKey, gotAccept = r.URL.RequestURI(), r.Header.Get("X-Api-Key"), r.Header.Get("Accept")
		w.Header().Set("Content-Type", "text/event-stream")
		writeSSE(w, ": ka\n\n", "id: 1\ndata: {\"id\":\"e1\",\"ledger\":5}\n\n")
		// Outlive the client's 50ms Timeout: streams must not be cut off by it.
		time.Sleep(120 * time.Millisecond)
		writeSSE(w, "data: not-json\n\n", "id: 2\ndata: {\"id\":\"e2\",\"ledger\":6}\n\n")
	})

	contract := "CABC"
	var got []string
	err := client.StreamEvents(context.Background(), &contract, func(e *Event) error {
		got = append(got, fmt.Sprintf("%s@%d", e.ID, e.Ledger))
		return nil
	})
	if err != nil {
		t.Fatalf("StreamEvents returned %v", err)
	}
	if strings.Join(got, ",") != "e1@5,e2@6" {
		t.Fatalf("events = %v", got)
	}
	if gotPath != "/v1/events/stream?contract_id=CABC" || gotKey != "sk_test" || gotAccept != "text/event-stream" {
		t.Fatalf("request = %q key=%q accept=%q", gotPath, gotKey, gotAccept)
	}
}

func TestStreamEventsStopsOnHandlerError(t *testing.T) {
	client := sseServer(t, func(w http.ResponseWriter, r *http.Request) {
		writeSSE(w, "data: {\"id\":\"e1\"}\n\n", "data: {\"id\":\"e2\"}\n\n")
	})
	stop := errors.New("stop")
	calls := 0
	err := client.StreamEvents(context.Background(), nil, func(*Event) error { calls++; return stop })
	if !errors.Is(err, stop) || calls != 1 {
		t.Fatalf("err = %v, calls = %d", err, calls)
	}
}

func TestStreamEventsHonoursContextCancel(t *testing.T) {
	client := sseServer(t, func(w http.ResponseWriter, r *http.Request) {
		writeSSE(w, "data: {\"id\":\"e1\"}\n\n")
		<-r.Context().Done()
	})
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() {
		done <- client.StreamEvents(ctx, nil, func(*Event) error { cancel(); return nil })
	}()
	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("err = %v, want context.Canceled", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("StreamEvents did not return after cancel")
	}
}

func TestStreamEventsReportsHTTPErrors(t *testing.T) {
	client := sseServer(t, func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, `{"error":"bad contract"}`, http.StatusBadRequest)
	})
	err := client.StreamEvents(context.Background(), nil, func(*Event) error { return nil })
	if err == nil || !strings.Contains(err.Error(), "400") {
		t.Fatalf("err = %v, want status 400", err)
	}
}
