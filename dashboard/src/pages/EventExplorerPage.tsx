import { useState, useEffect, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { dashboardApi, type Event, type EventListResponse, type EventFilterParams } from "../api/client";
import { EventFilterBar } from "../components/EventFilterBar";
import { EventTable } from "../components/EventTable";
import { EmptyState } from "../components/EmptyState";
import { EventDrawer } from "../components/EventDrawer";

export function EventExplorerPage() {
  const navigate = useNavigate();
  const [events, setEvents] = useState<Event[]>([]);
  const [loading, setLoading] = useState(true);
  const [total, setTotal] = useState(0);
  const [filters, setFilters] = useState<EventFilterParams>({ page: 1, limit: 20 });
  const [selectedEvent, setSelectedEvent] = useState<Event | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const loadEvents = useCallback(async () => {
    setLoading(true);
    try {
      const response = await dashboardApi.listEvents(filters);
      setEvents(response.events);
      setTotal(response.total);
    } catch {
      setEvents([]);
      setTotal(0);
    } finally {
      setLoading(false);
    }
  }, [filters]);

  useEffect(() => {
    loadEvents();
  }, [loadEvents]);

  const handleRowClick = (event: Event) => {
    setSelectedEvent(event);
    setDrawerOpen(true);
  };

  const handleCloseDrawer = () => {
    setDrawerOpen(false);
    setSelectedEvent(null);
  };

  const totalPages = Math.ceil(total / (filters.limit ?? 20));
  const currentPage = filters.page ?? 1;

  return (
    <div className="event-explorer">
      <h1>Event Explorer</h1>

      <EventFilterBar
        filters={filters}
        onChange={(newFilters) =>
          setFilters((prev) => ({ ...prev, ...newFilters, page: 1 }))
        }
        onSearch={loadEvents}
      />

      <div className="event-stats">
        <span>Total events: {total.toLocaleString()}</span>
        <span>Showing {events.length} of {total.toLocaleString()}</span>
      </div>

      {loading ? (
        <p>Loading events…</p>
      ) : events.length === 0 ? (
        <EmptyState
          message="No events match the current filters."
          actionLabel="Clear Filters"
          onAction={() => setFilters({ page: 1, limit: 20 })}
        />
      ) : (
        <>
          <EventTable events={events} onRowClick={handleRowClick} />

          <div className="event-pagination">
            <button
              className="btn btn-secondary"
              disabled={currentPage <= 1}
              onClick={() => setFilters((prev) => ({ ...prev, page: (prev.page ?? 1) - 1 }))}
            >
              ← Previous
            </button>
            <span className="event-pagination-info">
              Page {currentPage} of {totalPages}
            </span>
            <button
              className="btn btn-secondary"
              disabled={currentPage >= totalPages}
              onClick={() => setFilters((prev) => ({ ...prev, page: (prev.page ?? 1) + 1 }))}
            >
              Next →
            </button>
          </div>
        </>
      )}

      {/* Drawer for desktop, full page for mobile */}
      <EventDrawer event={selectedEvent} onClose={handleCloseDrawer} />
    </div>
  );
}
