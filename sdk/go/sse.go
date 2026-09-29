package soroban_pulse

import (
	"bufio"
	"io"
	"strings"
)

// sseMessage is one dispatched Server-Sent Events message.
type sseMessage struct {
	ID    string
	Event string
	Data  string
}

// sseReader incrementally parses a text/event-stream body following the
// WHATWG event-stream rules used by /v1/events/stream: `data:` lines are
// joined with "\n", lines starting with ":" are comments (keep-alives),
// a blank line dispatches the message, and `retry:` / unknown fields are ignored.
type sseReader struct {
	r *bufio.Reader
	// LastEventID is the most recent `id:` seen, for Last-Event-ID resume.
	LastEventID string
}

func newSSEReader(body io.Reader) *sseReader {
	return &sseReader{r: bufio.NewReader(body)}
}

// Next returns the next message that carries data. It returns io.EOF when the
// stream ends; a message that was not terminated by a blank line is dropped.
func (s *sseReader) Next() (sseMessage, error) {
	var (
		data    []string
		event   string
		id      string
		hasID   bool
		hasData bool
	)
	for {
		line, err := s.r.ReadString('\n')
		if err != nil && (err != io.EOF || line == "") {
			return sseMessage{}, err
		}
		line = strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r")

		if line == "" {
			if hasID {
				s.LastEventID = id
			}
			if hasData {
				if event == "" {
					event = "message"
				}
				return sseMessage{ID: id, Event: event, Data: strings.Join(data, "\n")}, nil
			}
			data, event, id, hasID = nil, "", "", false
			continue
		}
		if strings.HasPrefix(line, ":") {
			continue
		}

		field, value, found := strings.Cut(line, ":")
		if found {
			value = strings.TrimPrefix(value, " ")
		}
		switch field {
		case "data":
			data = append(data, value)
			hasData = true
		case "event":
			event = value
		case "id":
			if !strings.ContainsRune(value, 0) {
				id, hasID = value, true
			}
		}

		if err == io.EOF {
			return sseMessage{}, io.EOF
		}
	}
}
