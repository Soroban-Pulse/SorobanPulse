import { ReactNode, useId } from "react";
import { useTranslation } from "react-i18next";

export interface Column<T> {
  id: string;
  header: string;
  cell: (row: T) => ReactNode;
  /** Key columns are kept on the mobile card; the rest are table-only. */
  key?: boolean;
}

interface ResponsiveTableProps<T> {
  caption: string;
  columns: Column<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  emptyText: string;
  /** Makes the first column a button that opens the row's detail view. */
  onOpen?: (row: T) => void;
  openLabel?: (row: T) => string;
}

/**
 * Renders a captioned <table> on wide screens and a card list on narrow ones.
 * Only one of the two is displayed (CSS at the mobile breakpoint), so assistive
 * tech sees a single structure at any width.
 */
export function ResponsiveTable<T>({
  caption,
  columns,
  rows,
  rowKey,
  emptyText,
  onOpen,
  openLabel,
}: ResponsiveTableProps<T>) {
  const { t } = useTranslation();
  const captionId = useId();

  if (rows.length === 0) return <p className="empty-text">{emptyText}</p>;

  const [first, ...rest] = columns;
  const cardColumns = rest.filter((c) => c.key);

  const renderPrimary = (row: T) =>
    onOpen ? (
      <button
        type="button"
        className="link-button"
        aria-label={openLabel?.(row)}
        onClick={() => onOpen(row)}
      >
        {first.cell(row)}
      </button>
    ) : (
      first.cell(row)
    );

  return (
    <>
      <div className="table-scroll" role="region" aria-labelledby={captionId} tabIndex={0}>
        <table>
          <caption id={captionId}>{caption}</caption>
          <thead>
            <tr>
              {columns.map((column) => (
                <th key={column.id} scope="col">
                  {column.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={rowKey(row)}>
                <th scope="row">{renderPrimary(row)}</th>
                {rest.map((column) => (
                  <td key={column.id}>{column.cell(row)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <ul className="card-list" aria-label={caption}>
        {rows.map((row) => (
          <li key={rowKey(row)} className="card">
            <div className="card-title">
              <span className="card-label">{first.header}</span>
              <span className="card-primary">{first.cell(row)}</span>
            </div>
            <dl className="card-fields">
              {cardColumns.map((column) => (
                <div key={column.id} className="card-field">
                  <dt>{column.header}</dt>
                  <dd>{column.cell(row)}</dd>
                </div>
              ))}
            </dl>
            {onOpen && (
              <button type="button" className="card-action" aria-label={openLabel?.(row)} onClick={() => onOpen(row)}>
                {t("common.viewDetails")}
              </button>
            )}
          </li>
        ))}
      </ul>
    </>
  );
}
