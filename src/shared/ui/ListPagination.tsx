import type { ReactNode } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useI18n } from "@/shared/i18n";

export const LIST_PAGE_SIZE = 6;

interface Props {
  page: number;
  count: number;
  totalCount: number;
  unit: string;
  label: string;
  disabled?: boolean;
  onPage: (page: number) => void;
  children?: ReactNode;
}

export function ListPagination({ page, count, totalCount, unit, label, disabled = false, onPage, children }: Props) {
  const { t } = useI18n();
  const pages = Math.max(1, Math.ceil(count / LIST_PAGE_SIZE));
  const current = Math.min(page, pages);
  const range = count ? `${(current - 1) * LIST_PAGE_SIZE + 1} - ${Math.min(current * LIST_PAGE_SIZE, count)} / ${count}` : "0";

  return <div className="connection-list-summary">
    <span role="status" aria-live="polite" aria-atomic="true">{range} {unit}{count !== totalCount && t("ui.pagination.total", { total: totalCount, unit })}</span>
    <div className="connection-summary-actions">
      {children}
      {pages > 1 && <nav className="list-pagination" aria-label={label}>
        <button type="button" className="icon-button" title={t("ui.pagination.previous")} aria-label={t("ui.pagination.previous")} disabled={disabled || current <= 1} onClick={() => onPage(current - 1)}><ChevronLeft size={16} /></button>
        <span aria-label={t("ui.pagination.pageOf", { current, pages })}>{current} / {pages}</span>
        <button type="button" className="icon-button" title={t("ui.pagination.next")} aria-label={t("ui.pagination.next")} disabled={disabled || current >= pages} onClick={() => onPage(current + 1)}><ChevronRight size={16} /></button>
      </nav>}
    </div>
  </div>;
}
